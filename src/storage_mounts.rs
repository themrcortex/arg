use serde_json::{json, Value};
use std::{
    collections::HashSet,
    ffi::CString,
    fs,
    os::unix::ffi::OsStrExt,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

static INVENTORY_CACHE: OnceLock<Mutex<Option<(Instant, Value)>>> = OnceLock::new();

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
    let block = stat.f_frsize as u64;
    let total = (stat.f_blocks as u64).saturating_mul(block);
    let free_all = (stat.f_bfree as u64).saturating_mul(block);
    let available = (stat.f_bavail as u64).saturating_mul(block);
    let used = total.saturating_sub(free_all);
    let reserved = total.saturating_sub(used).saturating_sub(available);
    Some((total, used, available, reserved))
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
        let stats = if counts_as_local {
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
    use super::{filesystem_kind, unescape_mount_field};

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
}
