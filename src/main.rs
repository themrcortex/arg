use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::{header, HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use rand::RngCore;
use rusqlite::{params, Connection};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::os::unix::net::UnixStream as StdUnixStream;
use std::os::unix::{fs::PermissionsExt, io::AsRawFd};
use std::{
    collections::HashMap,
    env, fs,
    io::{self, BufRead, BufReader},
    net::SocketAddr,
    sync::{Arc, Mutex, OnceLock},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader},
    net::{UnixListener, UnixStream},
};

const PRIVILEGED_SOCKET: &str = "/run/donatello-privileged/manager.sock";
const NGINX_AVAILABLE_DIR: &str = "/etc/nginx/sites-available";
const NGINX_ENABLED_DIR: &str = "/etc/nginx/sites-enabled";
static NGINX_OPERATION_LOCK: Mutex<()> = Mutex::new(());

type AppState = Arc<Mutex<Connection>>;
type ApiResult = Result<Json<Value>, (StatusCode, Json<Value>)>;

#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
struct Frontend;

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
    #[serde(default)]
    setup_token: Option<String>,
}
#[derive(Deserialize)]
struct NewUser {
    username: String,
    password: String,
    role: String,
}
#[derive(Deserialize)]
struct UserUpdate {
    role: String,
    enabled: bool,
}
#[derive(Deserialize)]
struct PasswordChange {
    current_password: String,
    new_password: String,
}
#[derive(Serialize)]
struct User {
    id: i64,
    username: String,
    role: String,
    enabled: bool,
}

fn error(status: StatusCode, message: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"error": message})))
}
fn db_error(_: rusqlite::Error) -> (StatusCode, Json<Value>) {
    error(StatusCode::INTERNAL_SERVER_ERROR, "Database error")
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn session_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
fn token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}
fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            part.trim()
                .strip_prefix("donatello_session=")
                .map(str::to_owned)
        })
}
fn require_user(db: &Connection, headers: &HeaderMap) -> Result<User, (StatusCode, Json<Value>)> {
    let raw =
        cookie_token(headers).ok_or_else(|| error(StatusCode::UNAUTHORIZED, "Login required"))?;
    db.query_row("SELECT u.id,u.username,u.role,u.enabled FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=?1 AND s.expires_at>?2 AND u.enabled=1", params![session_hash(&raw), now()], |row| Ok(User { id: row.get(0)?, username: row.get(1)?, role: row.get(2)?, enabled: row.get(3)? })).map_err(|_| error(StatusCode::UNAUTHORIZED, "Session expired"))
}
fn require_admin(db: &Connection, headers: &HeaderMap) -> Result<User, (StatusCode, Json<Value>)> {
    let user = require_user(db, headers)?;
    if user.role != "admin" {
        return Err(error(StatusCode::FORBIDDEN, "Admin role required"));
    }
    Ok(user)
}
fn check_origin(headers: &HeaderMap) -> Result<(), (StatusCode, Json<Value>)> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| error(StatusCode::FORBIDDEN, "Origin header required"))?;
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| error(StatusCode::FORBIDDEN, "Host header required"))?;
    if origin == format!("http://{host}") || origin == format!("https://{host}") {
        Ok(())
    } else {
        Err(error(StatusCode::FORBIDDEN, "Invalid origin"))
    }
}
fn audit(db: &Connection, actor: &str, action: &str, detail: &str) {
    let _ = db.execute(
        "INSERT INTO audit (at,actor,action,detail) VALUES (?1,?2,?3,?4)",
        params![now(), actor, action, detail],
    );
}
fn initialized(db: &Connection) -> bool {
    db.query_row("SELECT EXISTS(SELECT 1 FROM users)", [], |r| {
        r.get::<_, bool>(0)
    })
    .unwrap_or(false)
}

async fn state(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    if !initialized(&db) {
        return Ok(Json(json!({"installed": false})));
    }
    let user = require_user(&db, &headers).ok();
    Ok(Json(json!({"installed": true, "user": user})))
}
async fn setup(
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Credentials>,
) -> ApiResult {
    check_origin(&headers)?;
    let local_setup = remote.ip().is_loopback();
    let db = state.lock().unwrap();
    let expected_token = env::var("DONATELLO_SETUP_TOKEN").ok().or_else(|| {
        db.query_row(
            "SELECT value FROM settings WHERE key='setup_token'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
    });
    let setup_token_valid = expected_token
        .zip(input.setup_token.as_deref().map(str::to_owned))
        .is_some_and(|(expected, supplied)| !expected.is_empty() && expected == supplied);
    if !local_setup && !setup_token_valid {
        return Err(error(
            StatusCode::FORBIDDEN,
            "Enter the one-time setup code shown in the server console.",
        ));
    }
    if !local_setup
        && headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            != Some("https")
    {
        return Err(error(
            StatusCode::FORBIDDEN,
            "Use the HTTPS panel URL to create the first administrator.",
        ));
    }
    if initialized(&db) {
        return Err(error(StatusCode::CONFLICT, "Already installed"));
    }
    if input.username.len() < 3
        || input.username.len() > 32
        || !input
            .username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        || input.password.len() < 12
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Username must be 3-32 characters and password at least 12 characters",
        ));
    }
    let hash = Argon2::default()
        .hash_password(input.password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Password hashing failed"))?
        .to_string();
    db.execute(
        "INSERT INTO users (username,password_hash,role,enabled) VALUES (?1,?2,'admin',1)",
        params![input.username, hash],
    )
    .map_err(db_error)?;
    db.execute("DELETE FROM settings WHERE key='setup_token'", [])
        .map_err(db_error)?;
    audit(
        &db,
        &input.username,
        "setup",
        "Initial administrator created",
    );
    Ok(Json(json!({"ok": true})))
}
async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Credentials>,
) -> Result<Response, (StatusCode, Json<Value>)> {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let attempts: (i64, i64) = db
        .query_row(
            "SELECT count,window_start FROM auth_failures WHERE username=?1",
            [&input.username],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((0, 0));
    if attempts.0 >= 8 && now() - attempts.1 < 600 {
        return Err(error(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many login attempts. Try again later",
        ));
    }
    let found = db.query_row(
        "SELECT id,password_hash,enabled FROM users WHERE username=?1",
        [&input.username],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
            ))
        },
    );
    let (id, hash, enabled) = match found {
        Ok(value) => value,
        Err(_) => {
            record_login_failure(&db, &input.username);
            return Err(error(StatusCode::UNAUTHORIZED, "Invalid credentials"));
        }
    };
    let parsed = PasswordHash::new(&hash).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Invalid stored password hash",
        )
    })?;
    if !enabled
        || Argon2::default()
            .verify_password(input.password.as_bytes(), &parsed)
            .is_err()
    {
        record_login_failure(&db, &input.username);
        return Err(error(StatusCode::UNAUTHORIZED, "Invalid credentials"));
    }
    let _ = db.execute(
        "DELETE FROM auth_failures WHERE username=?1",
        [&input.username],
    );
    let raw = token();
    let session_seconds = setting_u64(&db, "session_hours", 24).clamp(1, 72) as i64 * 3600;
    db.execute(
        "INSERT INTO sessions (token_hash,user_id,expires_at) VALUES (?1,?2,?3)",
        params![session_hash(&raw), id, now() + session_seconds],
    )
    .map_err(db_error)?;
    audit(&db, &input.username, "login", "Successful login");
    let secure = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        == Some("https");
    let cookie = format!(
        "donatello_session={raw}; HttpOnly; SameSite=Strict; Path=/; Max-Age={session_seconds}{}",
        if secure { "; Secure" } else { "" }
    );
    Ok(([(header::SET_COOKIE, cookie)], Json(json!({"ok": true}))).into_response())
}
fn record_login_failure(db: &Connection, username: &str) {
    if username.len() > 32 {
        return;
    }
    let _ = db.execute("INSERT INTO auth_failures(username,count,window_start) VALUES (?1,1,?2) ON CONFLICT(username) DO UPDATE SET count=CASE WHEN ?2-window_start>=600 THEN 1 ELSE count+1 END,window_start=CASE WHEN ?2-window_start>=600 THEN ?2 ELSE window_start END", params![username,now()]);
}
async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PasswordChange>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let user = require_user(&db, &headers)?;
    if input.new_password.len() < 12 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "New password must have at least 12 characters",
        ));
    }
    let hash: String = db
        .query_row(
            "SELECT password_hash FROM users WHERE id=?1",
            [user.id],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    let parsed = PasswordHash::new(&hash).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Invalid stored password hash",
        )
    })?;
    if Argon2::default()
        .verify_password(input.current_password.as_bytes(), &parsed)
        .is_err()
    {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "Current password is incorrect",
        ));
    }
    let new_hash = Argon2::default()
        .hash_password(
            input.new_password.as_bytes(),
            &SaltString::generate(&mut OsRng),
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Password hashing failed"))?
        .to_string();
    db.execute(
        "UPDATE users SET password_hash=?1 WHERE id=?2",
        params![new_hash, user.id],
    )
    .map_err(db_error)?;
    if let Some(raw) = cookie_token(&headers) {
        db.execute(
            "DELETE FROM sessions WHERE user_id=?1 AND token_hash<>?2",
            params![user.id, session_hash(&raw)],
        )
        .map_err(db_error)?;
    }
    audit(
        &db,
        &user.username,
        "account.password",
        "Password changed; other sessions revoked",
    );
    Ok(Json(json!({"ok":true})))
}

