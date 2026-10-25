use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    ffi::CString,
    fs,
    os::unix::ffi::OsStrExt,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

static INVENTORY_CACHE: OnceLock<Mutex<Option<(Instant, Value)>>> = OnceLock::new();
static DISK_IO_CACHE: OnceLock<Mutex<Option<(Instant, HashMap<String, (u64, u64)>)>>> =
    OnceLock::new();

fn parse_diskstats(content: &str) -> HashMap<String, (u64, u64)> {
    content
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 10 {
                return None;
            }
            let key = format!("{}:{}", fields[0], fields[1]);
            // Linux reports sectors in 512-byte units in /proc/diskstats.
            let read = fields[5].parse::<u64>().ok()?.saturating_mul(512);
            let written = fields[9].parse::<u64>().ok()?.saturating_mul(512);
            Some((key, (read, written)))
        })
        .collect()
}

fn disk_io_rates() -> HashMap<String, (Option<f64>, Option<f64>)> {
    let current = fs::read_to_string("/proc/diskstats")
        .map(|value| parse_diskstats(&value))
        .unwrap_or_default();
    let cache = DISK_IO_CACHE.get_or_init(|| Mutex::new(None));
    let Ok(mut previous) = cache.lock() else {
        return HashMap::new();
    };
    let now = Instant::now();
    let rates = previous
        .as_ref()
        .map(|(at, before)| {
            let elapsed = now.duration_since(*at).as_secs_f64().max(0.001);
            current
                .iter()
                .map(|(device, (read, written))| {
                    let rate = before.get(device).map(|(old_read, old_written)| {
                        (
                            read.checked_sub(*old_read).map(|delta| delta as f64 / elapsed),
                            written
                                .checked_sub(*old_written)
                                .map(|delta| delta as f64 / elapsed),
                        )
                    });
                    (device.clone(), rate.unwrap_or((None, None)))
                })
                .collect()
        })
        .unwrap_or_default();
    *previous = Some((now, current));
    rates
}

pub(super) fn root_disk_io_counters() -> Option<(u64, u64)> {
    let mountinfo = fs::read_to_string("/proc/self/mountinfo").ok()?;
    let device = mountinfo.lines().find_map(|line| {
        let (left, _) = line.split_once(" - ")?;
        let fields: Vec<_> = left.split_whitespace().collect();
        (fields.get(4).copied() == Some("/")).then(|| fields.get(2).copied().unwrap_or("").to_owned())
    })?;
    parse_diskstats(&fs::read_to_string("/proc/diskstats").ok()?).get(&device).copied()
}

fn unescape_mount_field(value: &str) -> String {
    value
        .replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

fn filesystem_kind(filesystem: &str, mount_point: &str) -> &'static str {
    let fs = filesystem.to_ascii_lowercase();
    if fs.starts_with("fuse.rclone")
        || fs.starts_with("fuse.s3fs")
        || mount_point.starts_with("/mnt/arg-s3/")
    {
        "cloud"
    } else if [
        "nfs",
        "nfs4",
        "cifs",
        "smb3",
        "sshfs",
        "9p",
        "ceph",
        "glusterfs",
    ]
    .contains(&fs.as_str())
    {
        "remote"
    } else if [
        "proc",
        "sysfs",
        "devtmpfs",
        "devpts",
        "tmpfs",
        "cgroup",
        "cgroup2",
        "overlay",
        "squashfs",
        "tracefs",
        "securityfs",
        "pstore",
        "efivarfs",
        "bpf",
        "autofs",
        "mqueue",
        "hugetlbfs",
        "debugfs",
        "fusectl",
        "configfs",
        "ramfs",
    ]
    .contains(&fs.as_str())
    {
        "virtual"
    } else {
        "local"
    }
}

fn stat_filesystem(path: &str) -> Option<(u64, u64, u64, u64)> {
    let path = CString::new(Path::new(path).as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    Some(capacity_values(
        stat.f_blocks as u64,
        stat.f_bfree as u64,
        stat.f_bavail as u64,
        stat.f_frsize as u64,
    ))
}

fn capacity_values(
    blocks: u64,
    free_blocks: u64,
    available_blocks: u64,
    block_size: u64,
) -> (u64, u64, u64, u64) {
    let total = blocks.saturating_mul(block_size);
    let free_all = free_blocks.saturating_mul(block_size);
    let available = available_blocks.saturating_mul(block_size);
    let used = total.saturating_sub(free_all);
    let reserved = total.saturating_sub(used).saturating_sub(available);
    (total, used, available, reserved)
}

/// Inventory mounted filesystems without summing pseudo, remote or cloud filesystems into local disk totals.
pub(super) fn inventory() -> Value {
    let cache = INVENTORY_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((at, value)) = guard.as_ref() {
            if at.elapsed() < Duration::from_secs(15) {
                return value.clone();
            }
        }
    }
    let value = build_inventory();
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((Instant::now(), value.clone()));
    }
    value
}

