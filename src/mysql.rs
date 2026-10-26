use super::*;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

#[derive(Deserialize)]
pub(super) struct InstallRequest {
    provider: String,
    confirmation: String,
}
#[derive(Deserialize)]
pub(super) struct ServiceRequest {
    action: String,
    #[serde(default)]
    confirmed: bool,
}
#[derive(Deserialize)]
pub(super) struct CatalogRequest {
    kind: String,
}
#[derive(Deserialize)]
pub(super) struct QueryRequest {
    database: Option<String>,
    sql: String,
    #[serde(default)]
    confirm_write: bool,
}
#[derive(Deserialize)]
pub(super) struct ResetPasswordRequest {
    username: String,
    host: String,
    new_password: String,
}
#[derive(Deserialize)]
pub(super) struct UnlockRequest {
    password: String,
}
#[derive(Deserialize)]
pub(super) struct CreateUserRequest {
    database: String,
    username: String,
    host: String,
    password: String,
    privileges: String,
}
#[derive(Deserialize)]
pub(super) struct DatabaseRequest {
    name: String,
    #[serde(default)]
    confirmation: String,
}
#[derive(Deserialize)]
pub(super) struct UserActionRequest {
    username: String,
    host: String,
    action: String,
    #[serde(default)]
    confirmed: bool,
}
#[derive(Deserialize)]
pub(super) struct UserPrivilegesRequest {
    username: String,
    host: String,
    database: String,
    privileges: String,
    operation: String,
    #[serde(default)]
    confirmed: bool,
}
#[derive(Deserialize)]
pub(super) struct MysqlSettingRequest {
    name: String,
    value: String,
    #[serde(default)]
    confirmed: bool,
}
#[derive(Deserialize)]
pub(super) struct MysqlPresetRequest {
    title: String,
    database: Option<String>,
    sql: String,
}
#[derive(Serialize)]
struct MysqlPreset {
    id: i64,
    title: String,
    database: Option<String>,
    sql: String,
    updated_at: i64,
}

const MANAGER_CONFIG: &str = "/etc/arg/mysql-manager.cnf";
const MYSQL_USER_CATALOG_QUERY: &str =
    "SELECT user,host,plugin,account_locked FROM mysql.user ORDER BY user,host";
const MYSQL_USER_CATALOG_FALLBACK_QUERY: &str =
    "SELECT user,host,plugin FROM mysql.user ORDER BY user,host";
static MYSQL_MANAGER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lacks_account_locked_column(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("unknown column") && error.contains("account_locked")
}

fn session_hash_for(headers: &HeaderMap) -> Result<String, (StatusCode, Json<Value>)> {
    cookie_token(headers)
        .map(|raw| session_hash(&raw))
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "Login required"))
}

fn require_mysql_access(
    db: &Connection,
    headers: &HeaderMap,
) -> Result<User, (StatusCode, Json<Value>)> {
    let actor = require_admin(db, headers)?;
    let hash = session_hash_for(headers)?;
    let allowed = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM mysql_authorizations a JOIN sessions s ON s.token_hash=a.session_hash WHERE a.session_hash=?1 AND a.user_id=?2 AND s.expires_at>?3)",
        params![hash, actor.id, now()], |row| row.get::<_, bool>(0),
    ).unwrap_or(false);
    if !allowed {
        return Err(error(
            StatusCode::FORBIDDEN,
            "برای فعال‌سازی مدیریت MySQL، یک بار رمز پنل را تأیید کن.",
        ));
    }
    Ok(actor)
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
fn valid_mysql_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 255
        && host.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'%' | b':' | b'-')
        })
}
fn protected_mysql_user(user: &str) -> bool {
    matches!(
        user.to_ascii_lowercase().as_str(),
        "root" | "mysql" | "mariadb.sys" | "mysql.infoschema" | "mysql.session" | "mysql.sys" | "debian-sys-maint" | "arg_manager"
    )
}
const MYSQL_TUNABLES: &[(&str, &str)] = &[
    ("max_connections", "integer"),
    ("wait_timeout", "integer"),
    ("interactive_timeout", "integer"),
    ("max_allowed_packet", "bytes"),
    ("long_query_time", "seconds"),
    ("slow_query_log", "boolean"),
    ("innodb_buffer_pool_size", "bytes"),
];