fn change_password_cli() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut username = String::new();
    let mut new_password = String::new();
    for value in [&mut username, &mut new_password] {
        if reader.read_line(value)? == 0 {
            return Err(
                io::Error::new(io::ErrorKind::UnexpectedEof, "Missing password input").into(),
            );
        }
        while value.ends_with('\n') || value.ends_with('\r') {
            value.pop();
        }
    }
    if new_password.len() < 12 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "New password must have at least 12 characters",
        )
        .into());
    }

    let db_path = env::var("DONATELLO_DB").unwrap_or_else(|_| "data/donatello.db".into());
    let db = Connection::open(db_path)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    let user_id = db.query_row(
        "SELECT id FROM users WHERE username=?1 AND enabled=1",
        [&username],
        |row| row.get::<_, i64>(0),
    );
    let user_id = match user_id {
        Ok(user_id) => user_id,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Account not found or disabled",
            )
            .into());
        }
        Err(error) => return Err(error.into()),
    };
    let new_hash = Argon2::default()
        .hash_password(new_password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map_err(|_| io::Error::other("Password hashing failed"))?
        .to_string();
    db.execute(
        "UPDATE users SET password_hash=?1 WHERE id=?2",
        params![new_hash, user_id],
    )?;
    db.execute("DELETE FROM sessions WHERE user_id=?1", [user_id])?;
    audit(
        &db,
        &username,
        "account.password",
        "Password reset by root; all sessions revoked",
    );
    println!("Password reset successfully. Sign in again with the new password.");
    Ok(())
}
async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, Json<Value>)> {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let user = require_user(&db, &headers)?;
    if let Some(raw) = cookie_token(&headers) {
        let _ = db.execute(
            "DELETE FROM sessions WHERE token_hash=?1",
            [session_hash(&raw)],
        );
    }
    audit(&db, &user.username, "logout", "Session ended");
    Ok((
        [(
            header::SET_COOKIE,
            "donatello_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
        )],
        Json(json!({"ok": true})),
    )
        .into_response())
}
fn meminfo() -> HashMap<String, u64> {
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
fn network() -> Vec<Value> {
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
fn cpu_ticks() -> (u64, u64) {
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
fn process_usage() -> (u64, f64) {
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
#[cfg(unix)]
fn disk_usage() -> (u64, u64) {
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let root = std::ffi::CString::new("/").unwrap();
    if unsafe { libc::statvfs(root.as_ptr(), &mut stat) } == 0 {
        (
            stat.f_blocks.saturating_mul(stat.f_frsize),
            stat.f_bavail.saturating_mul(stat.f_frsize),
        )
    } else {
        (0, 0)
    }
}
#[cfg(not(unix))]
fn disk_usage() -> (u64, u64) {
    (0, 0)
}
fn listening_ports() -> Vec<Value> {
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
fn network_totals() -> (u64, u64) {
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
fn setting_u64(db: &Connection, key: &str, default: u64) -> u64 {
    db.query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
        row.get::<_, String>(0)
    })
    .ok()
    .and_then(|value| value.parse().ok())
    .unwrap_or(default)
}
fn monitoring_config(db: &Connection) -> Value {
    json!({
        "enabled": setting_u64(db, "history_enabled", 1) == 1,
        "interval_seconds": setting_u64(db, "history_interval", 5),
        "retention_days": setting_u64(db, "history_retention_days", 7),
        "session_hours": setting_u64(db, "session_hours", 24)
    })
}
#[derive(Deserialize)]
struct MonitoringConfig {
    enabled: bool,
    interval_seconds: u64,
    retention_days: u64,
    session_hours: u64,
}
async fn get_monitoring_config(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    Ok(Json(monitoring_config(&db)))
}
async fn put_monitoring_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<MonitoringConfig>,
) -> ApiResult {
    check_origin(&headers)?;
    if ![5, 10, 15, 30, 60].contains(&input.interval_seconds)
        || ![1, 3, 7, 14, 30].contains(&input.retention_days)
        || ![1, 6, 12, 24, 72].contains(&input.session_hours)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Invalid monitoring settings",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    for (key, value) in [
        ("history_enabled", if input.enabled { 1 } else { 0 }),
        ("history_interval", input.interval_seconds),
        ("history_retention_days", input.retention_days),
        ("session_hours", input.session_hours),
    ] {
        db.execute("INSERT INTO settings(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value.to_string()]).map_err(db_error)?;
    }
    db.execute(
        "DELETE FROM metric_samples WHERE at<?1",
        [now() - (input.retention_days * 86400) as i64],
    )
    .map_err(db_error)?;
    audit(
        &db,
        &actor.username,
        "monitoring.config",
        &format!(
            "enabled={} interval={}s retention={}d session={}h",
            input.enabled, input.interval_seconds, input.retention_days, input.session_hours
        ),
    );
    Ok(Json(monitoring_config(&db)))
}
#[derive(Deserialize)]
struct PanelPortInput {
    port: u16,
}
async fn get_panel_port(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    let port = env::var("DONATELLO_BIND")
        .ok()
        .and_then(|bind| {
            bind.rsplit(':')
                .next()
                .and_then(|part| part.parse::<u16>().ok())
        })
        .unwrap_or(8282);
    Ok(Json(json!({"port": port})))
}
async fn change_panel_port(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PanelPortInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if input.port < 1024 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Choose a port from 1024 to 65535",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let output = std::process::Command::new("sudo")
        .args([
            "-n",
            "/usr/local/lib/donatello/port-helper",
            &input.port.to_string(),
        ])
        .output()
        .map_err(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Panel port helper unavailable; update Donatello",
            )
        })?;
    if !output.status.success() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Panel port change could not be scheduled; check the port and service logs",
        ));
    }
    audit(
        &db,
        &actor.username,
        "panel.port",
        &format!("scheduled HTTP port {}", input.port),
    );
    Ok(Json(json!({"scheduled": true, "port": input.port})))
}
async fn collect_metrics(state: AppState) {
    let mut previous_cpu = cpu_ticks();
    let mut previous_network = network_totals();
    let mut previous_at = Instant::now();
    let mut sample_count = 0u64;
    loop {
        let (enabled, interval_seconds, retention_days) = {
            let db = state.lock().unwrap();
            (
                setting_u64(&db, "history_enabled", 1) == 1,
                setting_u64(&db, "history_interval", 5).clamp(5, 60),
                setting_u64(&db, "history_retention_days", 7).clamp(1, 30),
            )
        };
        tokio::time::sleep(std::time::Duration::from_secs(interval_seconds)).await;
        let at = now();
        let current_cpu = cpu_ticks();
        let current_network = network_totals();
        let elapsed = previous_at.elapsed().as_secs_f64().max(0.1);
        previous_at = Instant::now();
        let total = current_cpu.0.saturating_sub(previous_cpu.0);
        let idle = current_cpu.1.saturating_sub(previous_cpu.1);
        let cpu = if total > 0 {
            (100.0 * (1.0 - idle as f64 / total as f64)).clamp(0.0, 100.0)
        } else {
            0.0
        };
        let mem = meminfo();
        let memory_total = *mem.get("MemTotal").unwrap_or(&0);
        let memory_available = *mem.get("MemAvailable").unwrap_or(&0);
        let memory = if memory_total > 0 {
            (100.0 * (1.0 - memory_available as f64 / memory_total as f64)).clamp(0.0, 100.0)
        } else {
            0.0
        };
        let (disk_total, disk_free) = disk_usage();
        let disk = if disk_total > 0 {
            (100.0 * (1.0 - disk_free as f64 / disk_total as f64)).clamp(0.0, 100.0)
        } else {
            0.0
        };
        let rx_rate = current_network.0.saturating_sub(previous_network.0) as f64 / elapsed;
        let tx_rate = current_network.1.saturating_sub(previous_network.1) as f64 / elapsed;
        previous_cpu = current_cpu;
        previous_network = current_network;
        sample_count = sample_count.wrapping_add(1);
        if enabled {
            if let Ok(db) = state.lock() {
                if setting_u64(&db, "history_enabled", 1) != 1 {
                    continue;
                }
                let _ = db.execute("INSERT OR REPLACE INTO metric_samples(at,cpu,memory,disk,rx_rate,tx_rate) VALUES (?1,?2,?3,?4,?5,?6)", params![at,cpu,memory,disk,rx_rate,tx_rate]);
                if sample_count % 60 == 0 {
                    let _ = db.execute(
                        "DELETE FROM metric_samples WHERE at<?1",
                        [at - (retention_days * 86400) as i64],
                    );
                }
            }
        }
    }
}
#[derive(Deserialize)]
struct HistoryQuery {
    range: Option<String>,
}
async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HistoryQuery>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    let (seconds, step) = match query.range.as_deref() {
        Some("1h") => (3600, 3),
        Some("24h") => (86400, 24),
        Some("7d") => (7 * 86400, 180),
        _ => (300, 1),
    };
    let mut stmt = db.prepare("SELECT at,cpu,memory,disk,rx_rate,tx_rate FROM metric_samples WHERE at>=?1 ORDER BY at").map_err(db_error)?;
    let rows = stmt.query_map([now() - seconds], |row| Ok(json!({
        "at": row.get::<_, i64>(0)?, "cpu": row.get::<_, f64>(1)?, "memory": row.get::<_, f64>(2)?, "disk": row.get::<_, f64>(3)?,
        "rx": row.get::<_, f64>(4)?, "tx": row.get::<_, f64>(5)?
    }))).map_err(db_error)?;
    let samples: Vec<Value> = rows
        .enumerate()
        .filter_map(|(index, row)| if index % step == 0 { row.ok() } else { None })
        .collect();
    Ok(Json(
        json!({"samples": samples, "config": monitoring_config(&db)}),
    ))
}
#[derive(Deserialize)]
struct AlertSettings {
    cpu: u8,
    memory: u8,
    disk: u8,
}
fn read_alert_settings(db: &Connection) -> Value {
    let read = |key: &str| -> u8 {
        db.query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
            row.get::<_, String>(0)
        })
        .ok()
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(85)
    };
    json!({"cpu": read("alert_cpu"), "memory": read("alert_memory"), "disk": read("alert_disk")})
}
async fn alert_settings(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    Ok(Json(read_alert_settings(&db)))
}
async fn update_alert_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AlertSettings>,
) -> ApiResult {
    check_origin(&headers)?;
    if [input.cpu, input.memory, input.disk]
        .iter()
        .any(|value| !(1..=100).contains(value))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Thresholds must be between 1 and 100",
        ));
    }
    let db = state.lock().unwrap();
    let user = require_admin(&db, &headers)?;
    for (key, value) in [
        ("alert_cpu", input.cpu),
        ("alert_memory", input.memory),
        ("alert_disk", input.disk),
    ] {
        db.execute("INSERT INTO settings(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value.to_string()]).map_err(db_error)?;
    }
    audit(
        &db,
        &user.username,
        "monitoring_settings",
        &format!(
            "CPU {}%, memory {}%, disk {}%",
            input.cpu, input.memory, input.disk
        ),
    );
    Ok(Json(read_alert_settings(&db)))
}
#[derive(Deserialize)]
struct PortProbe {
    port: u16,
}
async fn port_probe(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PortProbe>,
) -> ApiResult {
    check_origin(&headers)?;
    {
        let db = state.lock().unwrap();
        require_user(&db, &headers)?;
    }
    if input.port == 0 {
        return Err(error(StatusCode::BAD_REQUEST, "Invalid port"));
    }
    let target = SocketAddr::from(([127, 0, 0, 1], input.port));
    let open = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        tokio::net::TcpStream::connect(target),
    )
    .await
    .is_ok_and(|result| result.is_ok());
    Ok(Json(
        json!({"port": input.port, "open": open, "target": "127.0.0.1"}),
    ))
}
#[derive(Clone, Deserialize, Serialize)]
struct ManagedFirewallRule {
    id: i64,
    port: u16,
    protocol: String,
    policy: String,
    enabled: bool,
}
#[derive(Deserialize)]
struct FirewallRuleInput {
    port: u16,
    protocol: String,
    policy: String,
    enabled: bool,
}
fn valid_firewall_rule(input: &FirewallRuleInput) -> bool {
    input.port > 0
        && ![22, 80, 443, 8282, 8443].contains(&input.port)
        && ["tcp", "udp"].contains(&input.protocol.as_str())
        && ["allow", "deny"].contains(&input.policy.as_str())
}
fn firewall_rule_validation_error(input: &FirewallRuleInput) -> Option<&'static str> {
    if input.port == 0 {
        Some("شمارهٔ پورت باید بین 1 تا 65535 باشد.")
    } else if [22, 80, 443, 8282, 8443].contains(&input.port) {
        Some("این پورت برای دسترسی SSH، وب یا پنل Donatello محافظت شده است.")
    } else if !["tcp", "udp"].contains(&input.protocol.as_str()) {
        Some("پروتکل TCP یا UDP را انتخاب کن.")
    } else if !["allow", "deny"].contains(&input.policy.as_str()) {
        Some("مشخص کن این قانون ترافیک را مجاز یا مسدود کند.")
    } else {
        None
    }
}
fn read_managed_rule(
    db: &Connection,
    id: i64,
) -> Result<ManagedFirewallRule, (StatusCode, Json<Value>)> {
    db.query_row(
        "SELECT id,port,protocol,policy,enabled FROM managed_firewall_rules WHERE id=?1",
        [id],
        |row| {
            Ok(ManagedFirewallRule {
                id: row.get(0)?,
                port: row.get(1)?,
                protocol: row.get(2)?,
                policy: row.get(3)?,
                enabled: row.get(4)?,
            })
        },
    )
    .map_err(|_| error(StatusCode::NOT_FOUND, "Firewall rule not found"))
}
fn privileged_request(request: Value) -> Result<Value, (StatusCode, Json<Value>)> {
    let mut stream = StdUnixStream::connect(PRIVILEGED_SOCKET).map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Privileged manager unavailable; run sudo donatello update",
        )
    })?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(180)))
        .ok();
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(5)))
        .ok();
    serde_json::to_writer(&mut stream, &request).map_err(|_| {
        error(
            StatusCode::BAD_GATEWAY,
            "Could not send privileged operation",
        )
    })?;
    stream.write_all(b"\n").map_err(|_| {
        error(
            StatusCode::BAD_GATEWAY,
            "Could not send privileged operation",
        )
    })?;
    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .map_err(|_| {
            error(
                StatusCode::BAD_GATEWAY,
                "Privileged manager did not respond",
            )
        })?;
    serde_json::from_str(&response).map_err(|_| {
        error(
            StatusCode::BAD_GATEWAY,
            "Invalid response from privileged manager",
        )
    })
}
fn run_firewall_helper(
    action: &str,
    port: u16,
    protocol: &str,
    policy: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    let response = privileged_request(
        json!({"action": action, "port": port, "protocol": protocol, "policy": policy}),
    )?;
    if response["ok"] == true {
        Ok(())
    } else {
        Err(error(
            StatusCode::BAD_REQUEST,
            "UFW change failed; check active rules and protected ports",
        ))
    }
}
async fn managed_firewall_rules(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    // Import rules created by earlier Donatello releases so they can be edited here.
    if let Ok(output) = privileged_request(json!({"action":"status"})) {
        if output["installed"] == true {
            for line in output["status"]
                .as_str()
                .unwrap_or_default()
                .lines()
                .filter(|line| line.contains("# DONATELLO WEB"))
            {
                let tokens: Vec<_> = line.split_whitespace().collect();
                let port_protocol = tokens
                    .iter()
                    .find(|token| token.contains('/'))
                    .copied()
                    .unwrap_or("");
                if let Some((port, protocol)) = port_protocol.split_once('/') {
                    if let Ok(port) = port.parse::<u16>() {
                        let policy = if tokens.contains(&"DENY") {
                            "deny"
                        } else {
                            "allow"
                        };
                        let input = FirewallRuleInput {
                            port,
                            protocol: protocol.to_lowercase(),
                            policy: policy.into(),
                            enabled: true,
                        };
                        if valid_firewall_rule(&input) {
                            let _ = db.execute("INSERT OR IGNORE INTO managed_firewall_rules(port,protocol,policy,enabled) VALUES (?1,?2,?3,1)", params![input.port,input.protocol,input.policy]);
                        }
                    }
                }
            }
        }
    }
    let mut stmt = db.prepare("SELECT id,port,protocol,policy,enabled FROM managed_firewall_rules ORDER BY port,protocol").map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ManagedFirewallRule {
                id: row.get(0)?,
                port: row.get(1)?,
                protocol: row.get(2)?,
                policy: row.get(3)?,
                enabled: row.get(4)?,
            })
        })
        .map_err(db_error)?;
    Ok(Json(
        json!({"rules": rows.collect::<Result<Vec<_>,_>>().map_err(db_error)?}),
    ))
}
async fn add_firewall_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<FirewallRuleInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if let Some(message) = firewall_rule_validation_error(&input) {
        return Err(error(StatusCode::BAD_REQUEST, message));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    if db
        .query_row(
            "SELECT 1 FROM managed_firewall_rules WHERE port=?1 AND protocol=?2",
            params![input.port, input.protocol],
            |row| row.get::<_, i32>(0),
        )
        .is_ok()
    {
        return Err(error(
            StatusCode::CONFLICT,
            "A rule for this port and protocol already exists",
        ));
    }
    if input.enabled {
        run_firewall_helper("add", input.port, &input.protocol, &input.policy)?;
    }
    let changed = db.execute(
        "INSERT INTO managed_firewall_rules(port,protocol,policy,enabled) VALUES (?1,?2,?3,?4)",
        params![input.port, input.protocol, input.policy, input.enabled],
    );
    if changed.is_err() {
        if input.enabled {
            let _ = run_firewall_helper("remove", input.port, &input.protocol, &input.policy);
        }
        return Err(error(
            StatusCode::CONFLICT,
            "A rule for this port and protocol already exists",
        ));
    }
    let rule = read_managed_rule(&db, db.last_insert_rowid())?;
    audit(
        &db,
        &actor.username,
        "firewall.create",
        &format!(
            "{}/{} {} enabled={}",
            rule.port, rule.protocol, rule.policy, rule.enabled
        ),
    );
    Ok(Json(json!({"rule": rule})))
}
async fn update_firewall_rule(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<FirewallRuleInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if let Some(message) = firewall_rule_validation_error(&input) {
        return Err(error(StatusCode::BAD_REQUEST, message));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let old = read_managed_rule(&db, id)?;
    if db
        .query_row(
            "SELECT 1 FROM managed_firewall_rules WHERE port=?1 AND protocol=?2 AND id<>?3",
            params![input.port, input.protocol, id],
            |row| row.get::<_, i32>(0),
        )
        .is_ok()
    {
        return Err(error(
            StatusCode::CONFLICT,
            "Port and protocol already used",
        ));
    }
    let same =
        old.port == input.port && old.protocol == input.protocol && old.policy == input.policy;
    if old.enabled && (!same || !input.enabled) {
        run_firewall_helper("remove", old.port, &old.protocol, &old.policy)?;
    }
    if input.enabled && (!same || !old.enabled) {
        if let Err(err) = run_firewall_helper("add", input.port, &input.protocol, &input.policy) {
            if old.enabled && !same {
                let _ = run_firewall_helper("add", old.port, &old.protocol, &old.policy);
            }
            return Err(err);
        }
    }
    if let Err(err) = db.execute(
        "UPDATE managed_firewall_rules SET port=?1,protocol=?2,policy=?3,enabled=?4 WHERE id=?5",
        params![input.port, input.protocol, input.policy, input.enabled, id],
    ) {
        if input.enabled && (!same || !old.enabled) {
            let _ = run_firewall_helper("remove", input.port, &input.protocol, &input.policy);
        }
        if old.enabled && (!same || !input.enabled) {
            let _ = run_firewall_helper("add", old.port, &old.protocol, &old.policy);
        }
        return Err(if matches!(&err, rusqlite::Error::SqliteFailure(_, _)) {
            error(StatusCode::CONFLICT, "Port and protocol already used")
        } else {
            db_error(err)
        });
    }
    let rule = read_managed_rule(&db, id)?;
    audit(
        &db,
        &actor.username,
        "firewall.update",
        &format!(
            "id={id} {}/{} {} enabled={}",
            rule.port, rule.protocol, rule.policy, rule.enabled
        ),
    );
    Ok(Json(json!({"rule": rule})))
}
async fn delete_firewall_rule(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let rule = read_managed_rule(&db, id)?;
    if rule.enabled {
        run_firewall_helper("remove", rule.port, &rule.protocol, &rule.policy)?;
    }
    db.execute("DELETE FROM managed_firewall_rules WHERE id=?1", [id])
        .map_err(db_error)?;
    audit(
        &db,
        &actor.username,
        "firewall.delete",
        &format!("id={id} {}/{}", rule.port, rule.protocol),
    );
    Ok(Json(json!({"ok": true})))
}
async fn dashboard(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_user(&db, &headers)?;
    }
    let mem = meminfo();
    let uptime = fs::read_to_string("/proc/uptime")
        .unwrap_or_default()
        .split_whitespace()
        .next()
        .and_then(|x| x.parse::<f64>().ok())
        .unwrap_or(0.0);
    let load = fs::read_to_string("/proc/loadavg")
        .unwrap_or_default()
        .split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");
    let hostname = fs::read_to_string("/etc/hostname")
        .unwrap_or_default()
        .trim()
        .to_owned();
    let (cpu_total, cpu_idle) = cpu_ticks();
    let (process_rss_bytes, process_cpu_percent) = process_usage();
    let (disk_total_bytes, disk_free_bytes) = disk_usage();
    Ok(Json(json!({
        "hostname": hostname,
        "uptime_seconds": uptime as u64,
        "load": load,
        "memory_kib": mem.get("MemTotal"),
        "available_kib": mem.get("MemAvailable"),
        "memory_details": {
            "free_kib": mem.get("MemFree"),
            "buffers_kib": mem.get("Buffers"),
            "cached_kib": mem.get("Cached"),
            "active_kib": mem.get("Active"),
            "inactive_kib": mem.get("Inactive"),
            "dirty_kib": mem.get("Dirty"),
            "slab_kib": mem.get("Slab")
        },
        "swap_total_kib": mem.get("SwapTotal"),
        "swap_free_kib": mem.get("SwapFree"),
        "cpu_total": cpu_total,
        "cpu_idle": cpu_idle,
        "cpu_cores": std::thread::available_parallelism().map(|count| count.get()).unwrap_or(1),
        "process_rss_bytes": process_rss_bytes,
        "process_cpu_percent": process_cpu_percent,
        "disk_total_bytes": disk_total_bytes,
        "disk_free_bytes": disk_free_bytes,
        "interfaces": network(),
        "ports": listening_ports()
    })))
}
async fn users(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let mut query = db
        .prepare("SELECT id,username,role,enabled FROM users ORDER BY id")
        .map_err(db_error)?;
    let rows = query
        .query_map([], |r| {
            Ok(User {
                id: r.get(0)?,
                username: r.get(1)?,
                role: r.get(2)?,
                enabled: r.get(3)?,
            })
        })
        .map_err(db_error)?;
    let list = rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
    Ok(Json(json!({"users": list})))
}
async fn add_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<NewUser>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    if !["admin", "operator", "viewer"].contains(&input.role.as_str())
        || input.username.len() < 3
        || input.username.len() > 32
        || !input
            .username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        || input.password.len() < 12
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Invalid username, role or password",
        ));
    }
    let hash = Argon2::default()
        .hash_password(input.password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Password hashing failed"))?
        .to_string();
    db.execute(
        "INSERT INTO users (username,password_hash,role,enabled) VALUES (?1,?2,?3,1)",
        params![input.username, hash, input.role],
    )
    .map_err(|_| error(StatusCode::CONFLICT, "Username already exists"))?;
    audit(&db, &actor.username, "user.create", &input.username);
    Ok(Json(json!({"ok":true})))
}
async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<UserUpdate>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    if !["admin", "operator", "viewer"].contains(&input.role.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "Invalid role"));
    }
    if id == actor.id && (!input.enabled || input.role != "admin") {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Cannot remove your own admin access",
        ));
    }
    let target_role: String = db
        .query_row("SELECT role FROM users WHERE id=?1", [id], |r| r.get(0))
        .map_err(|_| error(StatusCode::NOT_FOUND, "User not found"))?;
    if target_role == "admin" && (!input.enabled || input.role != "admin") {
        let admin_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM users WHERE role='admin' AND enabled=1",
                [],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        if admin_count <= 1 {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "Cannot remove the last active admin",
            ));
        }
    }
    let changed = db
        .execute(
            "UPDATE users SET role=?1,enabled=?2 WHERE id=?3",
            params![input.role, input.enabled, id],
        )
        .map_err(db_error)?;
    if changed == 0 {
        return Err(error(StatusCode::NOT_FOUND, "User not found"));
    }
    if !input.enabled {
        let _ = db.execute("DELETE FROM sessions WHERE user_id=?1", [id]);
    }
    audit(
        &db,
        &actor.username,
        "user.update",
        &format!("id={id} role={} enabled={}", input.role, input.enabled),
    );
    Ok(Json(json!({"ok":true})))
}
async fn audit_log(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let mut query = db
        .prepare("SELECT at,actor,action,detail FROM audit ORDER BY id DESC LIMIT 100")
        .map_err(db_error)?;
    let rows = query.query_map([], |r| Ok(json!({"at":r.get::<_,i64>(0)?,"actor":r.get::<_,String>(1)?,"action":r.get::<_,String>(2)?,"detail":r.get::<_,String>(3)?}))).map_err(db_error)?;
    Ok(Json(
        json!({"events":rows.collect::<Result<Vec<_>,_>>().map_err(db_error)?}),
    ))
}
async fn firewall(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_user(&db, &headers)?;
    }
    let output = privileged_request(json!({"action":"status"}));
    Ok(Json(match output {
        Ok(value) => value,
        Err((_, detail)) => {
            json!({"available":false,"installed":false,"status":"UFW could not be queried","error":detail.0["error"].as_str().unwrap_or("Privileged manager unavailable")})
        }
    }))
}
#[derive(Deserialize)]
struct FirewallStateInput {
    enabled: bool,
}
fn run_ufw_admin_action(action: &str) -> Result<(), (StatusCode, Json<Value>)> {
    let output = privileged_request(json!({"action":action}))?;
    if output["ok"] == true {
        Ok(())
    } else {
        Err(error(
            StatusCode::BAD_REQUEST,
            output["error"].as_str().unwrap_or("UFW operation failed"),
        ))
    }
}
async fn update_firewall_state(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<FirewallStateInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let action = if input.enabled { "enable" } else { "disable" };
    run_ufw_admin_action(action)?;
    audit(&db, &actor.username, "firewall.state", action);
    Ok(Json(json!({"enabled": input.enabled})))
}
async fn install_firewall(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    run_ufw_admin_action("install")?;
    audit(
        &db,
        &actor.username,
        "firewall.install",
        "UFW installed from web panel",
    );
    Ok(Json(json!({"installed": true})))
}
fn update_status() -> Value {
    let output = match privileged_request(json!({"action":"update-check"})) {
        Ok(output) if output["ok"] == true => {
            output["stdout"].as_str().unwrap_or_default().to_owned()
        }
        Ok(output) => {
            return json!({"status":"unavailable", "error":output["error"].as_str().unwrap_or("Update check failed")})
        }
        Err((_, detail)) => {
            return json!({"status":"unavailable", "error":detail.0["error"].as_str().unwrap_or("Privileged manager unavailable")})
        }
    };
    let revisions: Vec<_> = output.split_whitespace().collect();
    if revisions.len() != 2 {
        return json!({"status":"unavailable"});
    }
    let local = revisions[0];
    let remote = revisions[1];
    if local.len() != 40 || remote.len() != 40 {
        return json!({"status":"unavailable"});
    }
    json!({"status": if local == remote { "current" } else { "available" }, "local": &local[..7], "remote": &remote[..7]})
}
async fn panel_update_status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    Ok(Json(update_status()))
}
async fn schedule_panel_update(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let output = privileged_request(json!({"action":"update-run"}))?;
    if output["ok"] != true {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Update could not be scheduled; check service logs",
        ));
    }
    audit(
        &db,
        &actor.username,
        "panel.update",
        "scheduled from web panel",
    );
    Ok(Json(json!({"scheduled": true})))
}
#[derive(Deserialize)]
struct NginxSiteContentInput {
    content: String,
}
#[derive(Deserialize)]
struct NginxSiteEnabledInput {
    enabled: bool,
}

