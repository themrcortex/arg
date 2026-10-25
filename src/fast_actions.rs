use super::*;

static CUSTOM_ACTIONS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

#[derive(Clone, Serialize)]
struct FastAction {
    id: i64,
    title: String,
    operation: String,
    target: String,
    site_name: Option<String>,
    show_execution_logs: bool,
    include_error_log: bool,
}

#[derive(Deserialize)]
pub(super) struct FastActionInput {
    title: String,
    operation: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    site_name: Option<String>,
    #[serde(default, alias = "include_access_log")]
    show_execution_logs: bool,
    #[serde(default)]
    include_error_log: bool,
}

fn read_fast_action(db: &Connection, id: i64) -> Result<FastAction, rusqlite::Error> {
    db.query_row(
        "SELECT id,title,operation,target,site_name,include_access_log,include_error_log FROM fast_actions WHERE id=?1",
        [id],
        |row| Ok(FastAction {
            id: row.get(0)?, title: row.get(1)?, operation: row.get(2)?, target: row.get(3)?,
            site_name: row.get(4)?, show_execution_logs: row.get::<_, i64>(5)? != 0,
            include_error_log: row.get::<_, i64>(6)? != 0,
        }),
    )
}

fn validate_fast_action(input: &FastActionInput) -> Result<(), (StatusCode, Json<Value>)> {
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 48 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "عنوان Fast Action باید بین 1 تا 48 نویسه باشد.",
        ));
    }
    let operation = input.operation.as_str();
    if operation == "custom-command" {
        let command = input.target.trim();
        if command.is_empty() || command.len() > 2048 || command.contains('\0') {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "دستور سفارشی باید بین 1 تا 2048 بایت باشد.",
            ));
        }
    }
    let service_operation = operation.strip_prefix("service-");
    let allowed = ["nginx-test", "nginx-reload", "nginx-restart"];
    if operation != "custom-command"
        && !allowed.contains(&operation)
        && service_operation.map_or(true, |op| !["start", "restart", "reload"].contains(&op))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "این عملیات در فهرست Fast Actionهای امن نیست.",
        ));
    }
    if let Some(op) = service_operation {
        let unit = input.target.trim();
        if !super::valid_service_unit(unit) {
            return Err(error(StatusCode::BAD_REQUEST, "واحد systemd معتبر نیست."));
        }
        if super::protected_service_mutation(unit, op) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "این سرویس برای جلوگیری از قطع دسترسی از Fast Action محافظت می‌شود.",
            ));
        }
    } else if operation != "custom-command" && !input.target.trim().is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "عملیات Nginx به target نیاز ندارد.",
        ));
    }
    if input.show_execution_logs && operation != "custom-command" {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نمایش لاگ‌های اجرا فقط برای اکشن سفارشی در دسترس است.",
        ));
    }
    if input.include_error_log {
        let name = input.site_name.as_deref().unwrap_or_default();
        if name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "برای نمایش لاگ، یک سایت Nginx معتبر انتخاب کن.",
            ));
        }
    }
    Ok(())
}

pub(super) async fn list(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let mut stmt = db.prepare("SELECT id,title,operation,target,site_name,include_access_log,include_error_log FROM fast_actions ORDER BY id").map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(FastAction {
                id: row.get(0)?,
                title: row.get(1)?,
                operation: row.get(2)?,
                target: row.get(3)?,
                site_name: row.get(4)?,
                show_execution_logs: row.get::<_, i64>(5)? != 0,
                include_error_log: row.get::<_, i64>(6)? != 0,
            })
        })
        .map_err(db_error)?;
    Ok(Json(
        json!({"actions": rows.collect::<Result<Vec<_>,_>>().map_err(db_error)?}),
    ))
}

pub(super) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<FastActionInput>,
) -> ApiResult {
    check_origin(&headers)?;
    validate_fast_action(&input)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    db.execute("INSERT INTO fast_actions(title,operation,target,site_name,include_access_log,include_error_log,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?7)", params![input.title.trim(), input.operation, input.target.trim(), input.site_name.as_deref().map(str::trim).filter(|value| !value.is_empty()), input.show_execution_logs as i64, input.include_error_log as i64, now()]).map_err(db_error)?;
    let action = read_fast_action(&db, db.last_insert_rowid()).map_err(db_error)?;
    audit(
        &db,
        &actor,
        "dashboard.fast_action.create",
        &format!("id={} operation={}", action.id, action.operation),
    );
    Ok(Json(json!({"action":action})))
}

pub(super) async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<FastActionInput>,
) -> ApiResult {
    check_origin(&headers)?;
    validate_fast_action(&input)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    read_fast_action(&db, id).map_err(|_| error(StatusCode::NOT_FOUND, "Fast Action پیدا نشد."))?;
    db.execute("UPDATE fast_actions SET title=?1,operation=?2,target=?3,site_name=?4,include_access_log=?5,include_error_log=?6,updated_at=?7 WHERE id=?8", params![input.title.trim(), input.operation, input.target.trim(), input.site_name.as_deref().map(str::trim).filter(|value| !value.is_empty()), input.show_execution_logs as i64, input.include_error_log as i64, now(), id]).map_err(db_error)?;
    let action = read_fast_action(&db, id).map_err(db_error)?;
    audit(
        &db,
        &actor,
        "dashboard.fast_action.update",
        &format!("id={id} operation={}", action.operation),
    );
    Ok(Json(json!({"action":action})))
}