fn mysql_buffer_pool_limit_bytes() -> u64 {
    let memory_kib = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|contents| contents.lines().find(|line| line.starts_with("MemTotal:"))?.split_whitespace().nth(1)?.parse::<u64>().ok())
        .unwrap_or(1_048_576);
    memory_kib.saturating_mul(1024).saturating_mul(3) / 4
}

fn validate_mysql_setting(name: &str, value: &str) -> Result<(), String> {
    let Some((_, kind)) = MYSQL_TUNABLES.iter().find(|(allowed, _)| *allowed == name) else {
        return Err("این متغیر سرور در فهرست تنظیمات امن ARG نیست.".into());
    };
    match *kind {
        "boolean" if ["ON", "OFF", "1", "0"].contains(&value.to_ascii_uppercase().as_str()) => Ok(()),
        "integer" | "bytes" => {
            let number = value.parse::<u64>().map_err(|_| "مقدار باید عدد صحیح باشد.")?;
            let (minimum, maximum) = match name {
                "max_connections" => (10, 10_000),
                "wait_timeout" | "interactive_timeout" => (1, 31_536_000),
                "max_allowed_packet" => (1_024, 1_073_741_824),
                "innodb_buffer_pool_size" => (5_242_880, mysql_buffer_pool_limit_bytes().min(1_073_741_824_000)),
                _ => (1, u64::MAX),
            };
            if !(minimum..=maximum).contains(&number) { return Err("مقدار خارج از محدودهٔ مجاز این تنظیم است.".into()); }
            Ok(())
        }
        "seconds" => {
            let number = value.parse::<f64>().map_err(|_| "زمان باید عددی بین 0 و 3600 ثانیه باشد.")?;
            if number.is_finite() && (0.0..=3600.0).contains(&number) { Ok(()) } else { Err("زمان باید عددی بین 0 و 3600 ثانیه باشد.".into()) }
        }
        _ => Err("مقدار این تنظیم معتبر نیست.".into()),
    }
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
fn mysql_sql_requires_confirmation(sql: &str) -> bool {
    const MUTATING: &[&str] = &[
        "CREATE", "ALTER", "DROP", "TRUNCATE", "DELETE", "UPDATE", "INSERT", "REPLACE",
        "GRANT", "REVOKE", "SET", "CALL", "RENAME", "ANALYZE", "OPTIMIZE", "REPAIR",
        "FLUSH", "RESET", "KILL", "SHUTDOWN", "LOAD", "INSTALL", "UNINSTALL", "START",
        "STOP", "CHANGE", "PURGE", "LOCK", "UNLOCK",
    ];
    sql.split(|character: char| !character.is_ascii_alphabetic())
        .any(|word| MUTATING.contains(&word.to_ascii_uppercase().as_str()))
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

fn run_mysql_with_config(
    sql: &str,
    database: Option<&str>,
    allow_catalog: bool,
    config_path: &str,
) -> Result<String, String> {
    validate_sql(sql, database, allow_catalog)?;
    let mut command = std::process::Command::new("mysql");
    command.arg(defaults_extra_file_arg(config_path));
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
}

fn ensure_manager_account() -> Result<(), String> {
    let _bootstrap_guard = MYSQL_MANAGER_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if let Ok(metadata) = fs::symlink_metadata(MANAGER_CONFIG) {
            if metadata.file_type().is_file()
                && metadata.uid() == 0
                && metadata.permissions().mode() & 0o077 == 0
                && run_mysql_with_config("SELECT 1;", None, false, MANAGER_CONFIG).is_ok()
            {
                return Ok(());
            }
        }
    }
    let existing = run_mysql(
        "SELECT COUNT(*) FROM mysql.user WHERE user='arg_manager' AND host='localhost';",
        "root",
        "",
        None,
        true,
    )?;
    if existing
        .lines()
        .nth(1)
        .is_some_and(|count| count.trim() != "0")
    {
        return Err("حساب arg_manager از قبل وجود دارد اما فایل امن اعتبار آن موجود یا معتبر نیست؛ برای جلوگیری از تغییر حسابی که متعلق به ARG نیست، آن را خودکار بازنویسی نکردم.".into());
    }
    let secret = token();
    let literal = mysql_password_literal(&secret)?;
    let bootstrap = format!("CREATE USER 'arg_manager'@'localhost' IDENTIFIED BY {literal}; GRANT ALL PRIVILEGES ON *.* TO 'arg_manager'@'localhost' WITH GRANT OPTION; FLUSH PRIVILEGES;");
    run_mysql(&bootstrap, "root", "", None, false).map_err(|e| format!("راه‌اندازی دسترسی مدیریتی ARG ناموفق بود؛ احراز هویت root از Unix socket در دسترس نیست: {e}"))?;
    let manager_path = PathBuf::from(MANAGER_CONFIG);
    let directory = manager_path
        .parent()
        .ok_or("Invalid ARG MySQL config path")?;
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let temporary = format!("{MANAGER_CONFIG}.{}.tmp", &token()[..12]);
    let config = format!(
        "[client]\nuser=\"arg_manager\"\npassword=\"{}\"\nprotocol=socket\n",
        safe_mysql_config_value(&secret)
    );
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(config.as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    fs::rename(&temporary, MANAGER_CONFIG).map_err(|e| {
        let _ = fs::remove_file(&temporary);
        e.to_string()
    })?;
    run_mysql_with_config("SELECT 1;", None, false, MANAGER_CONFIG).map(|_| ())
}

fn mysql_create_user_sql(input: &CreateUserRequest) -> Result<String, String> {
    if !valid_identifier(&input.database, 64)
        || !valid_identifier(&input.username, 32)
        || protected_mysql_user(&input.username)
        || !valid_mysql_host(&input.host)
    {
        return Err("نام دیتابیس، کاربر یا Host معتبر نیست.".into());
    }
    if ["mysql", "sys", "performance_schema", "information_schema"]
        .contains(&input.database.as_str())
    {
        return Err(
            "برای محافظت از دیتابیس‌های سیستمی، ساخت کاربر برنامه روی این دیتابیس مجاز نیست.".into(),
        );
    }
    let password = mysql_password_literal(&input.password)?;
    let grants = match input.privileges.as_str() {
        "read_only" => "SELECT, SHOW VIEW",
        "read_write" => {
            "SELECT, INSERT, UPDATE, DELETE, CREATE, ALTER, INDEX, DROP, EXECUTE, SHOW VIEW"
        }
        "full" => "ALL PRIVILEGES",
        _ => return Err("سطح دسترسی معتبر نیست.".into()),
    };
    let db = input.database.replace('`', "``");
    let username = input.username.replace('\'', "''");
    let host = input.host.replace('\'', "''");
    Ok(format!("CREATE USER IF NOT EXISTS '{username}'@'{host}' IDENTIFIED BY {password}; GRANT {grants} ON `{db}`.* TO '{username}'@'{host}'; FLUSH PRIVILEGES;"))
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
    Ok(format!(
        "'{}'",
        password.replace('\\', "\\\\").replace('\'', "''")
    ))
}

pub(super) async fn status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_mysql_access(&db, &headers)?;
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
pub(super) async fn access_status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    let _actor = require_admin(&db, &headers)?;
    let hash = session_hash_for(&headers)?;
    let authorized = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM mysql_authorizations WHERE session_hash=?1)",
            [&hash],
            |r| r.get::<_, bool>(0),
        )
        .unwrap_or(false);
    Ok(Json(json!({"authorized":authorized})))
}
pub(super) async fn unlock(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<UnlockRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let (actor, hash, installed) = {
        let db = state.lock().unwrap();
        let actor = reauthenticate_admin(&db, &headers, &input.password)?;
        let hash = session_hash_for(&headers)?;
        let installed = std::process::Command::new("systemctl")
            .args(["cat", "mysql.service"])
            .output()
            .is_ok_and(|o| o.status.success())
            || std::process::Command::new("systemctl")
                .args(["cat", "mariadb.service"])
                .output()
                .is_ok_and(|o| o.status.success());
        (actor, hash, installed)
    };
    if installed {
        let result = privileged_request_async(json!({"action":"mysql-ensure-manager"})).await?;
        if result["ok"] != true {
            return Err(error(
                StatusCode::BAD_GATEWAY,
                result["error"]
                    .as_str()
                    .unwrap_or("ساخت دسترسی مدیریتی MySQL ناموفق بود."),
            ));
        }
    }
    let db = state.lock().unwrap();
    db.execute("INSERT INTO mysql_authorizations(session_hash,user_id,authorized_at) VALUES (?1,?2,?3) ON CONFLICT(session_hash) DO UPDATE SET user_id=excluded.user_id,authorized_at=excluded.authorized_at", params![hash, actor.id, now()]).map_err(db_error)?;
    audit(
        &db,
        &actor.username,
        "mysql.unlock",
        "panel password re-authenticated once for this login session",
    );
    Ok(Json(json!({"authorized":true})))
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
        require_mysql_access(&db, &headers)?
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
    let manager = privileged_request_async(json!({"action":"mysql-ensure-manager"})).await?;
    if manager["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            manager["error"]
                .as_str()
                .unwrap_or("ساخت حساب داخلی ARG در MySQL ناموفق بود."),
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
        require_mysql_access(&db, &headers)?
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
    if ["start", "restart", "enable-now"].contains(&input.action.as_str()) {
        let manager = privileged_request_async(json!({"action":"mysql-ensure-manager"})).await?;
        if manager["ok"] != true {
            return Err(error(
                StatusCode::BAD_GATEWAY,
                manager["error"]
                    .as_str()
                    .unwrap_or("ساخت حساب داخلی ARG در MySQL ناموفق بود."),
            ));
        }
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
        require_mysql_access(&db, &headers)?
    };
    let query = if input.kind == "databases" {
        "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME"
    } else {
        MYSQL_USER_CATALOG_QUERY
    };
    let result =
        privileged_request_async(json!({"action":"mysql-catalog","kind":input.kind,"sql":query}))
            .await?;
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

pub(super) async fn metrics(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_mysql_access(&db, &headers)?;
    }
    let sql = "SHOW GLOBAL STATUS WHERE Variable_name IN ('Threads_connected','Max_used_connections','Connections','Queries','Slow_queries','Uptime','Innodb_buffer_pool_read_requests','Innodb_buffer_pool_reads')";
    let result = privileged_request_async(json!({"action":"mysql-query","sql":sql})).await?;
    if result["ok"] != true { return Err(error(StatusCode::BAD_GATEWAY, result["error"].as_str().unwrap_or("خواندن آمار MySQL ناموفق بود."))); }
    Ok(Json(json!({"output":result["output"].as_str().unwrap_or("")})))
}

pub(super) async fn settings(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_mysql_access(&db, &headers)?;
    }
    let names = MYSQL_TUNABLES.iter().map(|(name, _)| format!("'{name}'")).collect::<Vec<_>>().join(",");
    let sql = format!("SHOW GLOBAL VARIABLES WHERE Variable_name IN ({names})");
    let result = privileged_request_async(json!({"action":"mysql-query","sql":sql})).await?;
    if result["ok"] != true { return Err(error(StatusCode::BAD_GATEWAY, result["error"].as_str().unwrap_or("خواندن تنظیمات MySQL ناموفق بود."))); }
    Ok(Json(json!({"output":result["output"].as_str().unwrap_or(""),"tunable_names":MYSQL_TUNABLES.iter().map(|(name, _)| *name).collect::<Vec<_>>()})))
}

