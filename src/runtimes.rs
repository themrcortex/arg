use super::*;
use std::{
    ffi::CString,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const MAX_PROBE_OUTPUT: usize = 320;

pub(super) async fn inventory(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let result = privileged_request_async(json!({"action":"runtime-inventory"})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            result["error"]
                .as_str()
                .unwrap_or("Runtime inventory unavailable"),
        ));
    }
    Ok(Json(result["data"].clone()))
}

pub(super) fn execute_inventory() -> Value {
    let os_release = read_os_release(Path::new("/etc/os-release")).unwrap_or_default();
    let os_id = os_release.get("ID").cloned().unwrap_or_default();
    let os_version = os_release.get("VERSION_ID").cloned().unwrap_or_default();
    let supported_os = matches!(os_id.as_str(), "debian" | "ubuntu");
    let memory = read_memory_info(Path::new("/proc/meminfo"));
    let disk_available = available_bytes(deployment_path());
    let apt_sources = apt_sources_present(Path::new("/etc/apt"));

    let php = discover_php();
    let node = discover_node();
    let python = discover_python();
    let tools = vec![
        tool("composer", "Composer", &["--version", "--no-ansi"]),
        tool("npm", "npm", &["--version"]),
        packaged_tool("pnpm", "pnpm", "pnpm"),
        packaged_tool("yarn", "Yarn", "yarnpkg"),
        packaged_tool("corepack", "Corepack", "corepack"),
        tool_at("nginx", "Nginx", Path::new("/usr/sbin/nginx"), &["-v"]),
        tool("docker", "Docker CLI", &["--version"]),
    ];

    let runtimes = vec![php, node, python];
    let installed_count = runtimes
        .iter()
        .filter(|runtime| runtime["state"] == "installed")
        .count();
    json!({
        "platform": {
            "id": os_id,
            "version": os_version,
            "pretty_name": os_release.get("PRETTY_NAME").cloned().unwrap_or_default(),
            "architecture": std::env::consts::ARCH,
            "supported": supported_os,
            "apt_available": Path::new("/usr/bin/apt-get").is_file(),
        },
        "preflight": {
            "apt_sources_present": apt_sources,
            "memory_total_bytes": memory.0,
            "memory_available_bytes": memory.1,
            "deployment_disk_available_bytes": disk_available,
        },
        "runtimes": runtimes,
        "tools": tools,
        "summary": { "runtime_families_installed": installed_count },
        "measured_at": now(),
    })
}

fn read_os_release(path: &Path) -> io::Result<HashMap<String, String>> {
    let mut result = HashMap::new();
    for line in fs::read_to_string(path)?.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !key
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            continue;
        }
        let value = value.trim().trim_matches('"').trim_matches('\'');
        result.insert(key.to_owned(), value.to_owned());
    }
    Ok(result)
}

fn read_memory_info(path: &Path) -> (Option<u64>, Option<u64>) {
    let Ok(contents) = fs::read_to_string(path) else {
        return (None, None);
    };
    let mut total = None;
    let mut available = None;
    for line in contents.lines() {
        let (key, value) = line.split_once(':').unwrap_or(("", ""));
        let bytes = value
            .split_whitespace()
            .next()
            .and_then(|number| number.parse::<u64>().ok())
            .map(|kib| kib.saturating_mul(1024));
        match key {
            "MemTotal" => total = bytes,
            "MemAvailable" => available = bytes,
            _ => {}
        }
    }
    (total, available)
}

fn deployment_path() -> PathBuf {
    env::var_os("ARG_DEPLOY_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/arg/deployments"))
}

fn available_bytes(path: PathBuf) -> Option<u64> {
    let path = path
        .ancestors()
        .find(|candidate| candidate.exists())
        .unwrap_or(Path::new("/var/lib"));
    let path = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::statvfs(path.as_ptr(), &mut stat) };
    (result == 0).then(|| {
        (stat.f_bavail as u128)
            .saturating_mul(stat.f_frsize as u128)
            .min(u64::MAX as u128) as u64
    })
}

fn apt_sources_present(root: &Path) -> bool {
    let mut files = Vec::new();
    files.push(root.join("sources.list"));
    if let Ok(entries) = fs::read_dir(root.join("sources.list.d")) {
        files.extend(entries.flatten().map(|entry| entry.path()));
    }
    files.into_iter().any(|path| {
        fs::read_to_string(path).is_ok_and(|content| {
            content.lines().any(|line| {
                let line = line.trim();
                !line.is_empty() && !line.starts_with('#')
            })
        })
    })
}

fn tool(id: &str, label: &str, args: &[&str]) -> Value {
    let paths = [
        PathBuf::from(format!("/usr/bin/{id}")),
        PathBuf::from(format!("/usr/local/bin/{id}")),
        PathBuf::from(format!("/bin/{id}")),
    ];
    for path in paths {
        if path.is_file() {
            return tool_at(id, label, &path, args);
        }
    }
    json!({"id":id,"label":label,"state":"missing","version":null,"path":null})
}