pub(super) async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let action = read_fast_action(&db, id)
        .map_err(|_| error(StatusCode::NOT_FOUND, "Fast Action پیدا نشد."))?;
    db.execute("DELETE FROM fast_actions WHERE id=?1", [id])
        .map_err(db_error)?;
    audit(
        &db,
        &actor,
        "dashboard.fast_action.delete",
        &format!("id={id} operation={}", action.operation),
    );
    Ok(Json(json!({"ok":true})))
}

fn read_capped<R: Read>(mut reader: R, limit: usize) -> io::Result<(String, bool)> {
    let mut output = Vec::with_capacity(limit.min(8192));
    let mut truncated = false;
    let mut buffer = [0u8; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let available = limit.saturating_sub(output.len());
        let keep = available.min(count);
        output.extend_from_slice(&buffer[..keep]);
        truncated |= keep < count;
    }
    Ok((String::from_utf8_lossy(&output).into_owned(), truncated))
}

fn execute_custom_command(command: &str, include_logs: bool) -> Result<Value, String> {
    use std::process::{Command, Stdio};
    let mut child = Command::new("/usr/bin/timeout")
        .args([
            "--signal=TERM",
            "--kill-after=3s",
            "30s",
            "/bin/sh",
            "-c",
            command,
        ])
        .current_dir("/")
        .env_clear()
        .env(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )
        .env("HOME", "/var/lib/arg")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "اجرای ابزار محدودکنندهٔ زمان در دسترس نیست.".to_owned())?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let stdout_reader = std::thread::spawn(move || read_capped(stdout, 64 * 1024));
    let stderr_reader = std::thread::spawn(move || read_capped(stderr, 64 * 1024));
    let status = child
        .wait()
        .map_err(|e| format!("اجرای دستور سفارشی ناموفق بود: {e}"))?;
    let (stdout, stdout_truncated) = stdout_reader
        .join()
        .map_err(|_| "خواندن خروجی دستور ناموفق بود.".to_owned())?
        .map_err(|e| e.to_string())?;
    let (stderr, stderr_truncated) = stderr_reader
        .join()
        .map_err(|_| "خواندن خطای دستور ناموفق بود.".to_owned())?
        .map_err(|e| e.to_string())?;
    let exit_code = status.code().unwrap_or(-1);
    let timed_out = exit_code == 124 || exit_code == 137;
    Ok(json!({
        "success": status.success(), "exit_code": exit_code, "timed_out": timed_out,
        "stdout": if include_logs { stdout } else { String::new() },
        "stderr": if include_logs { stderr } else { String::new() },
        "logs_truncated": include_logs && (stdout_truncated || stderr_truncated),
    }))
}

pub(super) async fn run(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> ApiResult {
    check_origin(&headers)?;
    let (action, actor) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let action = read_fast_action(&db, id)
            .map_err(|_| error(StatusCode::NOT_FOUND, "Fast Action پیدا نشد."))?;
        if action.operation != "custom-command" {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "این اکشن از مسیر اجرای سفارشی پشتیبانی نمی‌کند.",
            ));
        }
        audit(
            &db,
            &actor,
            "dashboard.fast_action.custom.start",
            &format!("id={} title={}", action.id, action.title),
        );
        (action, actor)
    };
    let _permit = CUSTOM_ACTIONS.acquire().await.map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "اجرای اکشن سفارشی فعلاً در دسترس نیست.",
        )
    })?;
    let command = action.target.clone();
    let include_logs = action.show_execution_logs;
    let result =
        tokio::task::spawn_blocking(move || execute_custom_command(&command, include_logs))
            .await
            .map_err(|_| error(StatusCode::BAD_GATEWAY, "فرایند اکشن سفارشی متوقف شد."))?
            .map_err(|detail| error(StatusCode::BAD_GATEWAY, &detail))?;
    {
        let db = state.lock().unwrap();
        audit(
            &db,
            &actor,
            "dashboard.fast_action.custom.finish",
            &format!(
                "id={} exit_code={} timeout={}",
                action.id, result["exit_code"], result["timed_out"]
            ),
        );
    }
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom_input(command: &str) -> FastActionInput {
        FastActionInput {
            title: "Custom task".into(),
            operation: "custom-command".into(),
            target: command.into(),
            site_name: None,
            show_execution_logs: true,
            include_error_log: false,
        }
    }

    #[test]
    fn custom_action_command_is_required_and_bounded() {
        assert!(validate_fast_action(&custom_input("echo ready")).is_ok());
        assert!(validate_fast_action(&custom_input("  ")).is_err());
        assert!(validate_fast_action(&custom_input(&"x".repeat(2049))).is_err());
    }

    #[test]
    fn execution_output_is_capped_but_fully_drained() {
        let (output, truncated) = read_capped("abcdefghij".as_bytes(), 4).unwrap();
        assert_eq!(output, "abcd");
        assert!(truncated);
    }

    #[test]
    fn custom_action_runs_with_optional_execution_logs() {
        let with_logs = execute_custom_command("printf ready", true).unwrap();
        assert_eq!(with_logs["success"], true);
        assert_eq!(with_logs["stdout"], "ready");

        let without_logs = execute_custom_command("printf hidden", false).unwrap();
        assert_eq!(without_logs["success"], true);
        assert_eq!(without_logs["stdout"], "");
    }
}