pub(super) async fn update_setting(
    State(state): State<AppState>, headers: HeaderMap, Json(input): Json<MysqlSettingRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    validate_mysql_setting(&input.name, &input.value).map_err(|e| error(StatusCode::BAD_REQUEST, &e))?;
    if !input.confirmed { return Err(error(StatusCode::BAD_REQUEST, "برای اعمال تغییر تنظیم سرور، تأیید صریح لازم است.")); }
    let actor = { let db = state.lock().unwrap(); require_mysql_access(&db, &headers)? };
    let value = if matches!(input.value.to_ascii_uppercase().as_str(), "ON" | "OFF") { input.value.to_ascii_uppercase() } else { input.value.clone() };
    let sql = format!("SET GLOBAL {} = {}", input.name, value);
    let result = privileged_request_async(json!({"action":"mysql-query","sql":sql})).await?;
    if result["ok"] != true { return Err(error(StatusCode::BAD_GATEWAY, result["error"].as_str().unwrap_or("اعمال تنظیم MySQL ناموفق بود."))); }
    let db = state.lock().unwrap();
    audit(&db, &actor.username, "mysql.setting.update", &format!("variable={}; value={}", input.name, value));
    Ok(Json(json!({"updated":true,"message":"تنظیم در حال اجرای MySQL اعمال شد. برای دائمی‌شدن پس از راه‌اندازی مجدد، فایل تنظیمات سرور را جداگانه بررسی کن."})))
}

