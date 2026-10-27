use super::*;
use std::process::{Command, Output};

fn run_bounded(program: &str, args: &[&str]) -> Option<Output> {
    Command::new("/usr/bin/timeout")
        .arg("4s")
        .arg(program)
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .env("PGCONNECT_TIMEOUT", "2")
        .output()
        .ok()
}

fn stdout(output: Option<Output>) -> Option<String> {
    output
        .filter(|value| value.status.success())
        .map(|value| {
            String::from_utf8_lossy(&value.stdout)
                .trim()
                .chars()
                .take(256)
                .collect()
        })
        .filter(|value: &String| !value.is_empty())
}

pub(super) async fn status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let result = tokio::task::spawn_blocking(|| {
        let psql_path = "/usr/bin/psql";
        let installed = std::path::Path::new(psql_path).is_file();
        let version = if installed {
            stdout(run_bounded(psql_path, &["--version"]))
        } else {
            None
        };
        let unit = stdout(run_bounded(
            "/usr/bin/systemctl",
            &["show", "postgresql.service", "--property=ActiveState", "--value", "--no-pager"],
        ));
        let readiness = if installed {
            let ready = run_bounded("/usr/bin/pg_isready", &["--timeout=2"]);
            ready.map(|output| {
                if output.status.success() {
                    "accepting_connections"
                } else if output.status.code() == Some(2) {
                    "no_response"
                } else {
                    "not_accepting"
                }
            })
        } else {
            None
        };
        json!({
            "installed": installed,
            "version": version,
            "service_state": unit.as_deref().unwrap_or("unknown"),
            "readiness": readiness.unwrap_or("unknown"),
            "management_mode": "inventory_only",
            "sql_console_enabled": false,
            "note": "این نسخه فقط وضعیت واقعی PostgreSQL را می‌خواند؛ هنوز SQL یا تغییر نقش/داده را اجرا نمی‌کند."
        })
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "خواندن وضعیت PostgreSQL متوقف شد."))?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    #[test]
    fn output_is_bounded_and_trimmed() {
        let output = Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: vec![b'a'; 400],
            stderr: Vec::new(),
        };
        assert_eq!(stdout(Some(output)).unwrap().len(), 256);
    }
}
