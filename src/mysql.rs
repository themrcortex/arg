use super::*;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

#[derive(Deserialize)]
pub(super) struct InstallRequest {
    provider: String,
    admin_password: String,
    confirmation: String,
}
#[derive(Deserialize)]
pub(super) struct ServiceRequest {
    action: String,
    admin_password: String,
    #[serde(default)]
    confirmed: bool,
}
#[derive(Deserialize)]
pub(super) struct CredentialsRequest {
    admin_password: String,
    mysql_user: String,
    mysql_password: String,
}
#[derive(Deserialize)]
pub(super) struct CatalogRequest {
    #[serde(flatten)]
    credentials: CredentialsRequest,
    kind: String,
}
#[derive(Deserialize)]
pub(super) struct QueryRequest {
    #[serde(flatten)]
    credentials: CredentialsRequest,
    database: Option<String>,
    sql: String,
    #[serde(default)]
    confirm_write: bool,
}
#[derive(Deserialize)]
pub(super) struct ResetPasswordRequest {
    #[serde(flatten)]
    credentials: CredentialsRequest,
    username: String,
    host: String,
    new_password: String,
}

fn reauthenticate_admin(
    db: &Connection,
    headers: &HeaderMap,
    password: &str,
) -> Result<User, (StatusCode, Json<Value>)> {
    let actor = require_admin(db, headers)?;
    if password.is_empty() || password.len() > 1024 {
        return Err(error(StatusCode::UNAUTHORIZED, "رمز مدیر پنل معتبر نیست."));
    }
    let key = format!("mysql-reauth:{}", actor.id);
    let attempts: (i64, i64) = db
        .query_row(
            "SELECT count,window_start FROM auth_failures WHERE username=?1",
            [&key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap_or((0, 0));
    if attempts.0 >= 8 && now() - attempts.1 < 600 {
        return Err(error(
            StatusCode::TOO_MANY_REQUESTS,
            "تعداد تلاش‌ها زیاد است؛ ۱۰ دقیقه بعد دوباره تلاش کن.",
        ));
    }
    let hash: String = db
        .query_row(
            "SELECT password_hash FROM users WHERE id=?1 AND enabled=1",
            [actor.id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    let parsed = PasswordHash::new(&hash).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Stored administrator password hash is invalid",
        )
    })?;
    if Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_err()
    {
        record_login_failure(db, &key);
        return Err(error(StatusCode::UNAUTHORIZED, "رمز مدیر پنل اشتباه است."));
    }
    let _ = db.execute("DELETE FROM auth_failures WHERE username=?1", [&key]);
    Ok(actor)
}

fn valid_mysql_credentials(user: &str, password: &str) -> bool {
    !user.is_empty()
        && user.len() <= 128
        && !user.chars().any(char::is_control)
        && password.len() <= 1024
        && !password.chars().any(char::is_control)
}
fn valid_identifier(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
}
fn safe_mysql_config_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
struct RemoveCredentialFile(PathBuf);
impl Drop for RemoveCredentialFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn validate_sql(
    sql: &str,
    database: Option<&str>,
    allow_catalog_query: bool,
) -> Result<(), String> {
    if sql.is_empty() || sql.len() > 64 * 1024 {
        return Err("دستور SQL خالی است یا از 64 KiB بیشتر شده است.".into());
    }
    if sql.lines().any(|line| {
        let trimmed = line.trim_start().to_ascii_lowercase();
        trimmed.starts_with('\\')
            || [
                "system ", "system\t", "source ", "tee ", "pager ", "edit ", "\\!", "\\.",
            ]
            .iter()
            .any(|prefix| trimmed.starts_with(prefix))
    }) {
        return Err(
            "دستورات کنسول mysql مانند SYSTEM و SOURCE مجاز نیستند؛ فقط SQL وارد کن.".into(),
        );
    }
    if database.is_some_and(|name| !valid_identifier(name, 64)) {
        return Err("نام دیتابیس معتبر نیست.".into());
    }
    if !allow_catalog_query {
        let lowered = sql.to_ascii_lowercase();
        if [
            "mysql.user",
            "mysql.global_priv",
            "authentication_string",
            "show create user",
        ]
        .iter()
        .any(|needle| lowered.contains(needle))
        {
            return Err(
                "نمایش hashهای احراز هویت و تعریف حساب‌های MySQL از کنسول ARG مجاز نیست.".into(),
            );
        }
    }
    Ok(())
}
fn run_mysql(
    sql: &str,
    user: &str,
    password: &str,
    database: Option<&str>,
    allow_catalog_query: bool,
) -> Result<String, String> {
    if !valid_mysql_credentials(user, password) {
        return Err("نام کاربری یا رمز MySQL معتبر نیست.".into());
    }
    validate_sql(sql, database, allow_catalog_query)?;
    let config_path = format!("/run/arg-mysql-{}.cnf", &token()[..20]);
    let config = format!(
        "[client]\nuser=\"{}\"\npassword=\"{}\"\nprotocol=socket\n",
        safe_mysql_config_value(user),
        safe_mysql_config_value(password)
    );
    let _remove_config_on_exit = {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&config_path)
            .map_err(|e| format!("Could not create private MySQL credential file: {e}"))?;
        let cleanup = RemoveCredentialFile(PathBuf::from(&config_path));
        file.write_all(config.as_bytes())
            .map_err(|e| format!("Could not write private MySQL credentials: {e}"))?;
        cleanup
    };
    let result = (|| {
        let mut command = std::process::Command::new("mysql");
        // MySQL option-file switches must be the first option and take their path
        // with '='; passing the path as the next argv element makes some clients
        // treat the switch as an unknown flag.
        command.arg(defaults_extra_file_arg(&config_path));
        command.args([
            "--batch",
            "--raw",
            "--column-names",
            "--binary-mode",
            "--local-infile=0",
            "--connect-timeout=5",
        ]);
        if let Some(name) = database {
            command.arg(format!("--database={name}"));
        }
        command
            .env("MYSQL_HISTFILE", "/dev/null")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .process_group(0);
        let mut child = command
            .spawn()
            .map_err(|e| format!("MySQL client is unavailable: {e}"))?;
        let pid = child.id() as i32;
        if let Some(mut input) = child.stdin.take() {
            input.write_all(sql.as_bytes()).map_err(|e| e.to_string())?;
            input.write_all(b"\n").map_err(|e| e.to_string())?;
        }
        let overflow = Arc::new(AtomicBool::new(false));
        let out_flag = overflow.clone();
        let out = child.stdout.take().ok_or("Could not read MySQL output")?;
        let stdout = thread::spawn(move || read_bounded(out, 1024 * 1024, pid, out_flag));
        let err_flag = overflow.clone();
        let err = child.stderr.take().ok_or("Could not read MySQL errors")?;
        let stderr = thread::spawn(move || read_bounded(err, 64 * 1024, pid, err_flag));
        let started = Instant::now();
        let status = loop {
            if started.elapsed() > std::time::Duration::from_secs(30) {
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
                break child.wait().map_err(|e| e.to_string())?;
            }
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            thread::sleep(std::time::Duration::from_millis(40));
        };
        let stdout = stdout.join().unwrap_or_default();
        let stderr = stderr.join().unwrap_or_default();
        if overflow.load(Ordering::Relaxed) {
            return Err("خروجی SQL از سقف 1 MiB عبور کرد و برای محافظت از سرور متوقف شد.".into());
        }
        if !status.success() {
            return Err(sanitize_mysql_error(&String::from_utf8_lossy(&stderr)));
        }
        Ok(String::from_utf8_lossy(&stdout).into_owned())
    })();
    result
}