fn valid_application_database(name: &str) -> bool {
    valid_identifier(name, 64) && !["mysql", "sys", "performance_schema", "information_schema"].contains(&name)
}

pub(super) async fn create_database(
    State(state): State<AppState>, headers: HeaderMap, Json(input): Json<DatabaseRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_application_database(&input.name) || input.confirmation != format!("CREATE DATABASE {}", input.name) {
        return Err(error(StatusCode::BAD_REQUEST, "نام دیتابیس معتبر نیست یا تأیید ساخت با نام آن مطابقت ندارد."));
    }
    let actor = { let db = state.lock().unwrap(); require_mysql_access(&db, &headers)? };
    let sql = format!("CREATE DATABASE `{}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci", input.name);
    let result = privileged_request_async(json!({"action":"mysql-query","sql":sql})).await?;
    if result["ok"] != true { return Err(error(StatusCode::BAD_GATEWAY, result["error"].as_str().unwrap_or("ساخت دیتابیس ناموفق بود."))); }
    let db = state.lock().unwrap();
    audit(&db, &actor.username, "mysql.database.create", &input.name);
    Ok(Json(json!({"created":true,"message":format!("دیتابیس {} ساخته شد.", input.name)})))
}

pub(super) async fn delete_database(
    State(state): State<AppState>, Path(name): Path<String>, headers: HeaderMap, Json(input): Json<DatabaseRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_application_database(&name) || input.confirmation != format!("DROP DATABASE {name}") {
        return Err(error(StatusCode::BAD_REQUEST, "برای حذف، نام دیتابیس و عبارت تأیید باید دقیقاً مطابقت داشته باشند."));
    }
    let actor = { let db = state.lock().unwrap(); require_mysql_access(&db, &headers)? };
    let sql = format!("DROP DATABASE `{name}`");
    let result = privileged_request_async(json!({"action":"mysql-query","sql":sql})).await?;
    if result["ok"] != true { return Err(error(StatusCode::BAD_GATEWAY, result["error"].as_str().unwrap_or("حذف دیتابیس ناموفق بود."))); }
    let db = state.lock().unwrap();
    audit(&db, &actor.username, "mysql.database.drop", &name);
    Ok(Json(json!({"deleted":true,"message":format!("دیتابیس {} حذف شد.", name)})))
}