fn tool_at(id: &str, label: &str, path: &Path, args: &[&str]) -> Value {
    match probe(path, args) {
        Some(version) => {
            json!({"id":id,"label":label,"state":"installed","version":version,"path":path})
        }
        None => json!({"id":id,"label":label,"state":"missing","version":null,"path":null}),
    }
}

fn packaged_tool(id: &str, label: &str, package: &str) -> Value {
    // pnpm/yarn/corepack shims may download a manager on first execution. Read dpkg metadata
    // instead so opening the inventory never causes a network fetch or runs package code.
    let package_info = Command::new("/usr/bin/dpkg-query")
        .args(["-W", "-f=${db:Status-Status}\t${Version}\n", "--", package])
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    if let Some((status, version)) = package_info
        .as_deref()
        .and_then(|line| line.split_once('\t'))
    {
        if status == "installed" && !version.is_empty() {
            return json!({"id":id,"label":label,"state":"installed","version":version,"path":null});
        }
    }
    let paths = [
        PathBuf::from(format!("/usr/bin/{id}")),
        PathBuf::from(format!("/usr/local/bin/{id}")),
        PathBuf::from(format!("/bin/{id}")),
    ];
    if let Some(path) = paths.into_iter().find(|path| path.is_file()) {
        return json!({"id":id,"label":label,"state":"present_unverified","version":null,"path":path});
    }
    json!({"id":id,"label":label,"state":"missing","version":null,"path":null})
}

fn probe(path: &Path, args: &[&str]) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let output = Command::new(path)
        .args(args)
        .env(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )
        .env("HOME", "/root")
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).into_owned()
    } else {
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    text.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().chars().take(MAX_PROBE_OUTPUT).collect())
}

fn binary_names(directory: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

fn version_token(output: &str, family: &str) -> Option<String> {
    let line = output.lines().next()?.trim();
    let rest = match family {
        "php" => line.strip_prefix("PHP ")?,
        "node" => line.strip_prefix('v').unwrap_or(line),
        "python" => line.strip_prefix("Python ")?,
        _ => return None,
    };
    let token: String = rest
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect();
    (token.contains('.') && valid_version(&token)).then_some(token)
}

fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
        && value.split('.').all(|part| !part.is_empty())
}

fn discover_php() -> Value {
    let mut names = binary_names(Path::new("/usr/bin"));
    names.extend(binary_names(Path::new("/usr/local/bin")));
    let mut found = Vec::<(String, String, String)>::new();
    for name in names {
        let is_php = name == "php" || versioned_binary(&name, "php");
        if !is_php {
            continue;
        }
        let path = [
            Path::new("/usr/bin").join(&name),
            Path::new("/usr/local/bin").join(&name),
        ]
        .into_iter()
        .find(|candidate| candidate.is_file());
        let Some(path) = path else { continue };
        let Some(output) = probe(&path, &["-v"]) else {
            continue;
        };
        let Some(version) = version_token(&output, "php") else {
            continue;
        };
        let fpm_binary = format!("php-fpm{version}");
        let fpm_path = [
            Path::new("/usr/sbin").join(&fpm_binary),
            Path::new("/usr/bin").join(&fpm_binary),
        ]
        .into_iter()
        .find(|candidate| candidate.is_file());
        let fpm_version = fpm_path
            .as_ref()
            .and_then(|fpm| probe(fpm, &["-v"]))
            .and_then(|output| version_token(&output, "php"));
        found.push((
            version,
            path.to_string_lossy().into_owned(),
            fpm_version.unwrap_or_default(),
        ));
    }
    found.sort_by(|left, right| left.0.cmp(&right.0));
    found.dedup_by(|left, right| left.0 == right.0);
    let versions: Vec<Value> = found
        .into_iter()
        .map(|(version, cli_path, fpm_version)| {
            let fpm = fpm_version == version;
            let unit_state = if fpm {
                systemd_state(&format!("php{version}-fpm.service"))
            } else {
                "missing".to_owned()
            };
            json!({"version":version,"cli_path":cli_path,"fpm_installed":fpm,"fpm_service":unit_state})
        })
        .collect();
    json!({"family":"php","label":"PHP","state":if versions.is_empty(){"missing"}else{"installed"},"versions":versions})
}

fn discover_node() -> Value {
    let paths = ["/usr/bin/node", "/usr/local/bin/node", "/usr/bin/nodejs"];
    let versions = paths
        .into_iter()
        .filter_map(|path| {
            let path = Path::new(path);
            let output = probe(path, &["--version"])?;
            let version = version_token(&output, "node")?;
            Some(json!({"version":version,"path":path}))
        })
        .collect::<Vec<_>>();
    json!({"family":"node","label":"Node.js","state":if versions.is_empty(){"missing"}else{"installed"},"versions":versions})
}