fn defaults_extra_file_arg(path: &str) -> String {
    format!("--defaults-extra-file={path}")
}
fn read_bounded<R: Read>(
    mut reader: R,
    limit: usize,
    pid: i32,
    overflow: Arc<AtomicBool>,
) -> Vec<u8> {
    let mut captured = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                if captured.len() + count <= limit {
                    captured.extend_from_slice(&buffer[..count]);
                } else if !overflow.swap(true, Ordering::Relaxed) {
                    unsafe {
                        libc::kill(-pid, libc::SIGKILL);
                    }
                }
            }
        }
    }
    captured
}
fn sanitize_mysql_error(error_text: &str) -> String {
    let clean = error_text.lines().take(12).collect::<Vec<_>>().join("\n");
    if clean.is_empty() {
        "اجرای دستور MySQL ناموفق بود.".into()
    } else {
        clean
    }
}
fn mysql_password_literal(password: &str) -> Result<String, String> {
    if !(12..=128).contains(&password.len()) || password.chars().any(char::is_control) {
        return Err("رمز جدید باید 12 تا 128 نویسه باشد و نویسهٔ کنترلی نداشته باشد.".into());
    }
    Ok(format!("'{}'", password.replace('\'', "''")))
}

pub(super) async fn status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let output = privileged_request_async(json!({"action":"mysql-status"})).await?;
    if output["ok"] != true {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            output["error"]
                .as_str()
                .unwrap_or("وضعیت MySQL دریافت نشد."),
        ));
    }
    Ok(Json(output["data"].clone()))
}
pub(super) async fn install(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<InstallRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if !["mysql", "mariadb"].contains(&input.provider.as_str()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نوع سرویس دیتابیس معتبر نیست.",
        ));
    }
    if input.confirmation != "INSTALL MYSQL" {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "برای نصب، عبارت INSTALL MYSQL را تأیید کن.",
        ));
    }
    let actor = {
        let db = state.lock().unwrap();
        reauthenticate_admin(&db, &headers, &input.admin_password)?
    };
    let _apt_guard = APT_OPERATION_LOCK.lock().await;
    let output =
        privileged_request_async(json!({"action":"mysql-install","provider":input.provider}))
            .await?;
    if output["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            output["error"].as_str().unwrap_or("نصب MySQL ناموفق بود."),
        ));
    }
    let db = state.lock().unwrap();
    audit(&db, &actor.username, "mysql.install", &input.provider);
    Ok(Json(output))
}
pub(super) async fn service_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ServiceRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if ![
        "start",
        "stop",
        "restart",
        "enable",
        "disable",
        "enable-now",
        "disable-now",
    ]
    .contains(&input.action.as_str())
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "عملیات سرویس MySQL معتبر نیست.",
        ));
    }
    if ["stop", "restart", "disable", "disable-now"].contains(&input.action.as_str())
        && !input.confirmed
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "برای توقف یا غیرفعال‌سازی سرویس، تأیید صریح لازم است.",
        ));
    }
    let actor = {
        let db = state.lock().unwrap();
        reauthenticate_admin(&db, &headers, &input.admin_password)?
    };
    let result =
        privileged_request_async(json!({"action":"mysql-service", "operation":input.action}))
            .await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("عملیات سرویس MySQL ناموفق بود."),
        ));
    }
    let db = state.lock().unwrap();
    audit(&db, &actor.username, "mysql.service.action", &input.action);
    Ok(Json(
        json!({"output":result["output"].as_str().unwrap_or("MySQL service operation completed."),"action":input.action}),
    ))
}
pub(super) async fn catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CatalogRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if !["databases", "users"].contains(&input.kind.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "نوع فهرست معتبر نیست."));
    }
    let actor = {
        let db = state.lock().unwrap();
        reauthenticate_admin(&db, &headers, &input.credentials.admin_password)?
    };
    let query = if input.kind == "databases" {
        "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME"
    } else {
        "SELECT user,host,plugin,account_locked FROM mysql.user ORDER BY user,host"
    };
    let result = privileged_request_async(json!({"action":"mysql-catalog","kind":input.kind,"user":input.credentials.mysql_user,"password":input.credentials.mysql_password,"sql":query})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("دریافت اطلاعات MySQL ناموفق بود."),
        ));
    }
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor.username,
        &format!("mysql.catalog.{}", input.kind),
        "listed metadata; credentials not logged",
    );
    Ok(Json(
        json!({"output":result["output"].as_str().unwrap_or("")}),
    ))
}
pub(super) async fn query(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<QueryRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if input.sql.len() > 64 * 1024
        || input.credentials.mysql_user.len() > 128
        || input.credentials.mysql_password.len() > 1024
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "درخواست SQL از اندازهٔ مجاز بزرگ‌تر است.",
        ));
    }
    let writes_data = input
        .sql
        .split(|character: char| !character.is_ascii_alphabetic())
        .map(str::to_ascii_uppercase)
        .any(|word| {
            [
                "CREATE", "ALTER", "DROP", "TRUNCATE", "DELETE", "UPDATE", "INSERT", "REPLACE",
                "GRANT", "REVOKE",
            ]
            .contains(&word.as_str())
        });
    if writes_data && !input.confirm_write {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "این SQL داده یا دسترسی را تغییر می‌دهد؛ اجرای تغییردهنده را در رابط تأیید کن.",
        ));
    }
    let actor = {
        let db = state.lock().unwrap();
        reauthenticate_admin(&db, &headers, &input.credentials.admin_password)?
    };
    let keyword = input
        .sql
        .trim_start()
        .split_whitespace()
        .next()
        .unwrap_or("SQL")
        .chars()
        .take(24)
        .collect::<String>()
        .to_ascii_uppercase();
    let result = privileged_request_async(json!({"action":"mysql-query","user":input.credentials.mysql_user,"password":input.credentials.mysql_password,"database":input.database,"sql":input.sql})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("اجرای دستور SQL ناموفق بود."),
        ));
    }
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor.username,
        "mysql.sql.execute",
        &format!("statement={keyword}; bytes={}", input.sql.len()),
    );
    Ok(Json(
        json!({"output":result["output"].as_str().unwrap_or("Query completed.")}),
    ))
}
pub(super) async fn reset_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ResetPasswordRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_identifier(&input.username, 32)
        || !(input.host == "localhost" || input.host == "127.0.0.1" || input.host == "%")
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام کاربری یا Host مجاز نیست.",
        ));
    }
    let literal = mysql_password_literal(&input.new_password)
        .map_err(|e| error(StatusCode::BAD_REQUEST, &e))?;
    let user = input.username.replace('`', "``");
    let host = input.host.replace('`', "``");
    let sql = format!("ALTER USER `{user}`@`{host}` IDENTIFIED BY {literal};");
    let actor = {
        let db = state.lock().unwrap();
        reauthenticate_admin(&db, &headers, &input.credentials.admin_password)?
    };
    let result = privileged_request_async(json!({"action":"mysql-query","user":input.credentials.mysql_user,"password":input.credentials.mysql_password,"sql":sql})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("تغییر رمز کاربر MySQL ناموفق بود."),
        ));
    }
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor.username,
        "mysql.user.password.reset",
        &format!("target={}@{}", input.username, input.host),
    );
    Ok(Json(
        json!({"updated":true,"message":"رمز کاربر MySQL تغییر کرد؛ رمز قبلی قابل بازیابی نیست."}),
    ))
}