pub(super) async fn user_action(
    State(state): State<AppState>, headers: HeaderMap, Json(input): Json<UserActionRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_identifier(&input.username, 32) || protected_mysql_user(&input.username) || !valid_mysql_host(&input.host) || !["grants", "lock", "unlock", "delete"].contains(&input.action.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "حساب یا عملیات انتخاب‌شده مجاز نیست."));
    }
    if input.action != "grants" && !input.confirmed { return Err(error(StatusCode::BAD_REQUEST, "برای تغییر وضعیت حساب تأیید صریح لازم است.")); }
    let actor = { let db = state.lock().unwrap(); require_mysql_access(&db, &headers)? };
    let user = input.username.replace('\'', "''");
    let host = input.host.replace('\'', "''");
    let sql = match input.action.as_str() {
        "grants" => format!("SHOW GRANTS FOR '{user}'@'{host}'"),
        "lock" => format!("ALTER USER '{user}'@'{host}' ACCOUNT LOCK"),
        "unlock" => format!("ALTER USER '{user}'@'{host}' ACCOUNT UNLOCK"),
        "delete" => format!("DROP USER '{user}'@'{host}'"),
        _ => unreachable!(),
    };
    let result = privileged_request_async(json!({"action":"mysql-query","sql":sql})).await?;
    if result["ok"] != true { return Err(error(StatusCode::BAD_GATEWAY, result["error"].as_str().unwrap_or("عملیات حساب MySQL ناموفق بود."))); }
    let db = state.lock().unwrap();
    audit(&db, &actor.username, &format!("mysql.user.{}", input.action), &format!("target={}@{}", input.username, input.host));
    Ok(Json(json!({"output":result["output"].as_str().unwrap_or(""),"message":format!("عملیات {} برای {}@{} انجام شد.", input.action, input.username, input.host)})))
}

