use super::*;

static PROCESS_INSTANCE_ID: OnceLock<String> = OnceLock::new();

pub(super) fn meminfo() -> HashMap<String, u64> {
    fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((
                name.to_owned(),
                value.split_whitespace().next()?.parse().ok()?,
            ))
        })
        .collect()
}
pub(super) fn os_pretty_name() -> String {
    fs::read_to_string("/etc/os-release")
        .unwrap_or_default()
        .lines()
        .find_map(|line| {
            line.strip_prefix("PRETTY_NAME=")
                .map(|value| value.trim_matches('"').to_owned())
        })
        .unwrap_or_else(|| "Unknown Linux distribution".into())
}
pub(super) fn cpu_model_name() -> Option<String> {
    fs::read_to_string("/proc/cpuinfo")
        .ok()?
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| matches!(key.trim(), "model name" | "Hardware" | "Processor"))
                .map(|(_, value)| value.trim().to_owned())
        })
        .filter(|value| !value.is_empty())
}
pub(super) fn is_public_ip(value: &str) -> bool {
    let Ok(ip) = value.parse::<std::net::IpAddr>() else {
        return false;
    };
    match ip {
        std::net::IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_broadcast()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 192 && b == 0 && c == 2)
                && !(a == 198 && (b == 18 || b == 19))
                && !(a == 198 && b == 51 && c == 100)
                && !(a == 203 && b == 0 && c == 113)
        }
        std::net::IpAddr::V6(ip) => {
            let segments = ip.segments();
            !ip.is_loopback()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_unicast_link_local()
                && (segments[0] & 0xfe00) != 0xfc00
                && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
        }
    }
}
pub(super) fn network() -> Vec<Value> {
    let mut result = Vec::new();
    let mut addresses: HashMap<String, Vec<String>> = HashMap::new();
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for interface in interfaces {
            addresses
                .entry(interface.name)
                .or_default()
                .push(interface.addr.ip().to_string());
        }
    }
    if let Ok(entries) = fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            let read = |file: &str| {
                fs::read_to_string(path.join(file))
                    .unwrap_or_default()
                    .trim()
                    .to_owned()
            };
            let counter = |key: &str| read(key).parse::<u64>().unwrap_or(0);
            result.push(json!({
                "name": name,
                "state": read("operstate"),
                "mac": read("address"),
                "mtu": read("mtu").parse::<u64>().ok(),
                "speed_mbps": read("speed").parse::<u64>().ok(),
                "carrier": read("carrier") == "1",
                "rx_bytes": counter("statistics/rx_bytes"),
                "tx_bytes": counter("statistics/tx_bytes"),
                "rx_packets": counter("statistics/rx_packets"),
                "tx_packets": counter("statistics/tx_packets"),
                "rx_errors": counter("statistics/rx_errors"),
                "tx_errors": counter("statistics/tx_errors"),
                "rx_dropped": counter("statistics/rx_dropped"),
                "tx_dropped": counter("statistics/tx_dropped"),
                "addresses": addresses.remove(&name).unwrap_or_default()
            }));
        }
    }
    result
}
pub(super) fn cpu_ticks() -> (u64, u64) {
    let stat = fs::read_to_string("/proc/stat").unwrap_or_default();
    let values: Vec<u64> = stat
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .skip(1)
        .filter_map(|x| x.parse().ok())
        .collect();
    (
        values.iter().sum(),
        values.get(3).copied().unwrap_or(0) + values.get(4).copied().unwrap_or(0),
    )
}
static PROCESS_CPU_SAMPLE: OnceLock<Mutex<(u64, Instant)>> = OnceLock::new();
pub(super) fn process_usage() -> (u64, f64) {
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(1) as u64;
    let resident_pages = fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|value| value.split_whitespace().nth(1)?.parse::<u64>().ok())
        .unwrap_or(0);
    let rss_bytes = resident_pages.saturating_mul(page_size);

    let ticks = fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|value| {
            let end = value.rfind(')')?;
            let fields: Vec<_> = value[end + 1..].split_whitespace().collect();
            Some(
                fields
                    .get(11)?
                    .parse::<u64>()
                    .ok()?
                    .saturating_add(fields.get(12)?.parse::<u64>().ok()?),
            )
        })
        .unwrap_or(0);
    let now = Instant::now();
    let sample = PROCESS_CPU_SAMPLE.get_or_init(|| Mutex::new((ticks, now)));
    let cpu_percent = sample
        .lock()
        .map(|mut previous| {
            let elapsed = now.duration_since(previous.1).as_secs_f64();
            let delta = ticks.saturating_sub(previous.0);
            previous.0 = ticks;
            previous.1 = now;
            let ticks_per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
            let cores = std::thread::available_parallelism()
                .map(|value| value.get())
                .unwrap_or(1) as f64;
            if elapsed > 0.0 {
                ((delta as f64 / ticks_per_second / elapsed / cores) * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            }
        })
        .unwrap_or(0.0);
    (rss_bytes, cpu_percent)
}
pub(super) fn process_instance_id() -> &'static str {
    PROCESS_INSTANCE_ID.get_or_init(|| {
        let start_ticks = fs::read_to_string("/proc/self/stat")
            .ok()
            .and_then(|stat| stat.rsplit_once(')').map(|(_, rest)| rest.to_owned()))
            .and_then(|rest| rest.split_whitespace().nth(19).map(str::to_owned))
            .unwrap_or_else(|| now().to_string());
        format!("{}-{start_ticks}", std::process::id())
    })
}
#[cfg(unix)]
pub(super) fn disk_usage() -> (u64, u64, u64) {
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let root = std::ffi::CString::new("/").unwrap();
    if unsafe { libc::statvfs(root.as_ptr(), &mut stat) } == 0 {
        let total = stat.f_blocks.saturating_mul(stat.f_frsize);
        let used = total.saturating_sub(stat.f_bfree.saturating_mul(stat.f_frsize));
        let available = stat.f_bavail.saturating_mul(stat.f_frsize);
        (total, used, available)
    } else {
        (0, 0, 0)
    }
}
#[cfg(not(unix))]
pub(super) fn disk_usage() -> (u64, u64, u64) {
    (0, 0, 0)
}
pub(super) fn listening_ports() -> Vec<Value> {
    let mut ports = Vec::new();
    for (file, protocol, ipv6) in [
        ("/proc/net/tcp", "TCP", false),
        ("/proc/net/tcp6", "TCP", true),
        ("/proc/net/udp", "UDP", false),
        ("/proc/net/udp6", "UDP", true),
    ] {
        if let Ok(data) = fs::read_to_string(file) {
            for line in data.lines().skip(1) {
                let parts: Vec<_> = line.split_whitespace().collect();
                if parts.len() < 4 || (protocol == "TCP" && parts[3] != "0A") {
                    continue;
                }
                if let Some(port) = parts[1]
                    .split(':')
                    .nth(1)
                    .and_then(|p| u16::from_str_radix(p, 16).ok())
                {
                    ports.push(json!({"port": port, "protocol": protocol, "ipv6": ipv6}));
                }
            }
        }
    }
    ports.sort_by_key(|p| p["port"].as_u64().unwrap_or(0));
    ports.dedup();
    ports
}
pub(super) fn network_totals() -> (u64, u64) {
    let mut rx = 0u64;
    let mut tx = 0u64;
    if let Ok(entries) = fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            if entry.file_name() == "lo" {
                continue;
            }
            let path = entry.path().join("statistics");
            rx = rx.saturating_add(
                fs::read_to_string(path.join("rx_bytes"))
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0),
            );
            tx = tx.saturating_add(
                fs::read_to_string(path.join("tx_bytes"))
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0),
            );
        }
    }
    (rx, tx)
}