fn nginx_manager_request(request: Value) -> Result<Value, (StatusCode, Json<Value>)> {
    let response = privileged_request(request)?;
    if response["ok"] == true {
        Ok(response["data"].clone())
    } else {
        Err(error(
            StatusCode::BAD_REQUEST,
            response["error"]
                .as_str()
                .unwrap_or("Nginx operation failed"),
        ))
    }
}

async fn nginx_sites(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let result = nginx_manager_request(json!({"action":"nginx-list"}))?;
    Ok(Json(result))
}

async fn get_nginx_site(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    if !valid_nginx_site_name(&name) {
        return Err(error(StatusCode::BAD_REQUEST, "نام سایت معتبر نیست."));
    }
    let result = nginx_manager_request(json!({"action":"nginx-get","name":name}))?;
    Ok(Json(result))
}

async fn save_nginx_site(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(input): Json<NginxSiteContentInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&name) {
        return Err(error(StatusCode::BAD_REQUEST, "نام سایت معتبر نیست."));
    }
    if input.content.len() > 512 * 1024 {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "تنظیمات سایت نمی‌تواند بیشتر از 512 KB باشد.",
        ));
    }
    let result =
        nginx_manager_request(json!({"action":"nginx-save","name":name,"content":input.content}))?;
    let db = state.lock().unwrap();
    audit(&db, &actor, "nginx.site.edit", &format!("site={name}"));
    Ok(Json(result))
}