pub(super) async fn update_user_privileges(
    State(state): State<AppState>, headers: HeaderMap, Json(input): Json<UserPrivilegesRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_identifier(&input.username, 32) || protected_mysql_user(&input.username) || !valid_mysql_host(&input.host) || !valid_application_database(&input.database) || !["grant", "revoke"].contains(&input.operation.as_str()) || !["read_only", "read_write", "full"].contains(&input.privileges.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "حساب، دیتابیس، سطح دسترسی یا عملیات معتبر نیست."));
    }
    if !input.confirmed { return Err(error(StatusCode::BAD_REQUEST, "برای تغییر مجوز حساب تأیید صریح لازم است.")); }
    let privileges = match input.privileges.as_str() {
        "read_only" => "SELECT, SHOW VIEW",
        "read_write" => "SELECT, INSERT, UPDATE, DELETE, CREATE, ALTER, INDEX, DROP, EXECUTE, SHOW VIEW",
        "full" => "ALL PRIVILEGES",
        _ => unreachable!(),
    };
    let username = input.username.replace('\'', "''");
    let host = input.host.replace('\'', "''");
    let database = input.database.replace('`', "``");
    let sql = if input.operation == "grant" {
        format!("GRANT {privileges} ON `{database}`.* TO '{username}'@'{host}'")
    } else if input.privileges == "full" {
        format!("REVOKE ALL PRIVILEGES, GRANT OPTION ON `{database}`.* FROM '{username}'@'{host}'")
    } else {
        format!("REVOKE {privileges} ON `{database}`.* FROM '{username}'@'{host}'")
    };
    let actor = { let db = state.lock().unwrap(); require_mysql_access(&db, &headers)? };
    let result = privileged_request_async(json!({"action":"mysql-query","sql":sql})).await?;
    if result["ok"] != true { return Err(error(StatusCode::BAD_GATEWAY, result["error"].as_str().unwrap_or("تغییر مجوز کاربر ناموفق بود."))); }
    let db = state.lock().unwrap();
    audit(&db, &actor.username, &format!("mysql.user.privileges.{}", input.operation), &format!("target={}@{} database={} privilege={}", input.username, input.host, input.database, input.privileges));
    Ok(Json(json!({"updated":true,"message":format!("مجوز {} برای {}@{} روی {} اعمال شد.", input.privileges, input.username, input.host, input.database)})))
}

pub(super) async fn presets(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    let actor = require_mysql_access(&db, &headers)?;
    let mut statement = db.prepare("SELECT id,title,database_name,statement,updated_at FROM mysql_sql_presets WHERE user_id=?1 ORDER BY title COLLATE NOCASE").map_err(db_error)?;
    let items = statement.query_map([actor.id], |row| Ok(MysqlPreset { id:row.get(0)?, title:row.get(1)?, database:row.get(2)?, sql:row.get(3)?, updated_at:row.get(4)? })).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
    Ok(Json(json!({"presets":items})))
}

pub(super) async fn save_preset(
    State(state): State<AppState>, headers: HeaderMap, Json(input): Json<MysqlPresetRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if input.title.trim().is_empty() || input.title.trim().chars().count() > 80 { return Err(error(StatusCode::BAD_REQUEST, "نام پریست باید بین 1 تا 80 نویسه باشد.")); }
    validate_sql(&input.sql, input.database.as_deref(), false).map_err(|e| error(StatusCode::BAD_REQUEST, &e))?;
    let actor = { let db = state.lock().unwrap(); require_mysql_access(&db, &headers)? };
    let db = state.lock().unwrap();
    let count: i64 = db.query_row("SELECT COUNT(*) FROM mysql_sql_presets WHERE user_id=?1", [actor.id], |row| row.get(0)).map_err(db_error)?;
    if count >= 40 && db.query_row("SELECT EXISTS(SELECT 1 FROM mysql_sql_presets WHERE user_id=?1 AND title=?2)", params![actor.id, input.title.trim()], |row| row.get::<_,bool>(0)).map_err(db_error)? == false {
        return Err(error(StatusCode::CONFLICT, "حداکثر 40 پریست برای هر حساب ذخیره می‌شود."));
    }
    db.execute("INSERT INTO mysql_sql_presets(user_id,title,database_name,statement,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?5) ON CONFLICT(user_id,title) DO UPDATE SET database_name=excluded.database_name,statement=excluded.statement,updated_at=excluded.updated_at", params![actor.id, input.title.trim(), input.database.as_deref(), input.sql, now()]).map_err(db_error)?;
    audit(&db, &actor.username, "mysql.preset.save", input.title.trim());
    Ok(Json(json!({"saved":true,"message":"پریست SQL ذخیره شد."})))
}

