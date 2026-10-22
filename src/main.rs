use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{ConnectInfo, Path, State},
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
use std::{
    collections::HashMap,
    env, fs,
    io::{self, BufRead, BufReader},
    net::SocketAddr,
    sync::{Arc, Mutex, OnceLock},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

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
    db.execute(
        "INSERT INTO sessions (token_hash,user_id,expires_at) VALUES (?1,?2,?3)",
        params![session_hash(&raw), id, now() + 86400],
    )
    .map_err(db_error)?;
    audit(&db, &input.username, "login", "Successful login");
    let secure = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        == Some("https");
    let cookie = format!(
        "donatello_session={raw}; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400{}",
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
    let mut command = tokio::process::Command::new("sudo");
    command
        .args(["-n", "ufw", "status", "numbered"])
        .kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(5), command.output()).await;
    Ok(Json(match output {
        Ok(Ok(o)) => {
            json!({"available":o.status.success(),"status":String::from_utf8_lossy(&o.stdout),"error":String::from_utf8_lossy(&o.stderr)})
        }
        Ok(Err(_)) => json!({"available":false,"status":"UFW is not installed"}),
        Err(_) => json!({"available":false,"status":"UFW timed out"}),
    }))
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY,username TEXT NOT NULL UNIQUE,password_hash TEXT NOT NULL,role TEXT NOT NULL,enabled INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS sessions (token_hash TEXT PRIMARY KEY,user_id INTEGER NOT NULL REFERENCES users(id),expires_at INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS audit (id INTEGER PRIMARY KEY,at INTEGER NOT NULL,actor TEXT NOT NULL,action TEXT NOT NULL,detail TEXT NOT NULL); CREATE TABLE IF NOT EXISTS auth_failures (username TEXT PRIMARY KEY,count INTEGER NOT NULL,window_start INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY,value TEXT NOT NULL);")?;
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
    let app = Router::new()
        .route("/api/state", get(state_handler))
        .route("/api/setup", post(setup))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/account/password", post(change_password))
        .route("/api/dashboard", get(dashboard))
        .route("/api/users", get(users).post(add_user))
        .route("/api/users/{id}", put(update_user))
        .route("/api/audit", get(audit_log))
        .route("/api/firewall", get(firewall))
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