pub(super) fn network_interface_counters() -> HashMap<String, (u64, u64, bool)> {
    let mut totals = HashMap::new();
    if let Ok(entries) = fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "lo" {
                continue;
            }
            let path = entry.path();
            let read_counter = |key: &str| {
                fs::read_to_string(path.join("statistics").join(key))
                    .ok()
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .unwrap_or(0)
            };
            let rx = read_counter("rx_bytes");
            let tx = read_counter("tx_bytes");
            let virtual_name = [
                "veth", "docker", "br-", "virbr", "cni", "flannel", "podman", "lxc", "zt",
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix));
            let hardware_like = ["eth", "en", "eno", "ens", "enp", "wlan", "bond"]
                .iter()
                .any(|prefix| name.starts_with(prefix));
            let eligible = !path.join("master").exists()
                && !virtual_name
                && (path.join("device").exists() || hardware_like);
            totals.insert(name, (rx, tx, eligible));
        }
    }
    totals
}

pub(super) fn split_traffic_delta(from: i64, to: i64, rx: u64, tx: u64) -> Vec<(i64, u64, u64)> {
    if from < 0 || to <= from {
        return Vec::new();
    }
    let duration = (to - from) as u64;
    let mut cursor = from;
    let mut rx_used = 0u64;
    let mut tx_used = 0u64;
    let mut result = Vec::new();
    while cursor < to {
        let bucket = cursor.div_euclid(3600) * 3600;
        let end = to.min(bucket + 3600);
        let span = (end - cursor) as u64;
        let is_last = end == to;
        let rx_part = if is_last {
            rx.saturating_sub(rx_used)
        } else {
            ((rx as u128 * span as u128) / duration as u128) as u64
        };
        let tx_part = if is_last {
            tx.saturating_sub(tx_used)
        } else {
            ((tx as u128 * span as u128) / duration as u128) as u64
        };
        result.push((bucket, rx_part, tx_part));
        rx_used = rx_used.saturating_add(rx_part);
        tx_used = tx_used.saturating_add(tx_part);
        cursor = end;
    }
    result
}