pub(super) async fn delete_preset(State(state): State<AppState>, Path(id): Path<i64>, headers: HeaderMap) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_mysql_access(&db, &headers)?;
    let changed = db.execute("DELETE FROM mysql_sql_presets WHERE id=?1 AND user_id=?2", params![id, actor.id]).map_err(db_error)?;
    if changed == 0 { return Err(error(StatusCode::NOT_FOUND, "پریست پیدا نشد.")); }
    audit(&db, &actor.username, "mysql.preset.delete", &id.to_string());
    Ok(Json(json!({"deleted":true})))
}
pub(super) async fn query(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<QueryRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if input.sql.len() > 64 * 1024 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "درخواست SQL از اندازهٔ مجاز بزرگ‌تر است.",
        ));
    }
    let writes_data = mysql_sql_requires_confirmation(&input.sql);
    if writes_data && !input.confirm_write {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "این SQL داده یا دسترسی را تغییر می‌دهد؛ اجرای تغییردهنده را در رابط تأیید کن.",
        ));
    }
    let actor = {
        let db = state.lock().unwrap();
        require_mysql_access(&db, &headers)?
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
    let result = privileged_request_async(
        json!({"action":"mysql-query","database":input.database,"sql":input.sql}),
    )
    .await?;
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
        || protected_mysql_user(&input.username)
        || !valid_mysql_host(&input.host)
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
        require_mysql_access(&db, &headers)?
    };
    let result = privileged_request_async(json!({"action":"mysql-query","sql":sql})).await?;
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

pub(super) async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateUserRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_mysql_access(&db, &headers)?
    };
    mysql_create_user_sql(&input).map_err(|e| error(StatusCode::BAD_REQUEST, &e))?;
    let result = privileged_request_async(json!({"action":"mysql-create-user","database":input.database,"username":input.username,"host":input.host,"password":input.password,"privileges":input.privileges})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("ساخت کاربر MySQL ناموفق بود."),
        ));
    }
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor.username,
        "mysql.user.create",
        &format!(
            "user={}@{} database={} privileges={}",
            input.username, input.host, input.database, input.privileges
        ),
    );
    Ok(Json(
        json!({"created":true,"message":"کاربر دیتابیس ساخته شد و دسترسی فقط به دیتابیس انتخاب‌شده داده شد."}),
    ))
}

