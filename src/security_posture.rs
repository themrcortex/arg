use super::*;

const TOOLS: &[(&str, &str, &[&str])] = &[
    ("fail2ban", "Fail2ban CLI", &["fail2ban-client"]),
    ("lynis", "Lynis audit tool", &["lynis"]),
    ("clamav", "ClamAV scanner", &["clamscan"]),
    ("rkhunter", "Rootkit Hunter", &["rkhunter"]),
    ("chkrootkit", "chkrootkit scanner", &["chkrootkit"]),
    ("nftables", "nftables CLI", &["nft"]),
];

fn find_executable(dirs: &[PathBuf], names: &[&str]) -> Option<PathBuf> {
    dirs.iter()
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|path| {
            fs::metadata(path).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
}

pub(super) async fn inventory(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let dirs = ["/usr/bin", "/usr/sbin", "/usr/local/bin", "/usr/local/sbin"]
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let tools = TOOLS.iter().map(|(id, label, candidates)| {
        let path = find_executable(&dirs, candidates);
        json!({"id":id,"label":label,"binary_available":path.is_some(),"path":path,"active_state":null,"version":null})
    }).collect::<Vec<_>>();
    Ok(Json(json!({
        "tools": tools,
        "measured_at": now(),
        "inspection": "binary-presence-only",
        "note": "وجود فایل اجرایی بررسی شده است؛ فعال‌بودن سرویس، نصب Rule، پوشش یا امن‌بودن پیکربندی را ثابت نمی‌کند. هیچ ابزاری اجرا یا نصب نشد."
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::PermissionsExt,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn only_executable_regular_files_are_detected() {
        let root = std::env::temp_dir().join(format!(
            "arg-security-posture-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let binary = root.join("scanner");
        fs::write(&binary, b"placeholder").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            find_executable(std::slice::from_ref(&root), &["scanner"]),
            Some(binary.clone())
        );
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(find_executable(std::slice::from_ref(&root), &["scanner"]).is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