fn discover_python() -> Value {
    let mut names = binary_names(Path::new("/usr/bin"));
    names.extend(binary_names(Path::new("/usr/local/bin")));
    let mut versions = Vec::new();
    for name in names {
        if !(name == "python3" || versioned_binary(&name, "python3")) {
            continue;
        }
        let path = [
            Path::new("/usr/bin").join(&name),
            Path::new("/usr/local/bin").join(&name),
        ]
        .into_iter()
        .find(|candidate| candidate.is_file());
        let Some(path) = path else { continue };
        let Some(output) = probe(&path, &["--version"]) else {
            continue;
        };
        let Some(version) = version_token(&output, "python") else {
            continue;
        };
        versions.push(json!({"version":version,"path":path}));
    }
    versions.sort_by(|left: &Value, right: &Value| {
        left["version"].as_str().cmp(&right["version"].as_str())
    });
    versions.dedup_by(|left, right| left["version"] == right["version"]);
    json!({"family":"python","label":"Python","state":if versions.is_empty(){"missing"}else{"installed"},"versions":versions})
}

fn versioned_binary(name: &str, prefix: &str) -> bool {
    let Some(suffix) = name.strip_prefix(prefix) else {
        return false;
    };
    valid_version(suffix) && suffix.contains('.')
}

fn systemd_state(unit: &str) -> String {
    let output = Command::new("/usr/bin/systemctl")
        .args(["is-active", unit])
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .stdin(Stdio::null())
        .output();
    match output {
        Ok(output) => {
            let state = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if state.is_empty() {
                "inactive".to_owned()
            } else {
                state
            }
        }
        Err(_) => "unknown".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_parser_does_not_evaluate_shell_syntax() {
        let root = std::env::temp_dir().join(format!("arg-os-release-{}", now()));
        fs::write(
            &root,
            "ID=ubuntu\nVERSION_ID=\"24.04\"\nVALUE=$(touch /tmp/arg-runtime-pwned)\n",
        )
        .unwrap();
        let parsed = read_os_release(&root).unwrap();
        assert_eq!(parsed["ID"], "ubuntu");
        assert_eq!(parsed["VERSION_ID"], "24.04");
        assert_eq!(parsed["VALUE"], "$(touch /tmp/arg-runtime-pwned)");
        assert!(!Path::new("/tmp/arg-runtime-pwned").exists());
        let _ = fs::remove_file(root);
    }

    #[test]
    fn runtime_version_tokens_are_bounded_and_family_specific() {
        assert_eq!(
            version_token("PHP 8.5.1 (cli)", "php").as_deref(),
            Some("8.5.1")
        );
        assert_eq!(version_token("v22.3.0", "node").as_deref(), Some("22.3.0"));
        assert_eq!(
            version_token("Python 3.13.2", "python").as_deref(),
            Some("3.13.2")
        );
        assert!(version_token("PHP 8;touch bad", "php").is_none());
        assert!(!valid_version("8..5"));
        assert!(!valid_version("$(id)"));
    }

    #[test]
    fn inventory_emits_platform_runtime_tool_and_preflight_fields() {
        let inventory = execute_inventory();
        assert!(inventory["platform"]["architecture"].is_string());
        assert!(inventory["preflight"]["apt_sources_present"].is_boolean());
        assert_eq!(inventory["runtimes"].as_array().unwrap().len(), 3);
        let tool_ids: Vec<&str> = inventory["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["id"].as_str())
            .collect();
        assert!(tool_ids.contains(&"composer"));
        assert!(tool_ids.contains(&"pnpm"));
        assert!(tool_ids.contains(&"docker"));
    }

    #[tokio::test]
    async fn runtime_inventory_is_admin_only() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE users(id INTEGER PRIMARY KEY,username TEXT NOT NULL,role TEXT NOT NULL,enabled INTEGER NOT NULL); CREATE TABLE sessions(token_hash TEXT PRIMARY KEY,user_id INTEGER NOT NULL,expires_at INTEGER NOT NULL,last_seen INTEGER);").unwrap();
        let token = "runtime-viewer-session";
        db.execute(
            "INSERT INTO users(id,username,role,enabled) VALUES(1,'viewer','viewer',1)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO sessions(token_hash,user_id,expires_at) VALUES(?1,1,?2)",
            params![session_hash(token), now() + 3600],
        )
        .unwrap();
        let state = Arc::new(Mutex::new(db));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("arg_session={token}").parse().unwrap(),
        );
        let error = inventory(State(state), headers).await.unwrap_err();
        assert_eq!(error.0, StatusCode::FORBIDDEN);
    }
}