async fn toggle_nginx_site(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(input): Json<NginxSiteEnabledInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&name) {
        return Err(error(StatusCode::BAD_REQUEST, "نام سایت معتبر نیست."));
    }
    let result = nginx_manager_request(
        json!({"action":"nginx-toggle","name":name,"enabled":input.enabled}),
    )?;
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor,
        "nginx.site.toggle",
        &format!("site={name} enabled={}", input.enabled),
    );
    Ok(Json(result))
}
async fn static_file(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let file = if path.is_empty() { "index.html" } else { path };
    match Frontend::get(file) {
        Some(asset) => {
            let mime = mime_guess::from_path(file).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref().to_owned())],
                asset.data,
            )
                .into_response()
        }
        None if !path.starts_with("api/") && !path.contains('.') => static_file_index(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
fn static_file_index() -> Response {
    match Frontend::get("index.html") {
        Some(asset) => (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            asset.data,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn privileged_peer_allowed(stream: &UnixStream) -> bool {
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut credentials as *mut _ as *mut libc::c_void,
            &mut length,
        )
    };
    if result != 0 || length as usize != std::mem::size_of::<libc::ucred>() {
        return false;
    }
    static DONATELLO_UID: OnceLock<Option<libc::uid_t>> = OnceLock::new();
    DONATELLO_UID
        .get_or_init(|| {
            let user = std::ffi::CString::new("donatello").unwrap();
            unsafe {
                let entry = libc::getpwnam(user.as_ptr());
                (!entry.is_null()).then(|| (*entry).pw_uid)
            }
        })
        .is_some_and(|expected_uid| credentials.uid == expected_uid)
}

fn valid_nginx_site_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn nginx_site_paths(name: &str) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    if !valid_nginx_site_name(name) {
        return Err("نام سایت معتبر نیست.".into());
    }
    let available_dir = std::path::Path::new(NGINX_AVAILABLE_DIR);
    let enabled_dir = std::path::Path::new(NGINX_ENABLED_DIR);
    if !available_dir.is_dir() || !enabled_dir.is_dir() {
        return Err("Nginx نصب نیست یا پوشه‌های sites-available و sites-enabled پیدا نشدند.".into());
    }
    Ok((available_dir.join(name), enabled_dir.join(name)))
}

fn nginx_site_enabled(name: &str) -> Result<bool, String> {
    let (available, enabled) = nginx_site_paths(name)?;
    let available_meta = fs::symlink_metadata(&available).map_err(|_| "فایل سایت پیدا نشد.")?;
    if !available_meta.file_type().is_file() {
        return Err("فایل سایت باید یک فایل معمولی در sites-available باشد.".into());
    }
    match fs::symlink_metadata(&enabled) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
        Ok(meta) if !meta.file_type().is_symlink() => {
            Err("مسیر sites-enabled اشغال شده و symlink معتبر نیست.".into())
        }
        Ok(_) => {
            let target = fs::canonicalize(&enabled)
                .map_err(|_| "symlink سایت شکسته یا مقصد آن نامعتبر است.")?;
            let expected = fs::canonicalize(&available).map_err(|error| error.to_string())?;
            if target == expected {
                Ok(true)
            } else {
                Err("symlink موجود به فایل sites-available همین سایت اشاره نمی‌کند.".into())
            }
        }
    }
}