fn build_inventory() -> Value {
    let measured_at = crate::now();
    let content = fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let mut volumes = Vec::new();
    let mut counted_devices = HashSet::new();
    let (mut total, mut used, mut available, mut reserved) = (0u64, 0u64, 0u64, 0u64);
    let mut aggregation_reliable = true;
    let io_rates = disk_io_rates();

    for line in content.lines() {
        let Some((before, after)) = line.split_once(" - ") else {
            continue;
        };
        let left: Vec<_> = before.split_whitespace().collect();
        let right: Vec<_> = after.split_whitespace().collect();
        if left.len() < 6 || right.len() < 2 {
            continue;
        }
        let device_id = left[2];
        let mount_point = unescape_mount_field(left[4]);
        let filesystem = right[0];
        let device = unescape_mount_field(right[1]);
        let kind = filesystem_kind(filesystem, &mount_point);
        let read_only = left[5].split(',').any(|option| option == "ro");
        let is_virtual = kind == "virtual";
        let is_remote = kind == "remote";
        let is_cloud = kind == "cloud";
        let counts_as_local = kind == "local";
        let stats = if counts_as_local || mount_point == "/" {
            stat_filesystem(&mount_point)
        } else {
            None
        };
        if counts_as_local && stats.is_none() {
            aggregation_reliable = false;
        }
        if counts_as_local {
            if let Some(values) = stats {
                // Multiple bind mounts of the same block filesystem must not inflate the total.
                if counted_devices.insert(device_id.to_owned()) {
                    total = total.saturating_add(values.0);
                    used = used.saturating_add(values.1);
                    available = available.saturating_add(values.2);
                    reserved = reserved.saturating_add(values.3);
                } else {
                    // Shared block devices (for example, Btrfs subvolumes) cannot be
                    // safely added without double-counting their shared free space.
                    aggregation_reliable = false;
                }
            }
        }
        let (total_bytes, used_bytes, available_bytes, reserved_bytes) = stats
            .map(|values| {
                (
                    Some(values.0),
                    Some(values.1),
                    Some(values.2),
                    Some(values.3),
                )
            })
            .unwrap_or((None, None, None, None));
        let use_percent = total_bytes
            .filter(|value| *value > 0)
            .zip(used_bytes)
            .map(|(size, usage)| ((usage as f64 / size as f64) * 100.0).clamp(0.0, 100.0));
        let (read_bytes_per_sec, write_bytes_per_sec) = io_rates
            .get(device_id)
            .cloned()
            .unwrap_or((None, None));
        let is_mount = Path::new(&mount_point).exists();
        volumes.push(json!({
            "id": format!("{}:{}:{}", device_id, filesystem, mount_point),
            "device": device,
            "device_id": device_id,
            "mount_point": mount_point,
            "filesystem": filesystem,
            "kind": kind,
            "total_bytes": total_bytes,
            "used_bytes": used_bytes,
            "available_bytes": available_bytes,
            "reserved_bytes": reserved_bytes,
            "use_percent": use_percent,
            "read_bytes_per_sec": read_bytes_per_sec,
            "write_bytes_per_sec": write_bytes_per_sec,
            "read_only": read_only,
            "mount_status": if is_mount { "mounted" } else { "unknown" },
            "is_remote": is_remote,
            "is_virtual": is_virtual,
            "is_cloud": is_cloud,
            "measured_at": if stats.is_some() { Some(measured_at) } else { None },
            "error": if counts_as_local && stats.is_none() { Some("Could not read filesystem capacity") } else { None }
        }));
    }
    volumes.sort_by(|a, b| {
        let rank = |kind: &str| match kind {
            "local" => 0,
            "cloud" => 1,
            "remote" => 2,
            _ => 3,
        };
        rank(a["kind"].as_str().unwrap_or("virtual"))
            .cmp(&rank(b["kind"].as_str().unwrap_or("virtual")))
            .then_with(|| {
                a["mount_point"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(b["mount_point"].as_str().unwrap_or(""))
            })
    });
    json!({
        "volumes": volumes,
        "local": {
            "total_bytes": total,
            "used_bytes": used,
            "available_bytes": available,
            "reserved_bytes": reserved,
            "aggregation_reliable": aggregation_reliable,
            "aggregation_basis": "unique block device (major:minor)"
        },
        "measured_at": measured_at
    })
}

#[cfg(test)]
mod tests {
    use super::{capacity_values, filesystem_kind, parse_diskstats, unescape_mount_field};

    #[test]
    fn capacity_separates_used_available_and_reserved_blocks() {
        // 100 blocks total, 30 free overall and 20 available to unprivileged users.
        assert_eq!(
            capacity_values(100, 30, 20, 4096),
            (409_600, 286_720, 81_920, 40_960)
        );
    }

    #[test]
    fn mount_fields_decode_kernel_escapes() {
        assert_eq!(unescape_mount_field("/mnt/data\\040disk"), "/mnt/data disk");
    }

    #[test]
    fn remote_and_virtual_mounts_are_not_local_disk() {
        assert_eq!(
            filesystem_kind("fuse.rclone", "/mnt/arg-s3/backup"),
            "cloud"
        );
        assert_eq!(filesystem_kind("fuse.s3fs", "/mnt/archive"), "cloud");
        assert_eq!(filesystem_kind("nfs4", "/mnt/nas"), "remote");
        assert_eq!(filesystem_kind("overlay", "/"), "virtual");
        assert_eq!(filesystem_kind("ext4", "/"), "local");
    }

    #[test]
    fn diskstats_sectors_are_reported_as_bytes_and_keyed_by_device_id() {
        let stats = parse_diskstats("  8  1 sda1 10 0 42 2 20 0 84 3 0 0 0");
        assert_eq!(stats.get("8:1"), Some(&(42 * 512, 84 * 512)));
    }
}