pub(super) fn execute_privileged_request(action: &str, request: &Value) -> Value {
    match action {
        "mysql-status" => {
            let mysql_unit = std::process::Command::new("systemctl")
                .args(["cat", "mysql.service"])
                .output()
                .is_ok_and(|o| o.status.success());
            let mariadb_unit = std::process::Command::new("systemctl")
                .args(["cat", "mariadb.service"])
                .output()
                .is_ok_and(|o| o.status.success());
            let mysql_active = std::process::Command::new("systemctl")
                .args(["is-active", "--quiet", "mysql.service"])
                .status()
                .is_ok_and(|s| s.success());
            let mariadb_active = std::process::Command::new("systemctl")
                .args(["is-active", "--quiet", "mariadb.service"])
                .status()
                .is_ok_and(|s| s.success());
            let installed = mysql_unit || mariadb_unit;
            let version = installed
                .then(|| {
                    std::process::Command::new("mysql")
                        .arg("--version")
                        .output()
                        .ok()
                        .filter(|o| o.status.success())
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                })
                .flatten();
            json!({"ok":true,"data":{"installed":installed,"version":version,"service":if mariadb_unit && !mysql_unit {"mariadb"} else {"mysql"},"active":mysql_active || mariadb_active,"provider":if mariadb_active || (mariadb_unit && !mysql_unit) {"mariadb"} else {"mysql"}}})
        }
        "mysql-install" => {
            let Some(provider) = request["provider"]
                .as_str()
                .filter(|p| ["mysql", "mariadb"].contains(p))
            else {
                return json!({"ok":false,"error":"Invalid MySQL provider"});
            };
            let package = if provider == "mariadb" {
                "mariadb-server"
            } else {
                "mysql-server"
            };
            let mut update = std::process::Command::new("apt-get");
            update
                .env("DEBIAN_FRONTEND", "noninteractive")
                .env("APT_LISTCHANGES_FRONTEND", "none")
                .args(["-y", "update"]);
            match command_result(&mut update) {
                Ok((true, _)) => {}
                Ok((false, e)) => return json!({"ok":false,"error":e}),
                Err(e) => return json!({"ok":false,"error":e}),
            }
            let mut install = std::process::Command::new("apt-get");
            install
                .env("DEBIAN_FRONTEND", "noninteractive")
                .env("APT_LISTCHANGES_FRONTEND", "none")
                .args(["-y", "install", "--", package]);
            match command_result(&mut install) {
                Ok((true, output)) => {
                    let service = if provider == "mariadb" {
                        "mariadb.service"
                    } else {
                        "mysql.service"
                    };
                    let start = std::process::Command::new("systemctl")
                        .args(["enable", "--now", service])
                        .output();
                    if start.is_ok_and(|o| o.status.success()) {
                        json!({"ok":true,"message":format!("{provider} installed and enabled."),"output":output})
                    } else {
                        json!({"ok":false,"error":format!("{provider} installed, but service could not be enabled."),"output":output})
                    }
                }
                Ok((false, output)) => json!({"ok":false,"error":output}),
                Err(error) => json!({"ok":false,"error":error}),
            }
        }
        "mysql-service" => {
            let Some(operation) = request["operation"].as_str().filter(|op| {
                [
                    "start",
                    "stop",
                    "restart",
                    "enable",
                    "disable",
                    "enable-now",
                    "disable-now",
                ]
                .contains(op)
            }) else {
                return json!({"ok":false,"error":"Invalid MySQL service operation"});
            };
            let service = if std::process::Command::new("systemctl")
                .args(["cat", "mysql.service"])
                .output()
                .is_ok_and(|output| output.status.success())
            {
                "mysql.service"
            } else {
                "mariadb.service"
            };
            let mut command = std::process::Command::new("systemctl");
            match operation {
                "enable-now" => {
                    command.args(["enable", "--now", service]);
                }
                "disable-now" => {
                    command.args(["disable", "--now", service]);
                }
                _ => {
                    command.args([operation, service]);
                }
            }
            match command_result(&mut command) {
                Ok((true, output)) => json!({"ok":true,"output":output}),
                Ok((false, output)) => json!({"ok":false,"error":output}),
                Err(error) => json!({"ok":false,"error":error}),
            }
        }
        "mysql-query" => {
            let sql = request["sql"].as_str().unwrap_or("");
            let user = request["user"].as_str().unwrap_or("");
            let password = request["password"].as_str().unwrap_or("");
            let database = request["database"].as_str();
            match run_mysql(sql, user, password, database, false) {
                Ok(output) => json!({"ok":true,"output":output}),
                Err(error) => json!({"ok":false,"error":error}),
            }
        }
        "mysql-catalog" => {
            let Some(kind) = request["kind"]
                .as_str()
                .filter(|kind| ["databases", "users"].contains(kind))
            else {
                return json!({"ok":false,"error":"Invalid catalog kind"});
            };
            let expected = if kind == "databases" {
                "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME"
            } else {
                "SELECT user,host,plugin,account_locked FROM mysql.user ORDER BY user,host"
            };
            if request["sql"].as_str() != Some(expected) {
                return json!({"ok":false,"error":"Catalog query does not match the safe allowlist"});
            }
            let user = request["user"].as_str().unwrap_or("");
            let password = request["password"].as_str().unwrap_or("");
            match run_mysql(expected, user, password, None, true) {
                Ok(output) => json!({"ok":true,"output":output}),
                Err(error) => json!({"ok":false,"error":error}),
            }
        }
        _ => json!({"ok":false,"error":"Unsupported MySQL operation"}),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        mysql_password_literal, safe_mysql_config_value, valid_identifier, valid_mysql_credentials,
        validate_sql,
    };
    #[test]
    fn mysql_inputs_are_bounded_and_quoted() {
        assert!(valid_mysql_credentials("root", "secret"));
        assert!(!valid_mysql_credentials(
            "root\n--defaults-file=/tmp/evil",
            "secret"
        ));
        assert!(valid_identifier("app_db-1", 64));
        assert!(!valid_identifier("app db", 64));
        assert_eq!(
            mysql_password_literal("a'bcdefghijkl").unwrap(),
            "'a''bcdefghijkl'"
        );
        assert!(mysql_password_literal("short").is_err());
        assert!(validate_sql("SELECT 1;", Some("app_db"), false).is_ok());
        assert!(validate_sql("SELECT 1;\n\\! cat /etc/passwd", None, false).is_err());
        assert!(validate_sql("SELECT 1;", Some("bad/name"), false).is_err());
        assert!(validate_sql("SELECT authentication_string FROM mysql.user", None, false).is_err());
        assert!(validate_sql("SELECT user FROM mysql.user", None, true).is_ok());
        assert_eq!(safe_mysql_config_value("a\\b\"c"), "a\\\\b\\\"c");
    }

    #[test]
    fn defaults_extra_file_option_uses_required_equals_form() {
        let argument = super::defaults_extra_file_arg("/run/arg-mysql-test.cnf");
        assert_eq!(argument, "--defaults-extra-file=/run/arg-mysql-test.cnf");
    }
}