fn nginx_config_test() -> Result<(), String> {
    let output = std::process::Command::new("nginx")
        .arg("-t")
        .output()
        .map_err(|error| format!("اجرای nginx -t ناموفق بود: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if detail.is_empty() {
            "اعتبارسنجی تنظیمات Nginx ناموفق بود.".into()
        } else {
            detail
        })
    }
}

fn nginx_reload_if_active() -> Result<(), String> {
    let active = std::process::Command::new("systemctl")
        .args(["is-active", "--quiet", "nginx"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !active {
        return Ok(());
    }
    let output = std::process::Command::new("systemctl")
        .args(["reload", "nginx"])
        .output()
        .map_err(|error| format!("بارگذاری مجدد Nginx ناموفق بود: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if detail.is_empty() {
            "بارگذاری مجدد Nginx ناموفق بود.".into()
        } else {
            detail
        })
    }
}

fn nginx_list_sites() -> Result<Value, String> {
    let available_dir = std::path::Path::new(NGINX_AVAILABLE_DIR);
    if !available_dir.is_dir() {
        return Err("Nginx نصب نیست یا sites-available پیدا نشد.".into());
    }
    let mut sites = Vec::new();
    for entry in fs::read_dir(available_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !valid_nginx_site_name(&name) {
            continue;
        }
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        if metadata.len() > 512 * 1024 {
            continue;
        }
        let enabled = nginx_site_enabled(&name)?;
        sites.push(json!({"name":name,"enabled":enabled,"bytes":metadata.len()}));
    }
    sites.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    Ok(json!(sites))
}

fn nginx_get_site(name: &str) -> Result<Value, String> {
    let (available, _) = nginx_site_paths(name)?;
    let metadata = fs::symlink_metadata(&available).map_err(|_| "فایل سایت پیدا نشد.")?;
    if !metadata.file_type().is_file() {
        return Err("فایل سایت باید یک فایل معمولی باشد.".into());
    }
    if metadata.len() > 512 * 1024 {
        return Err("حجم تنظیمات سایت از حد مجاز بیشتر است.".into());
    }
    let content = fs::read_to_string(&available).map_err(|error| error.to_string())?;
    Ok(
        json!({"name":name,"enabled":nginx_site_enabled(name)?,"content":content,"bytes":metadata.len()}),
    )
}

fn nginx_atomic_write(path: &std::path::Path, content: &[u8], mode: u32) -> Result<(), String> {
    let parent = path.parent().ok_or("مسیر تنظیمات نامعتبر است.")?;
    let file_name = path
        .file_name()
        .and_then(|part| part.to_str())
        .ok_or("نام فایل نامعتبر است.")?;
    let temp = parent.join(format!(".donatello-{file_name}-{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| error.to_string())?;
        file.write_all(content).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::set_permissions(&temp, fs::Permissions::from_mode(mode & 0o777))
            .map_err(|error| error.to_string())?;
        fs::rename(&temp, path).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn nginx_save_site(name: &str, content: &str) -> Result<Value, String> {
    if content.len() > 512 * 1024 || content.contains('\0') {
        return Err("تنظیمات خالی، نامعتبر یا بزرگ‌تر از 512 KB است.".into());
    }
    let (available, _) = nginx_site_paths(name)?;
    let metadata = fs::symlink_metadata(&available).map_err(|_| "فایل سایت پیدا نشد.")?;
    if !metadata.file_type().is_file() {
        return Err("فقط فایل معمولی داخل sites-available قابل ویرایش است.".into());
    }
    let old_content = fs::read(&available).map_err(|error| error.to_string())?;
    let enabled = nginx_site_enabled(name)?;
    nginx_atomic_write(
        &available,
        content.as_bytes(),
        metadata.permissions().mode(),
    )?;
    if let Err(error) = nginx_config_test() {
        let _ = nginx_atomic_write(&available, &old_content, metadata.permissions().mode());
        return Err(format!(
            "تنظیمات ذخیره نشد و نسخهٔ قبلی بازگردانده شد: {error}"
        ));
    }
    if enabled {
        if let Err(error) = nginx_reload_if_active() {
            let _ = nginx_atomic_write(&available, &old_content, metadata.permissions().mode());
            let _ = nginx_reload_if_active();
            return Err(format!(
                "بارگذاری تنظیمات ناموفق بود؛ نسخهٔ قبلی بازگردانده شد: {error}"
            ));
        }
    }
    Ok(
        json!({"name":name,"enabled":enabled,"message":if enabled { "تنظیمات ذخیره و بررسی شد." } else { "تنظیمات ذخیره و بررسی شد؛ سایت غیرفعال است." }}),
    )
}

fn nginx_set_site_enabled(name: &str, enabled: bool) -> Result<Value, String> {
    let (available, link) = nginx_site_paths(name)?;
    let metadata = fs::symlink_metadata(&available).map_err(|_| "فایل سایت پیدا نشد.")?;
    if !metadata.file_type().is_file() {
        return Err("فقط فایل معمولی داخل sites-available قابل فعال‌سازی است.".into());
    }
    let was_enabled = nginx_site_enabled(name)?;
    if was_enabled == enabled {
        return Ok(json!({"name":name,"enabled":enabled,"message":"وضعیت سایت تغییری نکرد."}));
    }
    if enabled {
        std::os::unix::fs::symlink(format!("../sites-available/{name}"), &link)
            .map_err(|error| format!("ساخت symlink خودکار ناموفق بود: {error}"))?;
    } else {
        fs::remove_file(&link).map_err(|error| format!("حذف symlink ناموفق بود: {error}"))?;
    }
    let apply = nginx_config_test().and_then(|_| nginx_reload_if_active());
    if let Err(error) = apply {
        if enabled {
            let _ = fs::remove_file(&link);
        } else {
            let _ = std::os::unix::fs::symlink(format!("../sites-available/{name}"), &link);
        }
        return Err(format!(
            "تغییر اعمال نشد و وضعیت قبلی بازگردانده شد: {error}"
        ));
    }
    Ok(
        json!({"name":name,"enabled":enabled,"message":if enabled { "سایت فعال شد و symlink ساخته شد." } else { "سایت غیرفعال شد و symlink حذف شد." }}),
    )
}

fn execute_nginx_request(action: &str, request: &Value) -> Value {
    let result = match action {
        "nginx-list" => nginx_list_sites().map(|sites| json!({"sites":sites})),
        "nginx-get" => request["name"]
            .as_str()
            .ok_or_else(|| "نام سایت ارسال نشده است.".to_owned())
            .and_then(nginx_get_site)
            .map(|site| json!({"site":site})),
        "nginx-save" => request["name"]
            .as_str()
            .ok_or_else(|| "نام سایت ارسال نشده است.".to_owned())
            .and_then(|name| {
                request["content"]
                    .as_str()
                    .ok_or_else(|| "متن تنظیمات ارسال نشده است.".to_owned())
                    .and_then(|content| nginx_save_site(name, content))
            })
            .map(|site| json!({"site":site})),
        "nginx-toggle" => request["name"]
            .as_str()
            .ok_or_else(|| "نام سایت ارسال نشده است.".to_owned())
            .and_then(|name| {
                request["enabled"]
                    .as_bool()
                    .ok_or_else(|| "وضعیت سایت ارسال نشده است.".to_owned())
                    .and_then(|enabled| nginx_set_site_enabled(name, enabled))
            })
            .map(|site| json!({"site":site})),
        _ => Err("درخواست مدیریت Nginx پشتیبانی نمی‌شود.".to_owned()),
    };
    match result {
        Ok(value) => json!({"ok":true,"data":value}),
        Err(error) => json!({"ok":false,"error":error}),
    }
}

fn execute_privileged_request(request: &Value) -> Value {
    let action = match request["action"].as_str() {
        Some("status") => "status",
        Some("install") => "install",
        Some("enable") => "enable",
        Some("disable") => "disable",
        Some("add") => "add",
        Some("remove") => "remove",
        Some("update-check") => "update-check",
        Some("update-run") => "update-run",
        Some("nginx-list") => "nginx-list",
        Some("nginx-get") => "nginx-get",
        Some("nginx-save") => "nginx-save",
        Some("nginx-toggle") => "nginx-toggle",
        _ => return json!({"ok":false,"error":"Unsupported privileged action"}),
    };
    if action.starts_with("nginx-") {
        let _guard = NGINX_OPERATION_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        return execute_nginx_request(action, request);
    }
    let mut args = vec![action.to_owned()];
    if action == "add" || action == "remove" {
        let Some(port) = request["port"]
            .as_u64()
            .filter(|port| (1..=65535).contains(port))
        else {
            return json!({"ok":false,"error":"Invalid port"});
        };
        let Some(protocol) = request["protocol"]
            .as_str()
            .filter(|value| ["tcp", "udp"].contains(value))
        else {
            return json!({"ok":false,"error":"Invalid protocol"});
        };
        let Some(policy) = request["policy"]
            .as_str()
            .filter(|value| ["allow", "deny"].contains(value))
        else {
            return json!({"ok":false,"error":"Invalid policy"});
        };
        args.extend([port.to_string(), protocol.to_owned(), policy.to_owned()]);
    }
    let (helper, helper_args) = if action.starts_with("update-") {
        (
            "/usr/local/lib/donatello/update-helper",
            vec![action.trim_start_matches("update-")],
        )
    } else {
        (
            "/usr/local/lib/donatello/ufw-helper",
            args.iter().map(String::as_str).collect(),
        )
    };
    let output = match std::process::Command::new(helper)
        .args(helper_args)
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return json!({"ok":false,"available":false,"installed":false,"status":"","error":error.to_string()})
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if action == "status" {
        let installed = output.status.success();
        return json!({"ok":true,"available":installed,"installed":installed,"status":stdout,"error":if installed { "" } else { "UFW is not installed" }});
    }
    if output.status.success() {
        json!({"ok":true,"stdout":stdout})
    } else {
        json!({"ok":false,"error":if stderr.is_empty() { "UFW operation failed" } else { &stderr }})
    }
}

async fn serve_privileged_connection(mut stream: UnixStream) {
    if !privileged_peer_allowed(&stream) {
        return;
    }
    let mut line = String::new();
    let response = match AsyncBufReader::new(&mut stream).read_line(&mut line).await {
        Ok(size) if size > 0 && size <= 4 * 1024 * 1024 => {
            match serde_json::from_str::<Value>(&line) {
                Ok(request) => execute_privileged_request(&request),
                Err(_) => json!({"ok":false,"error":"Invalid request"}),
            }
        }
        _ => json!({"ok":false,"error":"Invalid request"}),
    };
    let _ = stream.write_all(format!("{}\n", response).as_bytes()).await;
}

async fn run_privileged_helper() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(PRIVILEGED_SOCKET);
    if path.exists() {
        fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o660))?;
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(serve_privileged_connection(stream));
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::args().nth(1).as_deref() == Some("privileged-helper") {
        return run_privileged_helper().await;
    }
    if env::args().nth(1).as_deref() == Some("change-password") {
        return change_password_cli();
    }
    let db_path = env::var("DONATELLO_DB").unwrap_or_else(|_| "data/donatello.db".into());
    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let db = Connection::open(&db_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&db_path, fs::Permissions::from_mode(0o600))?;
    }
    db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY,username TEXT NOT NULL UNIQUE,password_hash TEXT NOT NULL,role TEXT NOT NULL,enabled INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS sessions (token_hash TEXT PRIMARY KEY,user_id INTEGER NOT NULL REFERENCES users(id),expires_at INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS audit (id INTEGER PRIMARY KEY,at INTEGER NOT NULL,actor TEXT NOT NULL,action TEXT NOT NULL,detail TEXT NOT NULL); CREATE TABLE IF NOT EXISTS auth_failures (username TEXT PRIMARY KEY,count INTEGER NOT NULL,window_start INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS metric_samples (at INTEGER PRIMARY KEY,cpu REAL NOT NULL,memory REAL NOT NULL,disk REAL NOT NULL,rx_rate REAL NOT NULL,tx_rate REAL NOT NULL); CREATE TABLE IF NOT EXISTS managed_firewall_rules (id INTEGER PRIMARY KEY,port INTEGER NOT NULL,protocol TEXT NOT NULL,policy TEXT NOT NULL,enabled INTEGER NOT NULL,UNIQUE(port,protocol));")?;
    let has_users = initialized(&db);
    let configured_setup_token = env::var("DONATELLO_SETUP_TOKEN")
        .ok()
        .filter(|value| !value.is_empty());
    if !has_users && configured_setup_token.is_none() {
        let generated = token();
        let inserted = db.execute(
            "INSERT OR IGNORE INTO settings(key,value) VALUES ('setup_token',?1)",
            [&generated],
        )?;
        if inserted > 0 {
            println!("DONATELLO_SETUP_CODE={generated}");
        }
    }
    db.execute("DELETE FROM sessions WHERE expires_at<=?1", [now()])?;
    let state = Arc::new(Mutex::new(db));
    tokio::spawn(collect_metrics(state.clone()));
    let app = Router::new()
        .route("/api/state", get(state_handler))
        .route("/api/setup", post(setup))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/account/password", post(change_password))
        .route("/api/dashboard", get(dashboard))
        .route("/api/history", get(history))
        .route(
            "/api/monitoring/alerts",
            get(alert_settings).put(update_alert_settings),
        )
        .route(
            "/api/monitoring/config",
            get(get_monitoring_config).put(put_monitoring_config),
        )
        .route(
            "/api/settings/port",
            get(get_panel_port).put(change_panel_port),
        )
        .route(
            "/api/update",
            get(panel_update_status).post(schedule_panel_update),
        )
        .route("/api/network/probe", post(port_probe))
        .route("/api/users", get(users).post(add_user))
        .route("/api/users/{id}", put(update_user))
        .route("/api/audit", get(audit_log))
        .route("/api/nginx/sites", get(nginx_sites))
        .route(
            "/api/nginx/sites/{name}",
            get(get_nginx_site).put(save_nginx_site),
        )
        .route("/api/nginx/sites/{name}/enabled", put(toggle_nginx_site))
        .route("/api/firewall", get(firewall).put(update_firewall_state))
        .route("/api/firewall/install", post(install_firewall))
        .route(
            "/api/firewall/rules",
            get(managed_firewall_rules).post(add_firewall_rule),
        )
        .route(
            "/api/firewall/rules/{id}",
            put(update_firewall_rule).delete(delete_firewall_rule),
        )
        .fallback(get(static_file))
        .with_state(state);
    let bind = env::var("DONATELLO_BIND").unwrap_or_else(|_| "127.0.0.1:8282".into());
    let addr: SocketAddr = bind.parse()?;
    println!("Donatello listening at http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let tls_cert = env::var("DONATELLO_TLS_CERT").ok();
    let tls_key = env::var("DONATELLO_TLS_KEY").ok();
    match (tls_cert, tls_key) {
        (Some(cert), Some(key)) => {
            rustls::crypto::ring::default_provider()
                .install_default()
                .map_err(|_| "Failed to configure the TLS crypto provider")?;
            let tls_bind = env::var("DONATELLO_TLS_BIND")
                .unwrap_or_else(|_| "0.0.0.0:443".into())
                .parse::<SocketAddr>()?;
            let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key).await?;
            println!("Donatello listening at https://{tls_bind}");
            let https_app = app.clone().layer(axum::middleware::from_fn(
                |mut request: axum::extract::Request, next: axum::middleware::Next| async move {
                    if !request.headers().contains_key(axum::http::header::HOST) {
                        if let Some(authority) = request.uri().authority() {
                            if let Ok(host) = axum::http::HeaderValue::from_str(authority.as_str())
                            {
                                request.headers_mut().insert(axum::http::header::HOST, host);
                            }
                        }
                    }
                    request.headers_mut().insert(
                        axum::http::header::HeaderName::from_static("x-forwarded-proto"),
                        axum::http::HeaderValue::from_static("https"),
                    );
                    next.run(request).await
                },
            ));
            let https_app = https_app.into_make_service_with_connect_info::<SocketAddr>();
            let https_server = axum_server::bind_rustls(tls_bind, tls).serve(https_app);
            let http_app = app.into_make_service_with_connect_info::<SocketAddr>();
            tokio::select! {
                result = axum::serve(listener, http_app) => result?,
                result = https_server => result?,
            }
        }
        (None, None) => {
            let http_app = app.into_make_service_with_connect_info::<SocketAddr>();
            axum::serve(listener, http_app).await?;
        }
        _ => return Err("Both DONATELLO_TLS_CERT and DONATELLO_TLS_KEY must be set".into()),
    }
    Ok(())
}
async fn state_handler(state: State<AppState>, headers: HeaderMap) -> ApiResult {
    self::state(state, headers).await
}