pub(super) fn execute_privileged_request(action: &str, request: &Value) -> Value {
    match action {
        "mysql-ensure-manager" => match ensure_manager_account() {
            Ok(()) => json!({"ok":true,"ready":true}),
            Err(error) => json!({"ok":false,"error":error}),
        },
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
            let database = request["database"].as_str();
            match run_mysql_with_config(sql, database, false, MANAGER_CONFIG) {
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
                MYSQL_USER_CATALOG_QUERY
            };
            if request["sql"].as_str() != Some(expected) {
                return json!({"ok":false,"error":"Catalog query does not match the safe allowlist"});
            }
            let result = run_mysql_with_config(expected, None, true, MANAGER_CONFIG);
            let result = match result {
                Err(error) if kind == "users" && lacks_account_locked_column(&error) => {
                    run_mysql_with_config(
                        MYSQL_USER_CATALOG_FALLBACK_QUERY,
                        None,
                        true,
                        MANAGER_CONFIG,
                    )
                }
                other => other,
            };
            match result {
                Ok(output) => json!({"ok":true,"output":output}),
                Err(error) => json!({"ok":false,"error":error}),
            }
        }
        "mysql-create-user" => {
            let parsed = CreateUserRequest {
                database: request["database"].as_str().unwrap_or("").to_owned(),
                username: request["username"].as_str().unwrap_or("").to_owned(),
                host: request["host"].as_str().unwrap_or("").to_owned(),
                password: request["password"].as_str().unwrap_or("").to_owned(),
                privileges: request["privileges"].as_str().unwrap_or("").to_owned(),
            };
            let sql = match mysql_create_user_sql(&parsed) {
                Ok(sql) => sql,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            let user_check = format!(
                "SELECT COUNT(*) FROM mysql.user WHERE user='{}' AND host='{}';",
                parsed.username.replace('\'', "''"),
                parsed.host.replace('\'', "''")
            );
            if run_mysql_with_config(&user_check, None, true, MANAGER_CONFIG)
                .is_ok_and(|out| out.lines().nth(1).is_some_and(|count| count.trim() != "0"))
            {
                return json!({"ok":false,"error":"این حساب از قبل وجود دارد. برای تغییر رمز آن از بخش بازنشانی رمز استفاده کن."});
            }
            let exists = run_mysql_with_config(
                &format!(
                    "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME='{}';",
                    parsed.database.replace('\'', "''")
                ),
                None,
                true,
                MANAGER_CONFIG,
            );
            if !exists.is_ok_and(|v| v.lines().skip(1).any(|line| line == parsed.database)) {
                return json!({"ok":false,"error":"دیتابیس انتخاب‌شده وجود ندارد یا قابل دسترسی نیست."});
            }
            match run_mysql_with_config(&sql, None, true, MANAGER_CONFIG) {
                Ok(_) => json!({"ok":true}),
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
        validate_sql, validate_mysql_setting, valid_application_database, valid_mysql_host,
        mysql_sql_requires_confirmation, MYSQL_USER_CATALOG_FALLBACK_QUERY, MYSQL_USER_CATALOG_QUERY,
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

    #[test]
    fn user_catalog_falls_back_when_account_locked_is_not_supported() {
        assert_eq!(
            MYSQL_USER_CATALOG_QUERY,
            "SELECT user,host,plugin,account_locked FROM mysql.user ORDER BY user,host"
        );
        assert_eq!(
            MYSQL_USER_CATALOG_FALLBACK_QUERY,
            "SELECT user,host,plugin FROM mysql.user ORDER BY user,host"
        );
        assert!(super::lacks_account_locked_column(
            "ERROR 1054 (42S22): Unknown column 'account_locked' in 'SELECT'"
        ));
        assert!(!super::lacks_account_locked_column(
            "Access denied for table mysql.user"
        ));
    }

    #[test]
    fn database_user_grants_are_scoped_to_one_database() {
        let request = super::CreateUserRequest {
            database: "app_db".into(),
            username: "app_user".into(),
            host: "localhost".into(),
            password: "long-enough-secret".into(),
            privileges: "read_write".into(),
        };
        let sql = super::mysql_create_user_sql(&request).unwrap();
        assert!(sql.contains("ON `app_db`.*"));
        assert!(!sql.contains("ON *.*"));
        assert!(sql.contains("SELECT, INSERT, UPDATE"));
        let mut invalid = request;
        invalid.database = "app_db; GRANT ALL ON *".into();
        assert!(super::mysql_create_user_sql(&invalid).is_err());
    }

    #[test]
    fn database_and_account_admin_guards_exclude_system_accounts() {
        assert!(valid_application_database("customer_app"));
        assert!(!valid_application_database("mysql"));
        assert!(!valid_application_database("performance_schema"));
        assert!(valid_mysql_host("10.20.%"));
        assert!(!valid_mysql_host("localhost' OR 1=1"));
    }

    #[test]
    fn runtime_mysql_settings_are_allowlisted_and_bounded() {
        assert!(validate_mysql_setting("max_connections", "240").is_ok());
        assert!(validate_mysql_setting("slow_query_log", "ON").is_ok());
        assert!(validate_mysql_setting("max_connections", "1000000").is_err());
        assert!(validate_mysql_setting("init_file", "/tmp/evil.sql").is_err());
    }

    #[test]
    fn mutating_sql_requires_confirmation_for_advanced_operations() {
        assert!(!mysql_sql_requires_confirmation("SHOW GLOBAL VARIABLES;"));
        assert!(mysql_sql_requires_confirmation("SET GLOBAL slow_query_log = ON;"));
        assert!(mysql_sql_requires_confirmation("CALL rotate_data();"));
        assert!(mysql_sql_requires_confirmation("RENAME TABLE old_table TO new_table;"));
    }
}
