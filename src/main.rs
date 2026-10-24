use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, ConnectInfo, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode, Uri},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    extract::Request, Json, Router,
};
use rand::RngCore;
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use rusqlite::{params, Connection, OptionalExtension};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream as StdUnixStream;
use std::os::unix::{fs::{FileTypeExt, MetadataExt, PermissionsExt}, io::AsRawFd, process::CommandExt};
use std::{
    collections::{HashMap, HashSet},
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

const PRIVILEGED_SOCKET: &str = "/run/arg-privileged/manager.sock";
const NGINX_AVAILABLE_DIR: &str = "/etc/nginx/sites-available";
const NGINX_ENABLED_DIR: &str = "/etc/nginx/sites-enabled";
const UPDATE_STATUS_FILE: &str = "/run/arg-update-in-progress";
static NGINX_OPERATION_LOCK: Mutex<()> = Mutex::new(());
static UPDATE_OPERATION_GATE: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());
static APT_OPERATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static PROCESS_INSTANCE_ID: OnceLock<String> = OnceLock::new();
static ROOT_TERMINAL_PIDS: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();

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
#[derive(Deserialize)]
struct UsernameChange {
    username: String,
    current_password: String,
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

fn update_started_at() -> Option<i64> {
    let value = fs::read_to_string(UPDATE_STATUS_FILE).ok()?;
    let started = value.split_whitespace().next()?.parse::<i64>().ok()?;
    (now().saturating_sub(started) <= 21600).then_some(started)
}

fn update_progress_payload(raw: Option<&str>, marker_started: Option<i64>, instance: &str) -> Value {
    let mut job = raw.and_then(|text| serde_json::from_str::<Value>(text).ok())
        .unwrap_or_else(|| json!({"state":if marker_started.is_some() {"running"} else {"idle"},"phase":"unknown","started_at":marker_started,"job_id":null,"message":""}));
    let state_running = job["state"].as_str() == Some("running");
    if state_running && marker_started.is_none() {
        job["state"] = json!("failed");
        job["phase"] = json!("failed");
        job["message"] = json!("The update process ended without recording a final result. Review its logs before retrying.");
    }
    job["updating"] = json!(marker_started.is_some() && state_running);
    job["instance"] = json!(instance);
    if job["started_at"].is_null() {
        if let Some(started) = marker_started { job["started_at"] = json!(started); }
    }
    job
}

fn ensure_sessions_metadata(db: &Connection) -> Result<(), rusqlite::Error> {
    let mut statement = db.prepare("PRAGMA table_info(sessions)")?;
    let existing: Vec<String> = statement.query_map([], |row| row.get(1))?
        .filter_map(Result::ok).collect();
    drop(statement);
    for (name, definition) in [
        ("created_at", "INTEGER"),
        ("last_seen", "INTEGER"),
        ("remote_ip", "TEXT"),
        ("user_agent", "TEXT"),
    ] {
        if !existing.iter().any(|column| column == name) {
            db.execute_batch(&format!("ALTER TABLE sessions ADD COLUMN {name} {definition}"))?;
        }
    }
    Ok(())
}

async fn update_guard(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path();
    let exempt = matches!(path, "/api/update" | "/api/update/progress" | "/api/state" | "/api/logout" | "/api/login");
    let writes_api = method != axum::http::Method::GET && method != axum::http::Method::HEAD && path.starts_with("/api/") && !exempt;
    let opens_terminal = path == "/api/terminal";
    if writes_api || opens_terminal {
        let _operation = UPDATE_OPERATION_GATE.read().await;
        if update_started_at().is_some() {
            return (StatusCode::CONFLICT, Json(json!({"error":"ARG_UPDATE_IN_PROGRESS","message":"ARG is being updated. Please wait until the update finishes."}))).into_response();
        }
        return next.run(request).await;
    }
    next.run(request).await
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
                .strip_prefix("arg_session=")
                .map(str::to_owned)
        })
}
fn require_user(db: &Connection, headers: &HeaderMap) -> Result<User, (StatusCode, Json<Value>)> {
    let raw =
        cookie_token(headers).ok_or_else(|| error(StatusCode::UNAUTHORIZED, "Login required"))?;
    let hash = session_hash(&raw);
    let user = db.query_row("SELECT u.id,u.username,u.role,u.enabled FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=?1 AND s.expires_at>?2 AND u.enabled=1", params![hash, now()], |row| Ok(User { id: row.get(0)?, username: row.get(1)?, role: row.get(2)?, enabled: row.get(3)? })).map_err(|_| error(StatusCode::UNAUTHORIZED, "Session expired"))?;
    let current = now();
    let _ = db.execute("UPDATE sessions SET last_seen=?1 WHERE token_hash=?2 AND (last_seen IS NULL OR last_seen<?3)", params![current, hash, current - 60]);
    Ok(user)
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
    let expected_token = env::var("ARG_SETUP_TOKEN").ok().or_else(|| {
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
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
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
    let created_at = now();
    let user_agent = headers.get(header::USER_AGENT).and_then(|value| value.to_str().ok())
        .map(|value| value.chars().filter(|ch| !ch.is_control()).take(256).collect::<String>());
    db.execute(
        "INSERT INTO sessions (token_hash,user_id,expires_at,created_at,last_seen,remote_ip,user_agent) VALUES (?1,?2,?3,?4,?4,?5,?6)",
        params![session_hash(&raw), id, created_at + session_seconds, created_at, remote.ip().to_string(), user_agent],
    )
    .map_err(db_error)?;
    audit(&db, &input.username, "login", "Successful login");
    let secure = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        == Some("https");
    let cookie = format!(
        "arg_session={raw}; HttpOnly; SameSite=Strict; Path=/; Max-Age={session_seconds}{}",
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

async fn change_username(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<UsernameChange>,
) -> ApiResult {
    check_origin(&headers)?;
    if input.username.len() < 3 || input.username.len() > 32
        || !input.username.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
        return Err(error(StatusCode::BAD_REQUEST, "Username must be 3-32 ASCII letters, digits, '_' or '-'"));
    }
    let db = state.lock().unwrap();
    let user = require_user(&db, &headers)?;
    let hash: String = db.query_row("SELECT password_hash FROM users WHERE id=?1", [user.id], |row| row.get(0)).map_err(db_error)?;
    let parsed = PasswordHash::new(&hash).map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Invalid stored password hash"))?;
    if Argon2::default().verify_password(input.current_password.as_bytes(), &parsed).is_err() {
        return Err(error(StatusCode::UNAUTHORIZED, "Current password is incorrect"));
    }
    match db.execute("UPDATE users SET username=?1 WHERE id=?2", params![input.username, user.id]) {
        Ok(_) => {},
        Err(rusqlite::Error::SqliteFailure(ref code, _)) if code.code == rusqlite::ErrorCode::ConstraintViolation => {
            return Err(error(StatusCode::CONFLICT, "That username is already in use"));
        }
        Err(_) => return Err(db_error(rusqlite::Error::InvalidQuery)),
    }
    audit(&db, &input.username, "account.username", "Username changed");
    Ok(Json(json!({"ok":true,"username":input.username})))
}

async fn list_account_sessions(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    let user = require_user(&db, &headers)?;
    let current_hash = cookie_token(&headers).map(|raw| session_hash(&raw)).unwrap_or_default();
    let mut statement = db.prepare("SELECT rowid,created_at,last_seen,expires_at,remote_ip,user_agent,token_hash FROM sessions WHERE user_id=?1 AND expires_at>?2 ORDER BY (token_hash=?3) DESC, COALESCE(last_seen,created_at,0) DESC").map_err(db_error)?;
    let rows = statement.query_map(params![user.id, now(), current_hash], |row| Ok(json!({
        "id":row.get::<_,i64>(0)?,"created_at":row.get::<_,Option<i64>>(1)?,"last_seen":row.get::<_,Option<i64>>(2)?,
        "expires_at":row.get::<_,i64>(3)?,"remote_ip":row.get::<_,Option<String>>(4)?,"user_agent":row.get::<_,Option<String>>(5)?,
        "current":row.get::<_,String>(6)? == current_hash
    }))).map_err(db_error)?;
    let sessions: Vec<Value> = rows.filter_map(Result::ok).collect();
    Ok(Json(json!({"sessions":sessions})))
}

async fn revoke_account_session(State(state): State<AppState>, Path(id): Path<i64>, headers: HeaderMap) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let user = require_user(&db, &headers)?;
    let current_hash = cookie_token(&headers).map(|raw| session_hash(&raw)).unwrap_or_default();
    let target_hash: Option<String> = db.query_row("SELECT token_hash FROM sessions WHERE rowid=?1 AND user_id=?2", params![id, user.id], |row| row.get(0)).optional().map_err(db_error)?;
    let Some(target_hash) = target_hash else { return Err(error(StatusCode::NOT_FOUND, "Session not found")); };
    if target_hash == current_hash { return Err(error(StatusCode::BAD_REQUEST, "Use sign out to close the current session")); }
    db.execute("DELETE FROM sessions WHERE rowid=?1 AND user_id=?2", params![id, user.id]).map_err(db_error)?;
    audit(&db, &user.username, "account.session.revoke", "Another account session was revoked");
    Ok(Json(json!({"ok":true})))
}

async fn revoke_other_account_sessions(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let user = require_user(&db, &headers)?;
    let current_hash = cookie_token(&headers).map(|raw| session_hash(&raw)).unwrap_or_default();
    let removed = db.execute("DELETE FROM sessions WHERE user_id=?1 AND token_hash<>?2", params![user.id, current_hash]).map_err(db_error)?;
    audit(&db, &user.username, "account.sessions.revoke_others", "Other account sessions were revoked");
    Ok(Json(json!({"ok":true,"revoked":removed})))
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

    let db_path = env::var("ARG_DB").unwrap_or_else(|_| "data/arg.db".into());
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
            "arg_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
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
fn os_pretty_name() -> String {
    fs::read_to_string("/etc/os-release").unwrap_or_default().lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME=").map(|value| value.trim_matches('"').to_owned()))
        .unwrap_or_else(|| "Unknown Linux distribution".into())
}
fn cpu_model_name() -> Option<String> {
    fs::read_to_string("/proc/cpuinfo").ok()?.lines().find_map(|line| {
        line.split_once(':').filter(|(key, _)| matches!(key.trim(), "model name" | "Hardware" | "Processor"))
            .map(|(_, value)| value.trim().to_owned())
    }).filter(|value| !value.is_empty())
}
fn is_public_ip(value: &str) -> bool {
    let Ok(ip) = value.parse::<std::net::IpAddr>() else { return false; };
    match ip {
        std::net::IpAddr::V4(ip) => {
            let [a,b,c,_] = ip.octets();
            !ip.is_private() && !ip.is_loopback() && !ip.is_link_local() && !ip.is_broadcast()
                && !ip.is_unspecified() && !ip.is_multicast()
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 192 && b == 0 && c == 2)
                && !(a == 198 && (b == 18 || b == 19))
                && !(a == 198 && b == 51 && c == 100)
                && !(a == 203 && b == 0 && c == 113)
        }
        std::net::IpAddr::V6(ip) => {
            let segments = ip.segments();
            !ip.is_loopback() && !ip.is_unspecified() && !ip.is_multicast() && !ip.is_unicast_link_local()
                && (segments[0] & 0xfe00) != 0xfc00
                && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
        }
    }
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
fn process_instance_id() -> &'static str {
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

fn network_interface_counters() -> HashMap<String, (u64, u64, bool)> {
    let mut totals = HashMap::new();
    if let Ok(entries) = fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "lo" { continue; }
            let path = entry.path();
            let read_counter = |key: &str| fs::read_to_string(path.join("statistics").join(key)).ok().and_then(|value| value.trim().parse::<u64>().ok()).unwrap_or(0);
            let rx = read_counter("rx_bytes");
            let tx = read_counter("tx_bytes");
            let virtual_name = ["veth", "docker", "br-", "virbr", "cni", "flannel", "podman", "lxc", "zt"].iter().any(|prefix| name.starts_with(prefix));
            let hardware_like = ["eth", "en", "eno", "ens", "enp", "wlan", "bond"].iter().any(|prefix| name.starts_with(prefix));
            let eligible = !path.join("master").exists() && !virtual_name && (path.join("device").exists() || hardware_like);
            totals.insert(name, (rx, tx, eligible));
        }
    }
    totals
}

fn split_traffic_delta(from: i64, to: i64, rx: u64, tx: u64) -> Vec<(i64, u64, u64)> {
    if from < 0 || to <= from { return Vec::new(); }
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
        let rx_part = if is_last { rx.saturating_sub(rx_used) } else { ((rx as u128 * span as u128) / duration as u128) as u64 };
        let tx_part = if is_last { tx.saturating_sub(tx_used) } else { ((tx as u128 * span as u128) / duration as u128) as u64 };
        result.push((bucket, rx_part, tx_part));
        rx_used = rx_used.saturating_add(rx_part);
        tx_used = tx_used.saturating_add(tx_part);
        cursor = end;
    }
    result
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
    db.execute("DELETE FROM network_traffic_hourly WHERE hour_start<?1", [now() - (input.retention_days * 86400) as i64]).map_err(db_error)?;
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
    let port = env::var("ARG_BIND")
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
            "/usr/local/lib/arg/port-helper",
            &input.port.to_string(),
        ])
        .output()
        .map_err(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Panel port helper unavailable; update ARG",
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
    let mut previous_traffic = network_interface_counters();
    let mut previous_traffic_at = now();
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
        let current_traffic = network_interface_counters();
        let traffic_from = previous_traffic_at;
        let traffic_gap_is_reliable = at > traffic_from && at - traffic_from <= (interval_seconds as i64 * 2 + 2);
        let mut traffic_rows = Vec::new();
        if traffic_gap_is_reliable {
            for (name, (rx, tx, eligible)) in &current_traffic {
                if let Some((previous_rx, previous_tx, _)) = previous_traffic.get(name) {
                    let rx_delta = if rx >= previous_rx { rx - previous_rx } else { 0 };
                    let tx_delta = if tx >= previous_tx { tx - previous_tx } else { 0 };
                    for (hour, rx_bytes, tx_bytes) in split_traffic_delta(traffic_from, at, rx_delta, tx_delta) {
                        traffic_rows.push((hour, name.clone(), rx_bytes, tx_bytes, *eligible));
                    }
                }
            }
        }
        previous_traffic = current_traffic;
        previous_traffic_at = at;
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
                for (hour, interface, rx_bytes, tx_bytes, eligible) in traffic_rows {
                    let _ = db.execute("INSERT INTO network_traffic_hourly(hour_start,interface,rx_bytes,tx_bytes,eligible_for_total) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(hour_start,interface) DO UPDATE SET rx_bytes=network_traffic_hourly.rx_bytes+excluded.rx_bytes,tx_bytes=network_traffic_hourly.tx_bytes+excluded.tx_bytes,eligible_for_total=MAX(network_traffic_hourly.eligible_for_total,excluded.eligible_for_total)", params![hour,interface,rx_bytes.min(i64::MAX as u64) as i64,tx_bytes.min(i64::MAX as u64) as i64,if eligible {1} else {0}]);
                }
                if sample_count % 60 == 0 {
                    let _ = db.execute(
                        "DELETE FROM metric_samples WHERE at<?1",
                        [at - (retention_days * 86400) as i64],
                    );
                    let _ = db.execute("DELETE FROM network_traffic_hourly WHERE hour_start<?1", [at - (retention_days * 86400) as i64]);
                }
            }
        }
    }
}
#[derive(Deserialize)]
struct HistoryQuery {
    range: Option<String>,
    from: Option<i64>,
    to: Option<i64>,
}
async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HistoryQuery>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    let current = now();
    let (from, to, step) = match (query.from, query.to) {
        (Some(from), Some(to)) => {
            let span = to.saturating_sub(from);
            if from < 0 || to > current || from >= to || span > 366 * 86400 {
                return Err(error(StatusCode::BAD_REQUEST, "بازهٔ دلخواه تاریخ معتبر نیست یا از 366 روز بیشتر است."));
            }
            (from, to, ((span / 5 + 2499) / 2500).max(1) as usize)
        }
        (None, None) => {
            let (seconds, step) = match query.range.as_deref() {
                Some("1h") => (3600, 3),
                Some("24h") => (86400, 24),
                Some("7d") => (7 * 86400, 180),
                Some("30d") => (30 * 86400, 720),
                _ => (300, 1),
            };
            (current - seconds, current, step)
        }
        _ => return Err(error(StatusCode::BAD_REQUEST, "برای بازهٔ دلخواه، زمان شروع و پایان لازم است.")),
    };
    let mut stmt = db.prepare(
        "SELECT at,cpu,memory,disk,rx_rate,tx_rate FROM (SELECT at,cpu,memory,disk,rx_rate,tx_rate,ROW_NUMBER() OVER (ORDER BY at) AS sample_index FROM metric_samples WHERE at>=?1 AND at<=?2) WHERE (sample_index - 1) % ?3 = 0 ORDER BY at",
    ).map_err(db_error)?;
    let rows = stmt.query_map(params![from, to, step as i64], |row| Ok(json!({
        "at": row.get::<_, i64>(0)?, "cpu": row.get::<_, f64>(1)?, "memory": row.get::<_, f64>(2)?, "disk": row.get::<_, f64>(3)?,
        "rx": row.get::<_, f64>(4)?, "tx": row.get::<_, f64>(5)?
    }))).map_err(db_error)?;
    let samples: Vec<Value> = rows.filter_map(Result::ok).collect();
    Ok(Json(
        json!({"samples": samples, "config": monitoring_config(&db)}),
    ))
}

#[derive(Deserialize)]
struct TrafficVolumeQuery {
    #[serde(default)] range: Option<String>,
    #[serde(default)] granularity: Option<String>,
    #[serde(default)] from: Option<i64>,
    #[serde(default)] to: Option<i64>,
    #[serde(default)] interface: Option<String>,
}

async fn network_traffic_volume(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<TrafficVolumeQuery>) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    let current = now();
    let range = query.range.as_deref().unwrap_or("24h");
    let (from, to) = if range == "custom" {
        match (query.from, query.to) {
            (Some(from), Some(to)) if from >= 0 && from < to && to <= current && to - from <= 366 * 86400 => (from, to),
            _ => return Err(error(StatusCode::BAD_REQUEST, "بازهٔ دلخواه ترافیک معتبر نیست یا از 366 روز بیشتر است.")),
        }
    } else {
        let seconds = match range { "24h" => 86400, "7d" => 7 * 86400, "30d" => 30 * 86400, _ => return Err(error(StatusCode::BAD_REQUEST, "بازهٔ زمانی پشتیبانی نمی‌شود.")) };
        (current - seconds, current)
    };
    let granularity = query.granularity.as_deref().unwrap_or(if to - from <= 2 * 86400 { "hour" } else { "day" });
    if !["hour", "day"].contains(&granularity) || (granularity == "hour" && to - from > 90 * 86400) {
        return Err(error(StatusCode::BAD_REQUEST, "برای بازه‌های بیش از 90 روز از گروه‌بندی روزانه استفاده کن."));
    }
    let selected_interface = query.interface.clone().unwrap_or_else(|| "all".into());
    let interface = if selected_interface == "all" { None } else { Some(selected_interface.clone()) };
    if interface.as_ref().is_some_and(|name| name.len() > 32 || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_.:-".contains(&byte))) {
        return Err(error(StatusCode::BAD_REQUEST, "نام کارت شبکه معتبر نیست."));
    }
    let bucket_expr = if granularity == "hour" { "hour_start" } else { "(hour_start / 86400) * 86400" };
    let sql = format!("SELECT {bucket_expr},SUM(rx_bytes),SUM(tx_bytes),COUNT(DISTINCT interface) FROM network_traffic_hourly WHERE hour_start>=?1 AND hour_start<?2 AND (?3 IS NULL OR interface=?3) AND (?4=0 OR eligible_for_total=1) GROUP BY 1 ORDER BY 1");
    let mut statement = db.prepare(&sql).map_err(db_error)?;
    let rows = statement.query_map(params![from, to, interface, if selected_interface == "all" {1} else {0}], |row| Ok(json!({"at":row.get::<_,i64>(0)?,"rx_bytes":row.get::<_,i64>(1)?.max(0),"tx_bytes":row.get::<_,i64>(2)?.max(0),"interfaces":row.get::<_,i64>(3)?}))).map_err(db_error)?;
    let buckets: Vec<Value> = rows.filter_map(Result::ok).collect();
    let interface_query = if selected_interface != "all" {
        let name = selected_interface.clone();
        db.prepare("SELECT DISTINCT interface FROM network_traffic_hourly WHERE hour_start>=?1 AND hour_start<?2 AND interface=?3 ORDER BY interface").and_then(|mut statement| statement.query_map(params![from,to,name], |row| row.get::<_,String>(0)).map(|rows| rows.filter_map(Result::ok).collect::<Vec<_>>())).unwrap_or_default()
    } else {
        db.prepare("SELECT DISTINCT interface FROM network_traffic_hourly WHERE hour_start>=?1 AND hour_start<?2 AND eligible_for_total=1 ORDER BY interface").and_then(|mut statement| statement.query_map(params![from,to], |row| row.get::<_,String>(0)).map(|rows| rows.filter_map(Result::ok).collect::<Vec<_>>())).unwrap_or_default()
    };
    let fresh_at = buckets.last().and_then(|bucket| bucket["at"].as_i64());
    Ok(Json(json!({"from":from,"to":to,"granularity":granularity,"interface":selected_interface,"interfaces":interface_query,"buckets":buckets,"fresh_at":fresh_at,"enabled":setting_u64(&db,"history_enabled",1)==1,"timezone":"UTC"})))
}
#[derive(Deserialize, Default)]
struct TerminalQuery {
    mode: Option<String>,
}

async fn terminal_socket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TerminalQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("http");
    let origin_matches = origin
        .and_then(|value| value.parse::<Uri>().ok())
        .is_some_and(|uri| {
            uri.authority()
                .is_some_and(|authority| Some(authority.as_str()) == host)
                && uri
                    .scheme_str()
                    .is_some_and(|origin_scheme| origin_scheme == scheme)
        });
    if !origin_matches {
        return (StatusCode::FORBIDDEN, "Invalid WebSocket origin").into_response();
    }
    let root_mode = match query.mode.as_deref().unwrap_or("user") {
        "user" => false,
        "root" => true,
        _ => return (StatusCode::BAD_REQUEST, "Invalid terminal mode").into_response(),
    };
    {
        let db = state.lock().unwrap();
        let actor = match require_admin(&db, &headers) {
            Ok(actor) => actor,
            Err(_) => return (StatusCode::FORBIDDEN, "Administrator access required").into_response(),
        };
        if root_mode {
            audit(&db, &actor.username, "terminal.root.open", "web root terminal opened");
        }
    }
    let Some(raw_token) = cookie_token(&headers) else {
        return (StatusCode::FORBIDDEN, "Login required").into_response();
    };
    let terminal_session_hash = session_hash(&raw_token);
    ws.max_message_size(16 * 1024)
        .max_frame_size(16 * 1024)
        .on_upgrade(move |socket| async move {
            run_terminal(socket, state, terminal_session_hash, root_mode).await;
        })
}

async fn run_terminal(mut socket: WebSocket, state: AppState, terminal_session_hash: String, root_mode: bool) {
    let pty = native_pty_system();
    let pair = match pty.openpty(PtySize {
        rows: 28,
        cols: 100,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(pair) => pair,
        Err(_) => return,
    };
    let master = Arc::new(Mutex::new(pair.master));
    let mut child = None;
    let mut root_pid = None;
    if root_mode {
        let Some(tty) = master.lock().unwrap().tty_name().map(|path| path.to_string_lossy().into_owned()) else { return; };
        let response = tokio::task::spawn_blocking(move || privileged_request(json!({"action":"terminal-root-start","tty":tty}))).await;
        let failure = match response {
            Ok(Ok(value)) if value["ok"] == true => { root_pid = value["pid"].as_u64().map(|pid| pid as u32); None },
            Ok(Ok(value)) => Some(value["error"].as_str().unwrap_or("Privileged manager rejected the root terminal").to_owned()),
            Ok(Err((_, body))) => Some(body.0["error"].as_str().unwrap_or("Privileged manager is unavailable").to_owned()),
            Err(_) => Some("Could not start the root terminal helper".to_owned()),
        };
        if let Some(message) = failure {
            let _ = socket.send(Message::Text(format!("\r\n\x1b[31mRoot terminal failed: {message}\x1b[0m\r\n").into())).await;
            return;
        }
    } else {
        let executable = match env::current_exe() { Ok(path) => path, Err(_) => return };
        let mut command = CommandBuilder::new(executable);
        command.arg("terminal-shell");
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.cwd(env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/var/lib/arg")));
        match pair.slave.spawn_command(command) { Ok(process) => child = Some(process), Err(_) => return }
    }
    drop(pair.slave);
    let writer_result = { master.lock().unwrap().take_writer() };
    let writer = match writer_result {
        Ok(writer) => Arc::new(Mutex::new(writer)),
        Err(_) => {
            if let Some(child) = child.as_mut() { let _ = child.kill(); }
            if let Some(pid) = root_pid { let _ = privileged_request_async(json!({"action":"terminal-root-stop","pid":pid})).await; }
            return;
        }
    };
    let reader_result = { master.lock().unwrap().try_clone_reader() };
    let reader = match reader_result {
        Ok(reader) => reader,
        Err(_) => {
            if let Some(child) = child.as_mut() { let _ = child.kill(); }
            if let Some(pid) = root_pid { let _ = privileged_request_async(json!({"action":"terminal-root-stop","pid":pid})).await; }
            return;
        }
    };
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    std::thread::spawn(move || {
        let mut reader = reader;
        let mut buffer = [0u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(size) => {
                    if output_tx.blocking_send(buffer[..size].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    let (mut sender, mut receiver) = socket.split();
    let mut update_check = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = update_check.tick() => {
                if update_started_at().is_some() { break; }
                let valid = {
                    let db = state.lock().unwrap();
                    db.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=?1 AND s.expires_at>?2 AND u.enabled=1 AND u.role='admin')", params![terminal_session_hash, now()], |row| row.get::<_, bool>(0)).unwrap_or(false)
                };
                if !valid { break; }
            },
            output = output_rx.recv() => match output {
                Some(bytes) => if sender.send(Message::Binary(bytes.into())).await.is_err() { break; },
                None => break,
            },
            incoming = receiver.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(value) = serde_json::from_str::<Value>(&text) {
                        if value.get("type").and_then(Value::as_str) == Some("resize") {
                            let cols = value.get("cols").and_then(Value::as_u64).unwrap_or(100).clamp(20, 300) as u16;
                            let rows = value.get("rows").and_then(Value::as_u64).unwrap_or(28).clamp(5, 120) as u16;
                            let _ = master.lock().unwrap().resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
                            continue;
                        }
                    }
                    if writer.lock().unwrap().write_all(text.as_bytes()).is_err() { break; }
                },
                Some(Ok(Message::Binary(bytes))) => if writer.lock().unwrap().write_all(&bytes).is_err() { break; },
                Some(Ok(Message::Ping(bytes))) => if sender.send(Message::Pong(bytes)).await.is_err() { break; },
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {},
            }
        }
    }
    if let Some(mut child) = child {
        if let Some(pid) = child.process_id().filter(|pid| *pid > 1) {
            unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    if let Some(pid) = root_pid {
        let _ = privileged_request_async(json!({"action":"terminal-root-stop","pid":pid})).await;
    }
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
        && ![22, 8282, 8443].contains(&input.port)
        && ["tcp", "udp"].contains(&input.protocol.as_str())
        && ["allow", "deny"].contains(&input.policy.as_str())
}
fn firewall_rule_validation_error(input: &FirewallRuleInput) -> Option<&'static str> {
    if input.port == 0 {
        Some("شمارهٔ پورت باید بین 1 تا 65535 باشد.")
    } else if [22, 8282, 8443].contains(&input.port) {
        Some("این پورت برای دسترسی SSH یا پنل ARG محافظت شده است. پورت‌های HTTP و HTTPS قابل مدیریت هستند.")
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
            "Privileged manager unavailable; run sudo arg update",
        )
    })?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(1800)))
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
async fn privileged_request_async(request: Value) -> Result<Value, (StatusCode, Json<Value>)> {
    tokio::task::spawn_blocking(move || privileged_request(request))
        .await
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "Privileged operation task failed"))?
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
            response["error"]
                .as_str()
                .unwrap_or("UFW change failed; check the UFW service and active rules"),
        ))
    }
}
async fn managed_firewall_rules(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    // Import rules created by earlier ARG releases so they can be edited here.
    if let Ok(output) = privileged_request(json!({"action":"status"})) {
        if output["installed"] == true {
            for line in output["status"]
                .as_str()
                .unwrap_or_default()
                .lines()
                .filter(|line| line.contains("# ARG WEB") || line.contains("# DONATELLO WEB"))
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
    if revisions.len() < 2 {
        return json!({"status":"unavailable", "error":"The update helper returned an invalid revision response. Run 'sudo arg update' to repair the updater."});
    }
    let local = revisions[0];
    let remote = revisions[1];
    if local.len() != 40 || remote.len() != 40 {
        return json!({"status":"unavailable", "error":"Could not read the installed or remote ARG revision. Check the server's GitHub connection."});
    }
    let remote_version = revisions.get(2).copied().filter(|value| *value != "unknown");
    json!({"status": if local == remote { "current" } else { "available" }, "local": &local[..7], "remote": &remote[..7], "local_version": env!("CARGO_PKG_VERSION"), "remote_version": remote_version})
}
async fn panel_update_status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_user(&db, &headers)?;
    }
    let mut status = if update_started_at().is_some() {
        json!({"status":"unavailable","error":"ARG_UPDATE_IN_PROGRESS"})
    } else {
        update_status()
    };
    status["instance"] = json!(process_instance_id());
    Ok(Json(status))
}
async fn panel_update_progress(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    drop(db);
    let fallback_started = update_started_at();
    let output = privileged_request(json!({"action":"update-progress"}));
    let raw = output.ok().and_then(|value| value["stdout"].as_str().map(str::to_owned));
    Ok(Json(update_progress_payload(raw.as_deref(), fallback_started, process_instance_id())))
}
async fn panel_update_logs(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_user(&db, &headers)?;
    }
    let output = privileged_request(json!({"action":"update-logs"}))?;
    if output["ok"] != true {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            output["error"].as_str().unwrap_or("Update details are temporarily unavailable"),
        ));
    }
    Ok(Json(json!({"logs":output["stdout"].as_str().unwrap_or("")})))
}

#[derive(Deserialize)]
struct AptOperation { action: String, #[serde(default)] packages: Vec<String> }
#[derive(Deserialize)]
struct ServiceOperation { action: String, unit: String }
#[derive(Deserialize, Default)]
struct DockerOperation {
    action: String,
    #[serde(default)] target: String,
    #[serde(default)] name: String,
    #[serde(default)] image: String,
    #[serde(default)] ports: Vec<String>,
    #[serde(default)] env: Vec<String>,
    #[serde(default)] network: String,
    #[serde(default)] restart: String,
    #[serde(default)] compose_file: String,
    #[serde(default)] context_path: String,
    #[serde(default)] command: Vec<String>,
    #[serde(default)] confirmed: bool,
}

async fn list_packages(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    { let db = state.lock().unwrap(); require_admin(&db, &headers)?; }
    let result = privileged_request_async(json!({"action":"apt-list"})).await?;
    if result["ok"] != true { return Err(error(StatusCode::SERVICE_UNAVAILABLE, result["error"].as_str().unwrap_or("Package list unavailable"))); }
    Ok(Json(result["data"].clone()))
}
async fn run_apt_operation(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<AptOperation>) -> ApiResult {
    check_origin(&headers)?;
    let actor = { let db = state.lock().unwrap(); require_admin(&db, &headers)? };
    let _update_guard = UPDATE_OPERATION_GATE.read().await;
    let _apt_guard = APT_OPERATION_LOCK.lock().await;
    let allowed = ["update", "upgrade", "full-upgrade", "autoremove", "clean", "autoclean", "install", "remove"];
    if !allowed.contains(&input.action.as_str()) || input.packages.len() > 25 || input.packages.iter().any(|p| !valid_package_name(p)) {
        return Err(error(StatusCode::BAD_REQUEST, "Invalid APT action or package name"));
    }
    if ["install", "remove"].contains(&input.action.as_str()) && input.packages.is_empty() { return Err(error(StatusCode::BAD_REQUEST, "Select at least one package")); }
    let result = privileged_request_async(json!({"action":"apt-run","operation":input.action,"packages":input.packages})).await?;
    if result["ok"] != true { return Err(error(StatusCode::BAD_GATEWAY, result["error"].as_str().unwrap_or("APT operation failed"))); }
    { let db = state.lock().unwrap(); audit(&db, &actor.username, "apt.operation", &input.action); }
    Ok(Json(json!({"output":result["stdout"].as_str().unwrap_or("")})))
}
async fn list_system_services(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    { let db = state.lock().unwrap(); require_admin(&db, &headers)?; }
    let result = privileged_request_async(json!({"action":"service-list"})).await?;
    if result["ok"] != true { return Err(error(StatusCode::SERVICE_UNAVAILABLE, result["error"].as_str().unwrap_or("Service list unavailable"))); }
    Ok(Json(result["data"].clone()))
}
async fn run_service_operation(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<ServiceOperation>) -> ApiResult {
    check_origin(&headers)?;
    let actor = { let db = state.lock().unwrap(); require_admin(&db, &headers)? };
    let _update_guard = UPDATE_OPERATION_GATE.read().await;
    let allowed = ["start", "stop", "restart", "try-restart", "reload", "reload-or-restart", "enable", "disable", "enable-now", "disable-now", "reenable", "mask", "unmask", "reset-failed", "status", "logs", "daemon-reload"];
    if !allowed.contains(&input.action.as_str()) || (input.action != "daemon-reload" && !valid_service_unit(&input.unit)) { return Err(error(StatusCode::BAD_REQUEST, "Invalid service action or unit")); }
    let result = privileged_request_async(json!({"action":"service-action","operation":input.action,"unit":input.unit})).await?;
    if result["ok"] != true { return Err(error(StatusCode::BAD_GATEWAY, result["error"].as_str().unwrap_or("Service operation failed"))); }
    { let db = state.lock().unwrap(); audit(&db, &actor.username, "systemd.service.action", &format!("{} {}", input.action, input.unit)); }
    Ok(Json(json!({"output":result["stdout"].as_str().unwrap_or("")})))
}
async fn docker_status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    { let db = state.lock().unwrap(); require_admin(&db, &headers)?; }
    let result = privileged_request_async(json!({"action":"docker-status"})).await?;
    if result["ok"] != true { return Err(error(StatusCode::SERVICE_UNAVAILABLE, result["error"].as_str().unwrap_or("Docker status is unavailable"))); }
    Ok(Json(result["data"].clone()))
}
async fn docker_operation(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<DockerOperation>) -> ApiResult {
    check_origin(&headers)?;
    let actor = { let db = state.lock().unwrap(); require_admin(&db, &headers)?.username };
    let allowed = ["install", "uninstall", "start", "stop", "restart", "enable", "disable", "container-run", "container-start", "container-stop", "container-restart", "container-pause", "container-unpause", "container-remove", "container-logs", "container-inspect", "container-exec", "image-pull", "image-push", "image-build", "image-tag", "image-remove", "image-prune", "network-create", "network-remove", "volume-create", "volume-remove", "volume-prune", "system-prune", "compose-up", "compose-down", "compose-pull"];
    if !allowed.contains(&input.action.as_str()) { return Err(error(StatusCode::BAD_REQUEST, "Docker operation is invalid")); }
    if ["uninstall", "container-remove", "image-remove", "image-push", "network-remove", "volume-remove", "image-prune", "volume-prune", "system-prune", "stop", "restart", "disable", "compose-down"].contains(&input.action.as_str()) && !input.confirmed {
        return Err(error(StatusCode::BAD_REQUEST, "Confirm this Docker operation before continuing"));
    }
    let _update_guard = UPDATE_OPERATION_GATE.read().await;
    let result = privileged_request_async(json!({"action":"docker-operation","operation":input.action,"target":input.target,"name":input.name,"image":input.image,"ports":input.ports,"env":input.env,"network":input.network,"restart":input.restart,"compose_file":input.compose_file,"context_path":input.context_path,"command":input.command,"confirmed":input.confirmed})).await?;
    if result["ok"] != true { return Err(error(StatusCode::BAD_GATEWAY, result["error"].as_str().unwrap_or("Docker operation failed"))); }
    { let db = state.lock().unwrap(); audit(&db, &actor, "docker.operation", &input.action); }
    Ok(Json(json!({"output":result["stdout"].as_str().unwrap_or("Docker operation completed.")})))
}
fn valid_package_name(name: &str) -> bool {
    let (base, arch) = name.split_once(':').unwrap_or((name, ""));
    let valid_base = !base.is_empty() && base.len() <= 128 && base.as_bytes()[0].is_ascii_alphanumeric()
        && base.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"+.-".contains(&b));
    let valid_arch = !name.contains(':') || (!arch.is_empty() && arch.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-".contains(&b)));
    valid_base && valid_arch
}
fn valid_service_unit(unit: &str) -> bool {
    unit.len() <= 255 && unit.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) && unit.ends_with(".service") && unit.bytes().all(|b| b.is_ascii_alphanumeric() || b"@_.:-".contains(&b)) && !unit.contains("..")
}
async fn schedule_panel_update(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    check_origin(&headers)?;
    let _exclusive = UPDATE_OPERATION_GATE.write().await;
    if update_started_at().is_some() {
        return Err(error(StatusCode::CONFLICT, "ARG_UPDATE_IN_PROGRESS: another update is already running."));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let output = privileged_request(json!({"action":"update-run"}))?;
    if output["ok"] != true {
        let detail = output["error"].as_str().unwrap_or("Update could not be scheduled; check service logs");
        let status = if detail.contains("ARG_UPDATE_IN_PROGRESS") { StatusCode::CONFLICT } else { StatusCode::BAD_REQUEST };
        return Err(error(status, detail));
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
    #[serde(default)]
    settings: Option<Value>,
}
#[derive(Deserialize)]
struct NginxSiteEnabledInput {
    enabled: bool,
}
#[derive(Deserialize)]
struct NginxActionInput {
    action: String,
}
#[derive(Deserialize)]
struct NginxCreateSiteInput {
    name: String,
    content: String,
    #[serde(default)]
    settings: Option<Value>,
}
#[derive(Deserialize)]
struct NginxSiteSyncInput { source: String }
#[derive(Deserialize)]
struct NginxSslIssueInput {
    email: String,
    domains: Vec<String>,
    #[serde(default)]
    redirect: bool,
}

fn nginx_content_hash(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

fn store_nginx_snapshot(db: &Connection, name: &str, content: &str, settings: Option<&Value>) -> Result<(), rusqlite::Error> {
    db.execute("INSERT INTO nginx_site_config_state(name,content,settings_json,content_hash,updated_at) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(name) DO UPDATE SET content=excluded.content,settings_json=excluded.settings_json,content_hash=excluded.content_hash,updated_at=excluded.updated_at", params![name, content, settings.map(Value::to_string), nginx_content_hash(content), now()])?;
    Ok(())
}
#[derive(Deserialize)]
struct NginxSiteRenameInput {
    new_name: String,
}
#[derive(Deserialize)]
struct NginxSiteMetadataInput {
    description: String,
}
#[derive(Deserialize)]
struct NginxSiteLogInput {
    kind: String,
}
#[derive(Deserialize)]
struct NginxDomainCheckQuery {
    hosts: String,
}
#[derive(Deserialize)]
struct NginxBrowseQuery {
    path: Option<String>,
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

fn nginx_site_content(value: &Value) -> Result<&str, (StatusCode, Json<Value>)> {
    // Current privileged-manager responses wrap a site in `site`; older installed
    // helpers returned the site object directly. Accept both wire formats so an
    // upgrade cannot turn a readable config into an empty reconciliation preview.
    value["site"]["content"]
        .as_str()
        .or_else(|| value["content"].as_str())
        .ok_or_else(|| {
        error(
            StatusCode::BAD_GATEWAY,
            "مدیر Nginx محتوای فایل سایت را در قالب قابل‌خواندن برنگرداند.",
        )
    })
}

async fn nginx_sites(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let mut result = nginx_manager_request(json!({"action":"nginx-list"}))?;
    let db = state.lock().unwrap();
    if let Some(sites) = result["sites"].as_array_mut() {
        for site in sites {
            if let Some(name) = site["name"].as_str().map(str::to_owned) {
                let description = db.query_row("SELECT description FROM nginx_site_metadata WHERE name=?1", [&name], |row| row.get::<_, String>(0)).unwrap_or_default();
                site["description"] = json!(description);
                // Read the actual file before comparing hashes. The list payload
                // is metadata-only and may come from a helper with a stale schema.
                let current = nginx_manager_request(json!({"action":"nginx-get","name":name}))?;
                let content = nginx_site_content(&current)?;
                let file_hash = nginx_content_hash(content);
                let snapshot: Option<(String,String)> = db.query_row("SELECT content_hash,content FROM nginx_site_config_state WHERE name=?1", [&name], |row| Ok((row.get(0)?,row.get(1)?))).ok();
                if let Some((hash,db_content)) = snapshot {
                    let mismatch = hash != file_hash;
                    site["sync_mismatch"] = json!(mismatch);
                    if mismatch {
                        site["db_content"] = json!(db_content);
                        site["content"] = json!(content);
                    }
                } else {
                    store_nginx_snapshot(&db, &name, content, None).map_err(db_error)?;
                    site["sync_mismatch"] = json!(false);
                }
            }
        }
    }
    Ok(Json(result))
}

async fn nginx_status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    nginx_manager_request(json!({"action":"nginx-status"})).map(Json)
}

async fn api_nginx_browse_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<NginxBrowseQuery>,
) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let result = nginx_manager_request(json!({"action":"nginx-browse", "path":query.path}))?;
    Ok(Json(result))
}

async fn nginx_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<NginxActionInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    let allowed = ["install", "start", "stop", "restart", "reload", "enable", "disable", "test", "logs"];
    if !allowed.contains(&input.action.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "عملیات Nginx معتبر نیست."));
    }
    let result = nginx_manager_request(json!({"action":format!("nginx-{}", input.action)}))?;
    let db = state.lock().unwrap();
    audit(&db, &actor, "nginx.service.action", &input.action);
    Ok(Json(result))
}

async fn create_nginx_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<NginxCreateSiteInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&input.name) || input.content.len() > 512 * 1024 {
        return Err(error(StatusCode::BAD_REQUEST, "نام یا اندازهٔ فایل سایت معتبر نیست."));
    }
    let result = nginx_manager_request(json!({"action":"nginx-create", "name":input.name, "content":input.content}))?;
    let db = state.lock().unwrap();
    db.execute("INSERT OR IGNORE INTO nginx_site_metadata(name,description) VALUES (?1,'')", [&input.name]).map_err(db_error)?;
    store_nginx_snapshot(&db, &input.name, &input.content, input.settings.as_ref()).map_err(db_error)?;
    audit(&db, &actor, "nginx.site.create", &input.name);
    Ok(Json(result))
}

async fn delete_nginx_site(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&name) { return Err(error(StatusCode::BAD_REQUEST, "نام سایت معتبر نیست.")); }
    let result = nginx_manager_request(json!({"action":"nginx-delete", "name":name}))?;
    let db = state.lock().unwrap();
    db.execute("DELETE FROM nginx_site_metadata WHERE name=?1", [&name]).map_err(db_error)?;
    db.execute("DELETE FROM nginx_site_config_state WHERE name=?1", [&name]).map_err(db_error)?;
    audit(&db, &actor, "nginx.site.delete", &name);
    Ok(Json(result))
}

async fn rename_nginx_site(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(input): Json<NginxSiteRenameInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&name) || !valid_nginx_site_name(&input.new_name) {
        return Err(error(StatusCode::BAD_REQUEST, "نام فایل سایت معتبر نیست."));
    }
    let mut result = nginx_manager_request(json!({"action":"nginx-rename", "name":name, "new_name":input.new_name}))?;
    let db = state.lock().unwrap();
    let old_description = db.query_row("SELECT description FROM nginx_site_metadata WHERE name=?1", [&name], |row| row.get::<_, String>(0)).unwrap_or_default();
    db.execute("DELETE FROM nginx_site_metadata WHERE name=?1", [&name]).map_err(db_error)?;
    db.execute("INSERT OR REPLACE INTO nginx_site_metadata(name,description) VALUES (?1,?2)", params![input.new_name, old_description]).map_err(db_error)?;
    db.execute("UPDATE nginx_site_config_state SET name=?2 WHERE name=?1", params![name,input.new_name]).map_err(db_error)?;
    result["description"] = json!(old_description);
    audit(&db, &actor, "nginx.site.rename", &format!("{name} -> {}", input.new_name));
    Ok(Json(result))
}

async fn update_nginx_site_metadata(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(input): Json<NginxSiteMetadataInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_nginx_site_name(&name) || input.description.chars().count() > 500 {
        return Err(error(StatusCode::BAD_REQUEST, "نام سایت یا طول توضیحات معتبر نیست."));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    nginx_manager_request(json!({"action":"nginx-get", "name":name}))?;
    db.execute("INSERT INTO nginx_site_metadata(name,description) VALUES (?1,?2) ON CONFLICT(name) DO UPDATE SET description=excluded.description", params![name, input.description.trim()]).map_err(db_error)?;
    audit(&db, &actor, "nginx.site.metadata", &format!("site={name}"));
    Ok(Json(json!({"name":name,"description":input.description.trim(),"message":"توضیحات سایت ذخیره شد."})))
}

async fn api_nginx_site_logs(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(input): Json<NginxSiteLogInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&name) || !["access", "error"].contains(&input.kind.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "نوع لاگ یا نام سایت معتبر نیست."));
    }
    let result = nginx_manager_request(json!({"action":"nginx-site-logs", "name":name, "kind":input.kind}))?;
    let db = state.lock().unwrap();
    audit(&db, &actor, "nginx.site.logs", &format!("site={name} kind={}", input.kind));
    Ok(Json(result))
}

async fn check_nginx_domains(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<NginxDomainCheckQuery>,
) -> ApiResult {
    check_origin(&headers)?;
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    if query.hosts.len() > 2048 {
        return Err(error(StatusCode::BAD_REQUEST, "فهرست دامنه‌ها بیش از حد طولانی است."));
    }
    let hosts: Vec<String> = query.hosts.split(|ch: char| ch == ',' || ch.is_whitespace()).filter(|host| !host.is_empty()).map(str::to_owned).collect();
    if hosts.is_empty() || hosts.len() > 20 || hosts.iter().any(|host| !valid_domain_name(host)) {
        return Err(error(StatusCode::BAD_REQUEST, "دامنه نامعتبر است یا تعداد دامنه‌ها از 20 بیشتر شده است."));
    }
    let mut results = Vec::with_capacity(hosts.len());
    for host in hosts {
        let query_host = host.strip_prefix("*.").unwrap_or(&host);
        match tokio::net::lookup_host((query_host, 80)).await {
            Ok(addresses) => {
                let mut values: Vec<String> = addresses.map(|address| address.ip().to_string()).collect();
                values.sort();
                values.dedup();
                results.push(json!({"host":host,"resolved":!values.is_empty(),"addresses":values}));
            }
            Err(err) => results.push(json!({"host":host,"resolved":false,"addresses":[],"error":err.to_string()})),
        }
    }
    Ok(Json(json!({"domains":results})))
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
    let mut result = nginx_manager_request(json!({"action":"nginx-get","name":name}))?;
    let db = state.lock().unwrap();
    let content = nginx_site_content(&result)?.to_owned();
    let snapshot: Option<(String, Option<String>, String)> = db.query_row("SELECT content,settings_json,content_hash FROM nginx_site_config_state WHERE name=?1", [&name], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).ok();
    if let Some((db_content, settings, db_hash)) = snapshot {
        let mismatch = db_hash != nginx_content_hash(&content);
        result["site"]["sync_mismatch"] = json!(mismatch);
        result["site"]["db_content"] = json!(db_content);
        result["site"]["settings"] = settings.and_then(|value| serde_json::from_str::<Value>(&value).ok()).unwrap_or(Value::Null);
    } else {
        store_nginx_snapshot(&db, &name, &content, None).map_err(db_error)?;
        result["site"]["sync_mismatch"] = json!(false);
        result["site"]["db_content"] = json!(content);
        result["site"]["settings"] = Value::Null;
    }
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
    store_nginx_snapshot(&db, &name, &input.content, input.settings.as_ref()).map_err(db_error)?;
    audit(&db, &actor, "nginx.site.edit", &format!("site={name}"));
    Ok(Json(result))
}

async fn sync_nginx_site(
    State(state): State<AppState>, Path(name): Path<String>, headers: HeaderMap,
    Json(input): Json<NginxSiteSyncInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = { let db=state.lock().unwrap(); require_admin(&db,&headers)?.username };
    if !valid_nginx_site_name(&name) || !["database","file"].contains(&input.source.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST,"منبع همگام‌سازی معتبر نیست."));
    }
    let db = state.lock().unwrap();
    if input.source == "database" {
        let (content, settings): (String, Option<String>) = db.query_row("SELECT content,settings_json FROM nginx_site_config_state WHERE name=?1", [&name], |row| Ok((row.get(0)?,row.get(1)?))).map_err(|_| error(StatusCode::CONFLICT,"نسخه‌ای در دیتابیس برای این سایت ذخیره نشده است."))?;
        nginx_manager_request(json!({"action":"nginx-save","name":name,"content":content}))?;
        let parsed = settings.and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
        store_nginx_snapshot(&db,&name,&content,parsed.as_ref()).map_err(db_error)?;
    } else {
        nginx_manager_request(json!({"action":"nginx-test"}))?;
        let file = nginx_manager_request(json!({"action":"nginx-get","name":name}))?;
        let content=nginx_site_content(&file)?;
        // Keep the visual editor's DB metadata while accepting the on-disk file.
        let settings: Option<Option<String>> = db.query_row("SELECT settings_json FROM nginx_site_config_state WHERE name=?1", [&name], |row| row.get(0)).optional().map_err(db_error)?;
        let parsed = settings.flatten().and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
        store_nginx_snapshot(&db,&name,content,parsed.as_ref()).map_err(db_error)?;
    }
    audit(&db,&actor,"nginx.site.sync",&format!("site={name} source={}",input.source));
    Ok(Json(json!({"ok":true,"source":input.source,"message":"همگام‌سازی انجام شد. کانفیگ Nginx نیز اعتبارسنجی شد."})))
}

async fn issue_nginx_site_ssl(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(input): Json<NginxSslIssueInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = { let db = state.lock().unwrap(); require_admin(&db, &headers)?.username };
    if !valid_nginx_site_name(&name) || input.domains.is_empty() || input.domains.len() > 20
        || input.domains.iter().any(|domain| !valid_domain_name(domain) || domain.starts_with("*."))
        || input.email.len() > 254 || !input.email.split_once('@').is_some_and(|(local, domain)| !local.is_empty() && valid_domain_name(domain)) {
        return Err(error(StatusCode::BAD_REQUEST, "دامنه یا ایمیل معتبر نیست؛ wildcard با HTTP-01 پشتیبانی نمی‌شود."));
    }
    let mut domains = input.domains.clone();
    domains.sort();
    domains.dedup();
    if domains.len() != input.domains.len() {
        return Err(error(StatusCode::BAD_REQUEST, "دامنه تکراری در فهرست وجود دارد."));
    }
    let result = nginx_manager_request(json!({"action":"nginx-ssl-issue","name":name,"email":input.email,"domains":domains,"redirect":input.redirect}))?;
    if result["ok"] == false {
        return Err(error(StatusCode::BAD_GATEWAY, result["error"].as_str().unwrap_or("گرفتن گواهی SSL ناموفق بود.")));
    }
    let data = result.get("data").cloned().unwrap_or(result);
    let current = nginx_manager_request(json!({"action":"nginx-get","name":name}))?;
    let content = nginx_site_content(&current)?;
    let db = state.lock().unwrap();
    let settings: Option<Option<String>> = db.query_row("SELECT settings_json FROM nginx_site_config_state WHERE name=?1", [&name], |row| row.get(0)).optional().map_err(db_error)?;
    let parsed = settings.flatten().and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
    store_nginx_snapshot(&db, &name, content, parsed.as_ref()).map_err(db_error)?;
    audit(&db, &actor, "nginx.site.ssl", &format!("site={name} domains={}", domains.join(",")));
    Ok(Json(data))
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
            (frontend_headers(mime.as_ref()), asset.data).into_response()
        }
        None if !path.starts_with("api/") && !path.contains('.') => static_file_index(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
fn frontend_headers(content_type: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(content_type) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
    );
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(header::EXPIRES, HeaderValue::from_static("0"));
    headers
}
fn static_file_index() -> Response {
    match Frontend::get("index.html") {
        Some(asset) => (frontend_headers("text/html; charset=utf-8"), asset.data).into_response(),
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
    static ARG_UID: OnceLock<Option<libc::uid_t>> = OnceLock::new();
    ARG_UID
        .get_or_init(|| {
            let user = std::ffi::CString::new("arg").unwrap();
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

fn valid_domain_name(host: &str) -> bool {
    let host = host.strip_prefix("*.").unwrap_or(host);
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
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
        let content=fs::read_to_string(&entry.path()).map_err(|error| error.to_string())?;
        let content_hash=hex::encode(Sha256::digest(content.as_bytes()));
        sites.push(json!({"name":name,"enabled":enabled,"bytes":metadata.len(),"content_hash":content_hash}));
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
    let temp = parent.join(format!(".arg-{file_name}-{}.tmp", std::process::id()));
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

fn nginx_issue_ssl(name: &str, email: &str, domains: &[String], redirect: bool) -> Result<Value, String> {
    if !valid_nginx_site_name(name) || domains.is_empty() || domains.len() > 20
        || domains.iter().any(|domain| !valid_domain_name(domain) || domain.starts_with("*.")) {
        return Err("دامنه معتبر نیست؛ HTTP-01 برای wildcard مناسب نیست.".into());
    }
    let (site_path, _) = nginx_site_paths(name)?;
    if !nginx_site_enabled(name)? { return Err("ابتدا سایت را روشن کن؛ صدور گواهی برای سایت غیرفعال انجام نمی‌شود.".into()); }
    if !nginx_service_status()["active"].as_bool().unwrap_or(false) { return Err("سرویس Nginx باید روشن باشد تا Certbot بتواند challenge را پاسخ دهد.".into()); }
    let original_metadata = fs::metadata(&site_path).map_err(|error| error.to_string())?;
    let original_content = fs::read(&site_path).map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(&original_content);
    if name == "arg" { return Err("این سایت برای پنل ARG رزرو است؛ ابزار SSL سایت‌ها نباید گواهی پنل را تغییر دهد.".into()); }
    let mut configured_domains = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("server_name ") {
            if let Some(value) = trimmed.strip_prefix("server_name ").and_then(|value| value.split(';').next()) {
                configured_domains.extend(value.split_whitespace().map(|value| value.to_ascii_lowercase()));
            }
        }
    }
    for domain in domains {
        if !configured_domains.iter().any(|configured| configured == &domain.to_ascii_lowercase()) {
            return Err(format!("دامنهٔ {domain} در server_name همین سایت نیست. ابتدا دامنه‌های سایت را ذخیره و دوباره بررسی کن."));
        }
    }
    let panel_domain = fs::read_to_string("/etc/arg/https-domain").unwrap_or_default().trim().to_ascii_lowercase();
    if !panel_domain.is_empty() && domains.iter().any(|domain| domain.eq_ignore_ascii_case(&panel_domain)) {
        return Err("دامنهٔ پنل ARG را با ابزار SSL سایت‌ها مدیریت نکن؛ گواهی پنل بخش جداگانه‌ای دارد.".into());
    }
    let version = std::process::Command::new("certbot").arg("--version").output()
        .map_err(|_| "Certbot نصب نیست. ابتدا بستهٔ python3-certbot-nginx را نصب کن.".to_owned())?;
    if !version.status.success() { return Err("Certbot نصب نیست یا اجرا نمی‌شود. بستهٔ python3-certbot-nginx را نصب کن.".into()); }
    let backup_dir = std::path::Path::new("/var/backups/arg/nginx-ssl");
    fs::create_dir_all(backup_dir).map_err(|error| format!("ساخت پوشهٔ پشتیبان ناموفق بود: {error}"))?;
    fs::set_permissions(backup_dir, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())?;
    let backup = backup_dir.join(format!("{name}-{}.conf", now()));
    fs::write(&backup, &original_content).map_err(|error| format!("تهیهٔ پشتیبان ناموفق بود: {error}"))?;
    fs::set_permissions(&backup, fs::Permissions::from_mode(0o600)).map_err(|error| error.to_string())?;

    let mut command = std::process::Command::new("timeout");
    command.args(["600s", "certbot", "--nginx", "--non-interactive", "--agree-tos", "--email", email]);
    for domain in domains { command.arg("-d").arg(domain); }
    command.arg(if redirect { "--redirect" } else { "--no-redirect" });
    let output = command.output().map_err(|error| format!("اجرای Certbot ناموفق بود: {error}"))?;
    let mut detail = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        let _ = nginx_atomic_write(&site_path, &original_content, original_metadata.permissions().mode());
        let _ = nginx_reload_if_active();
        detail.truncate(6000);
        return Err(format!("صدور یا نصب گواهی کامل نشد؛ فایل سایت از پشتیبان بازگردانده شد. پورت 80، DNS و دسترسی Certbot را بررسی کن.\n{detail}"));
    }
    if let Err(error) = nginx_config_test() {
        let _ = nginx_atomic_write(&site_path, &original_content, original_metadata.permissions().mode());
        let _ = nginx_reload_if_active();
        return Err(format!("پس از Certbot آزمون nginx -t شکست خورد؛ کانفیگ سایت بازگردانده شد: {error}"));
    }
    if let Err(error) = nginx_reload_if_active() {
        let _ = nginx_atomic_write(&site_path, &original_content, original_metadata.permissions().mode());
        let _ = nginx_reload_if_active();
        return Err(format!("راه‌اندازی مجدد Nginx ناموفق شد؛ کانفیگ سایت بازگردانده شد: {error}"));
    }
    let certificate = format!("/etc/letsencrypt/live/{}/fullchain.pem", domains[0]);
    detail.truncate(3000);
    Ok(json!({"message":"گواهی صادر و در تنظیمات سایت فعال شد. Certbot تمدید خودکار را مدیریت می‌کند.","domains":domains,"certificate":certificate,"backup":backup,"output":detail.trim()}))
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

fn nginx_command(program: &str, args: &[&str]) -> Result<std::process::Output, String> {
    std::process::Command::new(program).args(args).output()
        .map_err(|error| format!("اجرای {program} ناموفق بود: {error}"))
}

fn nginx_service_status() -> Value {
    let installed = std::path::Path::new("/usr/sbin/nginx").exists()
        || std::path::Path::new("/usr/bin/nginx").exists();
    let active = nginx_command("systemctl", &["is-active", "nginx"]).ok()
        .is_some_and(|out| out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "active");
    let enabled = nginx_command("systemctl", &["is-enabled", "nginx"]).ok()
        .is_some_and(|out| out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "enabled");
    let version = if installed { nginx_command("nginx", &["-v"]).ok().map(|out| {
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        text.trim().to_owned()
    }).unwrap_or_default() } else { String::new() };
    let config_test = if installed { nginx_command("nginx", &["-t"]).ok().map(|out| {
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        json!({"ok":out.status.success(),"output":text.trim()})
    }).unwrap_or_else(|| json!({"ok":false,"output":"اجرای nginx -t ناموفق بود."})) } else { json!({"ok":false,"output":"Nginx نصب نیست."}) };
    json!({"installed":installed,"active":active,"enabled":enabled,"version":version,"config_test":config_test})
}

fn nginx_manage(action: &str) -> Result<Value, String> {
    match action {
        "nginx-status" => Ok(nginx_service_status()),
        "nginx-install" => {
            if std::path::Path::new("/usr/sbin/nginx").exists() { return Ok(json!({"message":"Nginx از قبل نصب است."})); }
            let (program, args): (&str, &[&str]) = if std::path::Path::new("/usr/bin/apt-get").exists() {
                ("apt-get", &["install", "-y", "nginx"])
            } else if std::path::Path::new("/usr/bin/dnf").exists() {
                ("dnf", &["install", "-y", "nginx"])
            } else if std::path::Path::new("/usr/bin/yum").exists() {
                ("yum", &["install", "-y", "nginx"])
            } else { return Err("مدیر بستهٔ پشتیبانی‌شده پیدا نشد (apt، dnf یا yum).".into()); };
            let output = nginx_command(program, args)?;
            if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()); }
            let enable = nginx_command("systemctl", &["enable", "--now", "nginx"])?;
            if !enable.status.success() { return Err(format!("نصب انجام شد، اما راه‌اندازی سرویس ناموفق بود: {}", String::from_utf8_lossy(&enable.stderr).trim())); }
            Ok(json!({"message":"Nginx نصب و سرویس آن راه‌اندازی شد."}))
        }
        "nginx-start" | "nginx-stop" | "nginx-restart" | "nginx-reload" | "nginx-enable" | "nginx-disable" => {
            let verb = action.trim_start_matches("nginx-");
            if !nginx_service_status()["installed"].as_bool().unwrap_or(false) { return Err("Nginx نصب نیست.".into()); }
            if matches!(verb, "reload" | "start" | "restart") { nginx_config_test()?; }
            let args: &[&str] = match verb {
                "enable" => &["enable", "nginx"], "disable" => &["disable", "nginx"],
                _ => &[verb, "nginx"],
            };
            let output = nginx_command("systemctl", args)?;
            if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()); }
            Ok(json!({"message":format!("دستور {verb} برای Nginx با موفقیت اجرا شد.")}))
        }
        "nginx-test" => {
            let output = nginx_command("nginx", &["-t"])?;
            let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            if output.status.success() { Ok(json!({"ok":true,"output":text.trim(),"message":"تنظیمات Nginx معتبر است."})) }
            else { Err(text.trim().to_owned()) }
        }
        "nginx-logs" => {
            let output = nginx_command("journalctl", &["-u", "nginx", "-n", "100", "--no-pager", "-o", "short-iso"])?;
            if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()); }
            Ok(json!({"logs":String::from_utf8_lossy(&output.stdout)}))
        }
        _ => Err("عملیات سرویس Nginx معتبر نیست.".into()),
    }
}

fn nginx_create_site(name: &str, content: &str) -> Result<Value, String> {
    if !valid_nginx_site_name(name) || content.is_empty() || content.len() > 512 * 1024 || content.contains('\0') {
        return Err("نام یا محتوای فایل سایت معتبر نیست.".into());
    }
    let (available, _) = nginx_site_paths(name)?;
    if available.exists() { return Err("فایلی با این نام از قبل وجود دارد.".into()); }
    nginx_atomic_write(&available, content.as_bytes(), 0o644)?;
    if let Err(error) = nginx_config_test() {
        let _ = fs::remove_file(&available);
        return Err(format!("پیکربندی معتبر نیست؛ فایل ساخته نشد: {error}"));
    }
    Ok(json!({"site":{"name":name,"enabled":false,"bytes":content.len()},"message":"فایل سایت ساخته و اعتبارسنجی شد؛ برای اعمال، آن را فعال کن."}))
}

fn nginx_delete_site(name: &str) -> Result<Value, String> {
    if name == "default" { return Err("حذف سایت پیش‌فرض سیستم از پنل مجاز نیست.".into()); }
    let (available, enabled) = nginx_site_paths(name)?;
    let metadata = fs::symlink_metadata(&available).map_err(|_| "فایل سایت پیدا نشد.")?;
    if !metadata.file_type().is_file() { return Err("فقط فایل معمولی قابل حذف است.".into()); }
    let content = fs::read(&available).map_err(|error| error.to_string())?;
    let mode = metadata.permissions().mode();
    let was_enabled = nginx_site_enabled(name)?;
    if was_enabled { fs::remove_file(&enabled).map_err(|error| error.to_string())?; }
    if let Err(error) = fs::remove_file(&available) {
        if was_enabled { let _ = std::os::unix::fs::symlink(format!("../sites-available/{name}"), &enabled); }
        return Err(error.to_string());
    }
    if let Err(error) = nginx_config_test().and_then(|_| nginx_reload_if_active()) {
        let _ = nginx_atomic_write(&available, &content, mode);
        if was_enabled { let _ = std::os::unix::fs::symlink(format!("../sites-available/{name}"), &enabled); }
        let _ = nginx_reload_if_active();
        return Err(format!("حذف اعمال نشد و سایت بازگردانده شد: {error}"));
    }
    Ok(json!({"message":"فایل سایت حذف شد."}))
}

fn nginx_rename_site(name: &str, new_name: &str) -> Result<Value, String> {
    if name == "default" || new_name == "default" {
        return Err("تغییر نام سایت پیش‌فرض سیستم مجاز نیست.".into());
    }
    if !valid_nginx_site_name(name) || !valid_nginx_site_name(new_name) {
        return Err("نام فایل سایت معتبر نیست.".into());
    }
    let (old_available, old_enabled) = nginx_site_paths(name)?;
    let (new_available, new_enabled) = nginx_site_paths(new_name)?;
    let metadata = fs::symlink_metadata(&old_available).map_err(|_| "فایل سایت پیدا نشد.")?;
    if !metadata.file_type().is_file() { return Err("فقط فایل معمولی قابل تغییر نام است.".into()); }
    if name == new_name { return Ok(json!({"name":name,"message":"نام سایت تغییری نکرد."})); }
    if new_available.exists() || fs::symlink_metadata(&new_enabled).is_ok() {
        return Err("یک فایل یا symlink با نام مقصد از قبل وجود دارد.".into());
    }
    let was_enabled = nginx_site_enabled(name)?;
    fs::rename(&old_available, &new_available).map_err(|error| format!("تغییر نام فایل ناموفق بود: {error}"))?;
    if was_enabled {
        if let Err(error) = fs::remove_file(&old_enabled).and_then(|_| std::os::unix::fs::symlink(format!("../sites-available/{new_name}"), &new_enabled)) {
            let _ = fs::rename(&new_available, &old_available);
            if fs::symlink_metadata(&old_enabled).is_err() {
                let _ = std::os::unix::fs::symlink(format!("../sites-available/{name}"), &old_enabled);
            }
            return Err(format!("تغییر نام symlink ناموفق بود: {error}"));
        }
    }
    if let Err(error) = nginx_config_test().and_then(|_| nginx_reload_if_active()) {
        if was_enabled {
            let _ = fs::remove_file(&new_enabled);
            let _ = std::os::unix::fs::symlink(format!("../sites-available/{name}"), &old_enabled);
        }
        let _ = fs::rename(&new_available, &old_available);
        let _ = nginx_reload_if_active();
        return Err(format!("تغییر نام اعمال نشد و نام قبلی بازگردانده شد: {error}"));
    }
    Ok(json!({"name":new_name,"enabled":was_enabled,"message":"نام فایل سایت تغییر کرد."}))
}

fn nginx_site_logs(name: &str, kind: &str) -> Result<Value, String> {
    if !valid_nginx_site_name(name) || !["access", "error"].contains(&kind) {
        return Err("نام سایت یا نوع لاگ معتبر نیست.".into());
    }
    let (available, _) = nginx_site_paths(name)?;
    let config = fs::read_to_string(&available).map_err(|_| "فایل سایت پیدا نشد.")?;
    let directive = if kind == "access" { "access_log" } else { "error_log" };
    let configured = config.lines().filter_map(|line| {
        let mut parts = line.trim().trim_end_matches(';').split_whitespace();
        (parts.next()? == directive).then(|| parts.next().unwrap_or("").trim_matches('"').to_owned())
    }).find(|value| !value.is_empty());
    let path = configured.unwrap_or_else(|| format!("/var/log/nginx/{}.log", if kind == "access" { "access" } else { "error" }));
    if path == "off" { return Ok(json!({"name":name,"kind":kind,"path":"off","logs":"این سایت ثبت لاگ را غیرفعال کرده است."})); }
    if !path.starts_with('/') || path.contains('$') { return Err("مسیر لاگ متغیر یا نسبی است و از پنل قابل خواندن نیست.".into()); }
    let allowed_root = fs::canonicalize("/var/log/nginx").map_err(|_| "پوشهٔ لاگ استاندارد Nginx پیدا نشد.")?;
    let requested = std::path::PathBuf::from(&path);
    let canonical = fs::canonicalize(&requested).map_err(|_| format!("فایل لاگ پیدا نشد: {path}"))?;
    if !canonical.starts_with(&allowed_root) || !canonical.is_file() {
        return Err("برای محافظت از فایل‌های سیستم، فقط فایل‌های داخل /var/log/nginx قابل مشاهده‌اند.".into());
    }
    let output = std::process::Command::new("tail").args(["-n", "100", "--", canonical.to_string_lossy().as_ref()]).output().map_err(|error| error.to_string())?;
    if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()); }
    let start = output.stdout.len().saturating_sub(512 * 1024);
    let logs = String::from_utf8_lossy(&output.stdout[start..]).into_owned();
    Ok(json!({"name":name,"kind":kind,"path":path,"logs":logs}))
}

fn nginx_browse_files(path: Option<&str>) -> Result<Value, String> {
    const ROOTS: &[&str] = &["/etc/letsencrypt", "/etc/ssl", "/etc/nginx", "/var/www"];
    let roots: Vec<String> = ROOTS.iter().filter(|root| std::path::Path::new(root).is_dir()).map(|root| (*root).to_owned()).collect();
    let Some(path) = path else {
        return Ok(json!({"path":"", "roots":roots, "entries":[]}));
    };
    if path.len() > 1024 { return Err("مسیر بیش از حد طولانی است.".into()); }
    let requested = std::path::PathBuf::from(path);
    let directory = fs::canonicalize(&requested).map_err(|_| "مسیر پیدا نشد یا قابل دسترسی نیست.")?;
    let allowed = roots.iter().filter_map(|root| fs::canonicalize(root).ok()).any(|root| directory.starts_with(root));
    if !allowed || !directory.is_dir() { return Err("دسترسی فقط به پوشه‌های مجاز SSL، Nginx و وب محدود است.".into()); }
    let mut entries = Vec::new();
    for entry in fs::read_dir(&directory).map_err(|error| error.to_string())?.take(500) {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.len() > 255 { continue; }
        let displayed = requested.join(&name);
        let Ok(target) = fs::canonicalize(entry.path()) else { continue; };
        if !roots.iter().filter_map(|root| fs::canonicalize(root).ok()).any(|root| target.starts_with(root)) { continue; }
        let is_dir = target.is_dir();
        let ext = entry.path().extension().and_then(|part| part.to_str()).unwrap_or("").to_ascii_lowercase();
        let selectable = !is_dir && matches!(ext.as_str(), "pem" | "crt" | "cer" | "key" | "p12" | "pfx");
        if is_dir || selectable {
            entries.push(json!({"name":name,"path":displayed.to_string_lossy(),"directory":is_dir,"selectable":selectable}));
        }
    }
    entries.sort_by(|a,b| b["directory"].as_bool().cmp(&a["directory"].as_bool()).then_with(|| a["name"].as_str().cmp(&b["name"].as_str())));
    Ok(json!({"path":requested.to_string_lossy(),"roots":roots,"entries":entries}))
}

fn execute_nginx_request(action: &str, request: &Value) -> Value {
    let result = match action {
        "nginx-status" | "nginx-install" | "nginx-start" | "nginx-stop" | "nginx-restart" | "nginx-reload" | "nginx-enable" | "nginx-disable" | "nginx-test" | "nginx-logs" => nginx_manage(action),
        "nginx-ssl-issue" => request["name"].as_str().ok_or_else(|| "نام سایت ارسال نشده است.".to_owned()).and_then(|name| {
            let email = request["email"].as_str().ok_or_else(|| "ایمیل ارسال نشده است.".to_owned())?;
            let domains = request["domains"].as_array().ok_or_else(|| "فهرست دامنه‌ها ارسال نشده است.".to_owned())?.iter().map(|value| value.as_str().map(str::to_owned).ok_or_else(|| "دامنه نامعتبر است.".to_owned())).collect::<Result<Vec<_>,_>>()?;
            nginx_issue_ssl(name, email, &domains, request["redirect"].as_bool().unwrap_or(false))
        }),
        "nginx-create" => request["name"].as_str().ok_or_else(|| "نام سایت ارسال نشده است.".to_owned()).and_then(|name| request["content"].as_str().ok_or_else(|| "محتوای سایت ارسال نشده است.".to_owned()).and_then(|content| nginx_create_site(name, content))),
        "nginx-delete" => request["name"].as_str().ok_or_else(|| "نام سایت ارسال نشده است.".to_owned()).and_then(nginx_delete_site).map(|_| json!({"message":"فایل سایت حذف شد."})),
        "nginx-rename" => request["name"].as_str().ok_or_else(|| "نام فعلی ارسال نشده است.".to_owned()).and_then(|name| request["new_name"].as_str().ok_or_else(|| "نام جدید ارسال نشده است.".to_owned()).and_then(|new_name| nginx_rename_site(name, new_name))),
        "nginx-site-logs" => request["name"].as_str().ok_or_else(|| "نام سایت ارسال نشده است.".to_owned()).and_then(|name| request["kind"].as_str().ok_or_else(|| "نوع لاگ ارسال نشده است.".to_owned()).and_then(|kind| nginx_site_logs(name, kind))),
        "nginx-browse" => nginx_browse_files(request["path"].as_str()),
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

fn valid_terminal_pty_path(path: &str) -> bool {
    path.strip_prefix("/dev/pts/")
        .is_some_and(|number| !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))
}

fn start_root_terminal(request: &Value) -> Result<u32, String> {
    let tty_path = request["tty"].as_str().ok_or("Terminal device was not provided")?;
    if !valid_terminal_pty_path(tty_path) {
        return Err("Invalid terminal device".into());
    }
    let metadata = fs::metadata(tty_path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_char_device() {
        return Err("Terminal device is not a character device".into());
    }
    let arg_name = std::ffi::CString::new("arg").unwrap();
    let arg_uid = unsafe {
        let entry = libc::getpwnam(arg_name.as_ptr());
        if entry.is_null() { return Err("ARG service account was not found".into()); }
        (*entry).pw_uid
    };
    if metadata.uid() != arg_uid {
        return Err("Terminal device is not owned by the ARG service account".into());
    }

    let tty = fs::OpenOptions::new().read(true).write(true).open(tty_path).map_err(|error| error.to_string())?;
    let stdin = tty.try_clone().map_err(|error| error.to_string())?;
    let stdout = tty.try_clone().map_err(|error| error.to_string())?;
    let mut command = std::process::Command::new("/bin/bash");
    command
        .args(["--login", "-i"])
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("HOME", "/root")
        .env("USER", "root")
        .env("LOGNAME", "root")
        .env("SHELL", "/bin/bash")
        .env("PATH", "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin")
        .current_dir("/")
        .stdin(std::process::Stdio::from(tty))
        .stdout(std::process::Stdio::from(stdin))
        .stderr(std::process::Stdio::from(stdout));
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 { return Err(io::Error::last_os_error()); }
            if libc::ioctl(libc::STDIN_FILENO, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn().map_err(|error| format!("Could not start root shell: {error}"))?;
    let pid = child.id();
    let pids = ROOT_TERMINAL_PIDS.get_or_init(|| Mutex::new(HashSet::new()));
    pids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(pid);
    std::thread::spawn(move || {
        let _ = child.wait();
        if let Some(pids) = ROOT_TERMINAL_PIDS.get() {
            pids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&pid);
        }
    });
    Ok(pid)
}

fn stop_root_terminal(request: &Value) -> Result<(), String> {
    let pid = request["pid"].as_u64().filter(|pid| *pid > 1 && *pid <= i32::MAX as u64).ok_or("Invalid root terminal process id")? as u32;
    let pids = ROOT_TERMINAL_PIDS.get_or_init(|| Mutex::new(HashSet::new()));
    let removed = pids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&pid);
    if !removed { return Ok(()); }
    let result = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    if result != 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) { return Err(error.to_string()); }
    }
    Ok(())
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
        Some("update-logs") => "update-logs",
        Some("update-progress") => "update-progress",
        Some("nginx-list") => "nginx-list",
        Some("nginx-get") => "nginx-get",
        Some("nginx-save") => "nginx-save",
        Some("nginx-toggle") => "nginx-toggle",
        Some("nginx-status") => "nginx-status",
        Some("nginx-install") => "nginx-install",
        Some("nginx-start") => "nginx-start",
        Some("nginx-stop") => "nginx-stop",
        Some("nginx-restart") => "nginx-restart",
        Some("nginx-reload") => "nginx-reload",
        Some("nginx-enable") => "nginx-enable",
        Some("nginx-disable") => "nginx-disable",
        Some("nginx-test") => "nginx-test",
        Some("nginx-logs") => "nginx-logs",
        Some("nginx-ssl-issue") => "nginx-ssl-issue",
        Some("nginx-create") => "nginx-create",
        Some("nginx-delete") => "nginx-delete",
        Some("nginx-rename") => "nginx-rename",
        Some("nginx-site-logs") => "nginx-site-logs",
        Some("nginx-browse") => "nginx-browse",
        Some("apt-list") => "apt-list",
        Some("apt-run") => "apt-run",
        Some("service-list") => "service-list",
        Some("service-action") => "service-action",
        Some("docker-status") => "docker-status",
        Some("docker-operation") => "docker-operation",
        Some("terminal-root-start") => "terminal-root-start",
        Some("terminal-root-stop") => "terminal-root-stop",
        _ => return json!({"ok":false,"error":"Unsupported privileged action"}),
    };
    if action == "terminal-root-start" {
        return match start_root_terminal(request) {
            Ok(pid) => json!({"ok":true,"pid":pid}),
            Err(error) => json!({"ok":false,"error":error}),
        };
    }
    if action == "terminal-root-stop" {
        return match stop_root_terminal(request) {
            Ok(()) => json!({"ok":true}),
            Err(error) => json!({"ok":false,"error":error}),
        };
    }
    if action == "docker-status" { return execute_docker_status(); }
    if action == "docker-operation" { return execute_docker_operation(request); }
    if action.starts_with("apt-") { return execute_apt_request(action, request); }
    if action.starts_with("service-") { return execute_service_request(action, request); }
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
            "/usr/local/lib/arg/update-helper",
            vec![action.trim_start_matches("update-")],
        )
    } else {
        (
            "/usr/local/lib/arg/ufw-helper",
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
        return json!({"ok":true,"available":installed,"installed":installed,"status":stdout,"error":if installed { "" } else if stderr.is_empty() { "UFW is not installed" } else { &stderr }});
    }
    if output.status.success() {
        json!({"ok":true,"stdout":stdout})
    } else {
        json!({"ok":false,"error":if stderr.is_empty() { "UFW operation failed" } else { &stderr }})
    }
}

fn command_result(command: &mut std::process::Command) -> Result<(bool, String), String> {
    let output = command.output().map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let result = if stderr.trim().is_empty() { stdout } else if stdout.trim().is_empty() { stderr } else { format!("{stdout}\n{stderr}") };
    Ok((output.status.success(), result))
}
fn execute_apt_request(action: &str, request: &Value) -> Value {
    if action == "apt-list" {
        let mut command = std::process::Command::new("dpkg-query");
        command.args(["-W", "-f=${binary:Package}\t${Version}\t${db:Status-Abbrev}\t${binary:Summary}\n"]);
        return match command_result(&mut command) {
            Ok((true, output)) => {
                let packages: Vec<Value> = output.lines().filter_map(|line| {
                    let mut fields = line.splitn(4, '\t');
                    let name = fields.next()?; let version = fields.next()?; let status = fields.next()?; let summary = fields.next().unwrap_or("");
                    status.starts_with("ii").then(|| json!({"name":name,"version":version,"summary":summary}))
                }).collect();
                json!({"ok":true,"data":{"packages":packages}})
            }
            Ok((false, output)) => json!({"ok":false,"error":output}),
            Err(error) => json!({"ok":false,"error":error}),
        };
    }
    let Some(operation) = request["operation"].as_str() else { return json!({"ok":false,"error":"Missing APT operation"}); };
    let allowed = ["update", "upgrade", "full-upgrade", "autoremove", "clean", "autoclean", "install", "remove"];
    if !allowed.contains(&operation) { return json!({"ok":false,"error":"Unsupported APT operation"}); }
    let packages: Vec<&str> = request["packages"].as_array().map(|items| items.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    if packages.len() > 25 || packages.iter().any(|name| !valid_package_name(name)) || ["install", "remove"].contains(&operation) && packages.is_empty() { return json!({"ok":false,"error":"Invalid package list"}); }
    let mut command = std::process::Command::new("apt-get");
    command.env("DEBIAN_FRONTEND", "noninteractive").env("APT_LISTCHANGES_FRONTEND", "none").arg("-y");
    match operation { "full-upgrade" => { command.arg("full-upgrade"); }, "autoremove" => { command.arg("autoremove"); }, _ => { command.arg(operation); } }
    if operation == "install" || operation == "remove" { command.arg("--").args(packages); }
    match command_result(&mut command) { Ok((true, output)) => json!({"ok":true,"stdout":output}), Ok((false, output)) => json!({"ok":false,"error":output}), Err(error) => json!({"ok":false,"error":error}) }
}
fn execute_service_request(action: &str, request: &Value) -> Value {
    if action == "service-list" {
        let mut files = std::process::Command::new("systemctl"); files.args(["list-unit-files", "--type=service", "--no-legend", "--no-pager"]);
        let mut units = std::process::Command::new("systemctl"); units.args(["list-units", "--all", "--type=service", "--no-legend", "--no-pager"]);
        let (Ok((true, file_output)), Ok((true, active_output))) = (command_result(&mut files), command_result(&mut units)) else { return json!({"ok":false,"error":"Could not list systemd services"}); };
        let active: HashMap<String, (String,String,String)> = active_output.lines().filter_map(|line| {
            let mut f=line.split_whitespace(); let unit=f.next()?; let _load=f.next()?; let state=f.next()?; let sub=f.next()?;
            let desc=line.splitn(5, char::is_whitespace).nth(4).unwrap_or("").trim().to_owned();
            Some((unit.to_owned(),(state.to_owned(),sub.to_owned(),desc)))
        }).collect();
        let services: Vec<Value> = file_output.lines().filter_map(|line| {
            let mut f=line.split_whitespace(); let unit=f.next()?; let enabled=f.next()?;
            if !valid_service_unit(unit) { return None; }
            let (state,sub,description)=active.get(unit).cloned().unwrap_or_else(||("inactive".into(),"dead".into(),String::new()));
            Some(json!({"unit":unit,"enabled":enabled,"state":state,"sub_state":sub,"description":if description.is_empty(){"System service"}else{description.as_str()}}))
        }).collect();
        return json!({"ok":true,"data":{"services":services}});
    }
    let Some(operation)=request["operation"].as_str() else { return json!({"ok":false,"error":"Missing systemd action"}); };
    let allowed=["start","stop","restart","try-restart","reload","reload-or-restart","enable","disable","enable-now","disable-now","reenable","mask","unmask","reset-failed","status","logs","daemon-reload"];
    if !allowed.contains(&operation) { return json!({"ok":false,"error":"Unsupported systemd action"}); }
    if operation == "daemon-reload" {
        let mut command=std::process::Command::new("systemctl"); command.arg("daemon-reload");
        return match command_result(&mut command) { Ok((true,output))=>json!({"ok":true,"stdout":output}),Ok((false,output))=>json!({"ok":false,"error":output}),Err(error)=>json!({"ok":false,"error":error}) };
    }
    let Some(unit)=request["unit"].as_str().filter(|u|valid_service_unit(u)) else { return json!({"ok":false,"error":"Invalid service unit"}); };
    if operation == "logs" {
        let mut command=std::process::Command::new("journalctl"); command.args(["--no-pager","--output=short-iso","--lines=100","--unit",unit]);
        return match command_result(&mut command) { Ok((true,output))=>json!({"ok":true,"stdout":output}),Ok((false,output))=>json!({"ok":false,"error":output}),Err(error)=>json!({"ok":false,"error":error}) };
    }
    let mut command=std::process::Command::new("systemctl");
    if operation == "status" { command.args(["--no-pager","--full","status",unit]); return match command_result(&mut command) { Ok((_,output))=>json!({"ok":true,"stdout":output}),Err(error)=>json!({"ok":false,"error":error}) }; }
    match operation { "enable-now"=>{command.arg("enable").arg("--now");},"disable-now"=>{command.arg("disable").arg("--now");},_=>{command.arg(operation);} }
    command.arg(unit);
    match command_result(&mut command) { Ok((true,output))=>json!({"ok":true,"stdout":output}),Ok((false,output))=>json!({"ok":false,"error":output}),Err(error)=>json!({"ok":false,"error":error}) }
}

fn docker_table(args: &[&str], fields: &[&str]) -> Result<Vec<Value>, String> {
    let mut command = std::process::Command::new("docker");
    command.args(args);
    let (success, output) = command_result(&mut command)?;
    if !success { return Err(output); }
    Ok(output.lines().filter_map(|line| {
        let values: Vec<_> = line.split('\t').collect();
        if values.len() != fields.len() { return None; }
        Some(Value::Object(fields.iter().zip(values).map(|(key, value)| ((*key).to_owned(), json!(value))).collect()))
    }).collect())
}

fn execute_docker_status() -> Value {
    let mut client = std::process::Command::new("docker");
    client.arg("--version");
    let client_result = command_result(&mut client);
    let Ok((true, client_version)) = client_result else {
        return json!({"ok":true,"data":{"installed":false,"active":false,"version":"","containers":[],"images":[],"networks":[],"volumes":[],"error":"Docker is not installed."}});
    };
    let mut server = std::process::Command::new("docker");
    server.args(["version", "--format", "{{.Server.Version}}"]);
    let server_result = command_result(&mut server);
    let active = matches!(&server_result, Ok((true, _)));
    if !active {
        let detail = server_result.map(|(_, output)| output).unwrap_or_else(|error| error);
        return json!({"ok":true,"data":{"installed":true,"active":false,"version":client_version,"server_version":"","containers":[],"images":[],"networks":[],"volumes":[],"error":detail}});
    }
    let server_version = server_result.unwrap().1;
    let mut containers = docker_table(&["ps", "-a", "--no-trunc", "--format", "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}\t{{.State}}"], &["id", "names", "image", "status", "ports", "state"]);
    if let Ok(stats) = docker_table(&["stats", "--no-stream", "--format", "{{.Container}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}\t{{.NetIO}}\t{{.BlockIO}}"], &["container", "cpu", "memory", "memory_percent", "network_io", "block_io"]) {
        if let Some(rows) = containers.as_mut().ok() {
            for container in rows {
                if let Some(id) = container["id"].as_str() {
                    if let Some(stat) = stats.iter().find(|stat| stat["container"].as_str().is_some_and(|key| id.starts_with(key))) {
                        container["stats"] = stat.clone();
                    }
                }
            }
        }
    }
    let images = docker_table(&["image", "ls", "-a", "--no-trunc", "--format", "{{.ID}}\t{{.Repository}}\t{{.Tag}}\t{{.Size}}\t{{.CreatedSince}}"], &["id", "repository", "tag", "size", "created"]);
    let networks = docker_table(&["network", "ls", "--no-trunc", "--format", "{{.ID}}\t{{.Name}}\t{{.Driver}}\t{{.Scope}}"], &["id", "name", "driver", "scope"]);
    let volumes = docker_table(&["volume", "ls", "--format", "{{.Name}}\t{{.Driver}}\t{{.Mountpoint}}"], &["name", "driver", "mountpoint"]);
    let mut compose_version_command = std::process::Command::new("docker"); compose_version_command.args(["compose", "version", "--short"]);
    let compose_version = command_result(&mut compose_version_command).ok().filter(|(success, _)| *success).map(|(_, output)| output).unwrap_or_default();
    let mut compose_list_command = std::process::Command::new("docker"); compose_list_command.args(["compose", "ls", "--all", "--format", "json"]);
    let compose_projects: Vec<Value> = command_result(&mut compose_list_command).ok().and_then(|(success, output)| if success { serde_json::from_str(&output).ok() } else { None }).unwrap_or_default();
    let failed = [&containers, &images, &networks, &volumes].iter().find_map(|result| result.as_ref().err().cloned());
    json!({"ok":true,"data":{"installed":true,"active":true,"version":client_version,"server_version":server_version,"compose_version":compose_version,"compose_projects":compose_projects,"containers":containers.unwrap_or_default(),"images":images.unwrap_or_default(),"networks":networks.unwrap_or_default(),"volumes":volumes.unwrap_or_default(),"error":failed.unwrap_or_default()}})
}

fn valid_docker_object(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
        && !value.contains("..")
}

fn valid_docker_image(value: &str) -> bool {
    !value.is_empty() && value.len() <= 255 && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._/:@-".contains(&byte))
}

fn valid_docker_port(value: &str) -> bool {
    let (mapping, protocol) = value.split_once('/').unwrap_or((value, "tcp"));
    if !["tcp", "udp"].contains(&protocol) { return false; }
    let Some((host, container)) = mapping.split_once(':') else { return false; };
    host.parse::<u16>().is_ok_and(|port| port > 0) && container.parse::<u16>().is_ok_and(|port| port > 0)
}

fn execute_docker_operation(request: &Value) -> Value {
    let Some(operation) = request["operation"].as_str() else { return json!({"ok":false,"error":"Missing Docker operation"}); };
    let confirmed = request["confirmed"].as_bool().unwrap_or(false);
    let target = request["target"].as_str().unwrap_or_default();
    let name = request["name"].as_str().unwrap_or_default();
    let image = request["image"].as_str().unwrap_or_default();
    let mut args: Vec<String> = Vec::new();
    match operation {
        "install" => {
            let mut update = std::process::Command::new("apt-get"); update.args(["update"]);
            if let Err(error) = command_result(&mut update).and_then(|(ok, output)| if ok { Ok((ok, output)) } else { Err(output) }) { return json!({"ok":false,"error":error}); }
            let mut install = std::process::Command::new("apt-get"); install.args(["install", "-y", "docker.io"]);
            return match command_result(&mut install) { Ok((true, output)) => { let mut enable = std::process::Command::new("systemctl"); enable.args(["enable", "--now", "docker"]); match command_result(&mut enable) { Ok((true, service)) => json!({"ok":true,"stdout":format!("{output}\n{service}")}), Ok((false, service)) => json!({"ok":false,"error":service}), Err(error) => json!({"ok":false,"error":error}) } }, Ok((false, output)) => json!({"ok":false,"error":output}), Err(error) => json!({"ok":false,"error":error}) };
        }
        "uninstall" => {
            if !confirmed { return json!({"ok":false,"error":"Confirmation required"}); }
            let mut stop = std::process::Command::new("systemctl"); stop.args(["disable", "--now", "docker"]);
            let stop_result = command_result(&mut stop).map(|(_, output)| output).unwrap_or_default();
            let mut purge = std::process::Command::new("apt-get"); purge.args(["purge", "-y", "docker.io"]);
            return match command_result(&mut purge) { Ok((true, output)) => json!({"ok":true,"stdout":format!("{stop_result}\n{output}\nDocker data directory was not explicitly removed.")}), Ok((false, output)) => json!({"ok":false,"error":output}), Err(error) => json!({"ok":false,"error":error}) };
        }
        "start" | "stop" | "restart" | "enable" | "disable" => {
            if ["stop", "restart", "disable"].contains(&operation) && !confirmed { return json!({"ok":false,"error":"Confirmation required"}); }
            if operation == "enable" || operation == "disable" { args.extend(["systemctl".to_owned(), operation.to_owned(), "docker".to_owned()]); }
            else { args.extend(["systemctl".to_owned(), operation.to_owned(), "docker".to_owned()]); }
            let mut command = std::process::Command::new(&args[0]); command.args(&args[1..]);
            return match command_result(&mut command) { Ok((true, output)) => json!({"ok":true,"stdout":output}), Ok((false, output)) => json!({"ok":false,"error":output}), Err(error) => json!({"ok":false,"error":error}) };
        }
        "container-run" => {
            if !valid_docker_object(name) || !valid_docker_image(image) { return json!({"ok":false,"error":"Invalid container name or image"}); }
            let ports: Vec<&str> = request["ports"].as_array().map(|items| items.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            let envs: Vec<&str> = request["env"].as_array().map(|items| items.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            if ports.len() > 32 || ports.iter().any(|port| !valid_docker_port(port)) || envs.len() > 64 || envs.iter().any(|value| value.len() > 4096 || value.bytes().any(|byte| matches!(byte, b'\n' | b'\r')) || !value.split_once('=').is_some_and(|(key, _)| !key.is_empty() && key.bytes().enumerate().all(|(i, byte)| if i == 0 { byte.is_ascii_alphabetic() || byte == b'_' } else { byte.is_ascii_alphanumeric() || byte == b'_' }))) { return json!({"ok":false,"error":"Invalid port or environment settings"}); }
            let restart = request["restart"].as_str().unwrap_or("unless-stopped");
            if !["no", "always", "unless-stopped", "on-failure"].contains(&restart) { return json!({"ok":false,"error":"Invalid restart policy"}); }
            args.extend(["run".into(), "-d".into(), "--name".into(), name.into(), "--restart".into(), restart.into()]);
            for port in ports { args.extend(["-p".into(), port.into()]); }
            for value in envs { args.extend(["--env".into(), value.into()]); }
            let network = request["network"].as_str().unwrap_or_default();
            if !network.is_empty() { if !valid_docker_object(network) { return json!({"ok":false,"error":"Invalid Docker network"}); } args.extend(["--network".into(), network.into()]); }
            args.push(image.into());
        }
        "container-start" | "container-stop" | "container-restart" | "container-pause" | "container-unpause" | "container-remove" | "container-logs" | "container-inspect" => {
            if !valid_docker_object(target) { return json!({"ok":false,"error":"Invalid container identifier"}); }
            if operation == "container-remove" && !confirmed { return json!({"ok":false,"error":"Confirmation required"}); }
            let action = match operation { "container-start" => "start", "container-stop" => "stop", "container-restart" => "restart", "container-pause" => "pause", "container-unpause" => "unpause", "container-remove" => "rm", "container-logs" => "logs", "container-inspect" => "inspect", _ => unreachable!() };
            args.push(action.into());
            if operation == "container-remove" { args.push("-f".into()); }
            if operation == "container-logs" { args.extend(["--tail".into(), "200".into(), "--timestamps".into()]); }
            if operation == "container-inspect" { args.extend(["--type".into(), "container".into()]); }
            args.push(target.into());
        }
        "container-exec" => {
            if !valid_docker_object(target) { return json!({"ok":false,"error":"Invalid container identifier"}); }
            let command: Vec<&str> = request["command"].as_array().map(|items| items.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            if command.is_empty() || command.len() > 32 || command.iter().any(|part| part.is_empty() || part.len() > 1024 || part.bytes().any(|byte| matches!(byte, b'\n' | b'\r'))) { return json!({"ok":false,"error":"Enter a valid command and arguments"}); }
            args.extend(["exec".into(), "-i".into(), target.into()]);
            args.extend(command.iter().map(|part| (*part).to_owned()));
        }
        "image-pull" | "image-push" | "image-remove" => {
            let reference = if target.is_empty() { image } else { target };
            if !valid_docker_image(reference) || ["image-remove", "image-push"].contains(&operation) && !confirmed { return json!({"ok":false,"error":"Invalid image reference or missing confirmation"}); }
            args.extend(["image".into(), match operation { "image-pull" => "pull", "image-push" => "push", _ => "rm" }.into()]);
            if operation == "image-remove" { args.push("--".into()); }
            args.push(reference.into());
        }
        "image-build" => {
            let Some(context) = request["context_path"].as_str().filter(|context| {
                let path = std::path::Path::new(context);
                path.is_absolute() && !path.components().any(|part| matches!(part, std::path::Component::ParentDir))
                    && path.is_dir() && path.join("Dockerfile").is_file()
            }) else { return json!({"ok":false,"error":"Build context must be an absolute directory containing Dockerfile"}); };
            if !valid_docker_image(image) { return json!({"ok":false,"error":"Invalid image tag"}); }
            args.extend(["build".into(), "--tag".into(), image.into(), context.into()]);
        }
        "image-tag" => {
            if !valid_docker_image(target) || !valid_docker_image(image) { return json!({"ok":false,"error":"Invalid source or target image reference"}); }
            args.extend(["image".into(), "tag".into(), target.into(), image.into()]);
        }
        "image-prune" => { if !confirmed { return json!({"ok":false,"error":"Confirmation required"}); } args.extend(["image".into(), "prune".into(), "-af".into()]); }
        "network-create" | "network-remove" => {
            let value = if operation == "network-create" { name } else { target };
            if !valid_docker_object(value) || operation == "network-remove" && !confirmed { return json!({"ok":false,"error":"Invalid network name or missing confirmation"}); }
            args.extend(["network".into(), if operation == "network-create" { "create" } else { "rm" }.into(), value.into()]);
        }
        "volume-create" | "volume-remove" => {
            let value = if operation == "volume-create" { name } else { target };
            if !valid_docker_object(value) || operation == "volume-remove" && !confirmed { return json!({"ok":false,"error":"Invalid volume name or missing confirmation"}); }
            args.extend(["volume".into(), if operation == "volume-create" { "create" } else { "rm" }.into(), value.into()]);
        }
        "volume-prune" => { if !confirmed { return json!({"ok":false,"error":"Confirmation required"}); } args.extend(["volume".into(), "prune".into(), "-f".into()]); }
        "system-prune" => { if !confirmed { return json!({"ok":false,"error":"Confirmation required"}); } args.extend(["system".into(), "prune".into(), "-af".into()]); }
        "compose-up" | "compose-down" | "compose-pull" => {
            if operation == "compose-down" && !confirmed { return json!({"ok":false,"error":"Confirmation required"}); }
            let Some(file) = request["compose_file"].as_str().filter(|file| {
                let path = std::path::Path::new(file);
                path.is_absolute() && !path.components().any(|part| matches!(part, std::path::Component::ParentDir))
                    && path.extension().and_then(std::ffi::OsStr::to_str).is_some_and(|extension| ["yml", "yaml"].contains(&extension))
                    && path.metadata().is_ok_and(|metadata| metadata.is_file() && metadata.len() <= 1024 * 1024)
            }) else { return json!({"ok":false,"error":"Choose an existing absolute Compose YAML file (maximum 1 MB)"}); };
            args.extend(["compose".into(), "-f".into(), file.into(), match operation { "compose-up" => "up", "compose-down" => "down", _ => "pull" }.into()]);
            if operation == "compose-up" { args.push("-d".into()); }
        }
        _ => return json!({"ok":false,"error":"Unsupported Docker operation"}),
    }
    let mut command = std::process::Command::new("docker"); command.args(&args);
    match command_result(&mut command) { Ok((true, output)) => json!({"ok":true,"stdout":output}), Ok((false, output)) => json!({"ok":false,"error":output}), Err(error) => json!({"ok":false,"error":error}) }
}

async fn serve_privileged_connection(mut stream: UnixStream) {
    if !privileged_peer_allowed(&stream) {
        return;
    }
    let mut line = String::new();
    let response = match AsyncBufReader::new(&mut stream).read_line(&mut line).await {
        Ok(size) if size > 0 && size <= 4 * 1024 * 1024 => {
            match serde_json::from_str::<Value>(&line) {
                Ok(request) => tokio::task::spawn_blocking(move || execute_privileged_request(&request))
                    .await.unwrap_or_else(|_| json!({"ok":false,"error":"Privileged task failed"})),
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

fn run_terminal_shell() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        if libc::prctl(
            libc::PR_CAP_AMBIENT,
            libc::PR_CAP_AMBIENT_CLEAR_ALL,
            0,
            0,
            0,
        ) != 0
        {
            return Err(io::Error::last_os_error().into());
        }
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
            return Err(io::Error::last_os_error().into());
        }
    }
    use std::os::unix::process::CommandExt;
    let error = std::process::Command::new("/bin/bash")
        .arg("--login")
        .arg("-i")
        .exec();
    Err(error.into())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::args().nth(1).as_deref() == Some("terminal-shell") {
        return run_terminal_shell();
    }
    if env::args().nth(1).as_deref() == Some("privileged-helper") {
        return run_privileged_helper().await;
    }
    if env::args().nth(1).as_deref() == Some("change-password") {
        return change_password_cli();
    }
    let db_path = env::var("ARG_DB").unwrap_or_else(|_| "data/arg.db".into());
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
    db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY,username TEXT NOT NULL UNIQUE,password_hash TEXT NOT NULL,role TEXT NOT NULL,enabled INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS sessions (token_hash TEXT PRIMARY KEY,user_id INTEGER NOT NULL REFERENCES users(id),expires_at INTEGER NOT NULL,created_at INTEGER,last_seen INTEGER,remote_ip TEXT,user_agent TEXT); CREATE TABLE IF NOT EXISTS audit (id INTEGER PRIMARY KEY,at INTEGER NOT NULL,actor TEXT NOT NULL,action TEXT NOT NULL,detail TEXT NOT NULL); CREATE TABLE IF NOT EXISTS auth_failures (username TEXT PRIMARY KEY,count INTEGER NOT NULL,window_start INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS metric_samples (at INTEGER PRIMARY KEY,cpu REAL NOT NULL,memory REAL NOT NULL,disk REAL NOT NULL,rx_rate REAL NOT NULL,tx_rate REAL NOT NULL); CREATE TABLE IF NOT EXISTS network_traffic_hourly (hour_start INTEGER NOT NULL,interface TEXT NOT NULL,rx_bytes INTEGER NOT NULL,tx_bytes INTEGER NOT NULL,eligible_for_total INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(hour_start,interface)); CREATE INDEX IF NOT EXISTS network_traffic_hourly_time ON network_traffic_hourly(hour_start); CREATE TABLE IF NOT EXISTS managed_firewall_rules (id INTEGER PRIMARY KEY,port INTEGER NOT NULL,protocol TEXT NOT NULL,policy TEXT NOT NULL,enabled INTEGER NOT NULL,UNIQUE(port,protocol)); CREATE TABLE IF NOT EXISTS nginx_site_metadata (name TEXT PRIMARY KEY,description TEXT NOT NULL DEFAULT ''); CREATE TABLE IF NOT EXISTS nginx_site_config_state (name TEXT PRIMARY KEY,content TEXT NOT NULL,settings_json TEXT,content_hash TEXT NOT NULL,updated_at INTEGER NOT NULL);")?;
    ensure_sessions_metadata(&db)?;
    let has_users = initialized(&db);
    let configured_setup_token = env::var("ARG_SETUP_TOKEN")
        .ok()
        .filter(|value| !value.is_empty());
    if !has_users && configured_setup_token.is_none() {
        let generated = token();
        let inserted = db.execute(
            "INSERT OR IGNORE INTO settings(key,value) VALUES ('setup_token',?1)",
            [&generated],
        )?;
        if inserted > 0 {
            println!("ARG_SETUP_CODE={generated}");
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
        .route("/api/account/username", post(change_username))
        .route("/api/account/sessions", get(list_account_sessions))
        .route("/api/account/sessions/{id}", axum::routing::delete(revoke_account_session))
        .route("/api/account/sessions/revoke-others", post(revoke_other_account_sessions))
        .route("/api/terminal", get(terminal_socket))
        .route("/api/dashboard", get(dashboard))
        .route("/api/history", get(history))
        .route("/api/network/traffic", get(network_traffic_volume))
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
        .route("/api/update/progress", get(panel_update_progress))
        .route("/api/update/logs", get(panel_update_logs))
        .route("/api/packages", get(list_packages))
        .route("/api/packages/action", post(run_apt_operation))
        .route("/api/services", get(list_system_services))
        .route("/api/services/action", post(run_service_operation))
        .route("/api/docker", get(docker_status))
        .route("/api/docker/action", post(docker_operation))
        .route("/api/network/probe", post(port_probe))
        .route("/api/users", get(users).post(add_user))
        .route("/api/users/{id}", put(update_user))
        .route("/api/audit", get(audit_log))
        .route("/api/nginx/sites", get(nginx_sites).post(create_nginx_site))
        .route("/api/nginx/sites/{name}/rename", post(rename_nginx_site))
        .route("/api/nginx/sites/{name}/metadata", put(update_nginx_site_metadata))
        .route("/api/nginx/sites/{name}/logs", post(api_nginx_site_logs))
        .route("/api/nginx/sites/{name}/sync", post(sync_nginx_site))
        .route("/api/nginx/sites/{name}/ssl", post(issue_nginx_site_ssl))
        .route("/api/nginx/domains/check", get(check_nginx_domains))
        .route("/api/nginx/status", get(nginx_status))
        .route("/api/nginx/files", get(api_nginx_browse_files))
        .route("/api/nginx/action", post(nginx_action))
        .route(
            "/api/nginx/sites/{name}",
            get(get_nginx_site).put(save_nginx_site).delete(delete_nginx_site),
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
        .layer(middleware::from_fn(update_guard))
        .with_state(state);
    let bind = env::var("ARG_BIND").unwrap_or_else(|_| "127.0.0.1:8282".into());
    let addr: SocketAddr = bind.parse()?;
    println!("ARG listening at http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let tls_cert = env::var("ARG_TLS_CERT").ok();
    let tls_key = env::var("ARG_TLS_KEY").ok();
    match (tls_cert, tls_key) {
        (Some(cert), Some(key)) => {
            rustls::crypto::ring::default_provider()
                .install_default()
                .map_err(|_| "Failed to configure the TLS crypto provider")?;
            let tls_bind = env::var("ARG_TLS_BIND")
                .unwrap_or_else(|_| "0.0.0.0:443".into())
                .parse::<SocketAddr>()?;
            let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key).await?;
            println!("ARG listening at https://{tls_bind}");
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
        _ => return Err("Both ARG_TLS_CERT and ARG_TLS_KEY must be set".into()),
    }
    Ok(())
}
async fn state_handler(state: State<AppState>, headers: HeaderMap) -> ApiResult {
    self::state(state, headers).await
}

#[cfg(test)]
mod update_progress_tests {
    use super::{firewall_rule_validation_error, update_progress_payload, valid_docker_image, valid_docker_object, valid_docker_port, FirewallRuleInput};

    #[test]
    fn docker_inputs_reject_shell_metacharacters_and_invalid_ports() {
        assert!(valid_docker_object("web_app-1"));
        assert!(!valid_docker_object("../etc"));
        assert!(!valid_docker_object("web;id"));
        assert!(valid_docker_image("registry.example/app:v1.2"));
        assert!(!valid_docker_image("app; touch /tmp/pwned"));
        assert!(valid_docker_port("8080:80/tcp"));
        assert!(valid_docker_port("5353:53/udp"));
        assert!(!valid_docker_port("65536:80/tcp"));
        assert!(!valid_docker_port("80:80/icmp"));
    }

    #[test]
    fn nginx_file_content_supports_current_and_legacy_manager_payloads() {
        let response = serde_json::json!({"site":{"name":"example.conf","content":"server { listen 80; }"}});
        assert_eq!(super::nginx_site_content(&response).unwrap(), "server { listen 80; }");
        let legacy = serde_json::json!({"name":"example.conf","content":"legacy config"});
        assert_eq!(super::nginx_site_content(&legacy).unwrap(), "legacy config");
        let malformed = serde_json::json!({"site":{"name":"example.conf"}});
        assert!(super::nginx_site_content(&malformed).is_err());
    }

    #[test]
    fn root_terminal_accepts_only_numeric_pty_device_paths() {
        assert!(super::valid_terminal_pty_path("/dev/pts/17"));
        assert!(!super::valid_terminal_pty_path("/dev/pts/../../etc/passwd"));
        assert!(!super::valid_terminal_pty_path("/dev/tty0"));
    }

    #[test]
    fn active_job_locks_only_while_authoritative_state_is_running() {
        let running = update_progress_payload(Some(r#"{"job_id":"job-1","state":"running","phase":"building","started_at":100,"message":""}"#), Some(100), "instance-a");
        assert_eq!(running["updating"], true);
        assert_eq!(running["phase"], "building");

        let succeeded = update_progress_payload(Some(r#"{"job_id":"job-1","state":"succeeded","phase":"complete","started_at":100,"message":"done"}"#), Some(100), "instance-b");
        assert_eq!(succeeded["updating"], false);
        assert_eq!(succeeded["state"], "succeeded");
    }

    #[test]
    fn failed_and_orphaned_jobs_are_recoverable_not_locked() {
        let failed = update_progress_payload(Some(r#"{"job_id":"job-2","state":"failed","phase":"failed","started_at":100,"message":"build failed"}"#), None, "instance-a");
        assert_eq!(failed["updating"], false);
        assert_eq!(failed["state"], "failed");

        let orphaned = update_progress_payload(Some(r#"{"job_id":"job-3","state":"running","phase":"building","started_at":100,"message":""}"#), None, "instance-a");
        assert_eq!(orphaned["updating"], false);
        assert_eq!(orphaned["state"], "failed");
        assert!(orphaned["message"].as_str().unwrap().contains("final result"));
    }

    #[test]
    fn malformed_state_falls_back_to_the_update_marker() {
        let fallback = update_progress_payload(Some("not json"), Some(123), "instance-a");
        assert_eq!(fallback["updating"], true);
        assert_eq!(fallback["started_at"], 123);
    }

    #[test]
    fn sessions_metadata_migration_is_idempotent_and_preserves_existing_rows() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE sessions (token_hash TEXT PRIMARY KEY,user_id INTEGER NOT NULL,expires_at INTEGER NOT NULL); INSERT INTO sessions VALUES ('hash',7,9999999999);").unwrap();
        super::ensure_sessions_metadata(&db).unwrap();
        super::ensure_sessions_metadata(&db).unwrap();
        let row: (String, i64, Option<i64>) = db.query_row("SELECT token_hash,expires_at,created_at FROM sessions", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!(row, ("hash".to_owned(), 9999999999, None));
    }

    #[test]
    fn network_deltas_split_at_utc_hour_boundaries_without_losing_bytes() {
        let buckets = super::split_traffic_delta(3598, 3602, 101, 57);
        assert_eq!(buckets, vec![(0, 50, 28), (3600, 51, 29)]);
        assert_eq!(buckets.iter().map(|bucket| bucket.1).sum::<u64>(), 101);
        assert_eq!(buckets.iter().map(|bucket| bucket.2).sum::<u64>(), 57);
        assert!(super::split_traffic_delta(10, 10, 500, 200).is_empty());
    }

    #[test]
    fn daily_traffic_buckets_keep_known_rx_and_tx_totals() {
        let rx = 1536u64 * 1024 * 1024;
        let tx = 512u64 * 1024 * 1024;
        let buckets = super::split_traffic_delta(0, 24 * 3600, rx, tx);
        assert_eq!(buckets.len(), 24);
        assert_eq!(buckets.iter().map(|bucket| bucket.1).sum::<u64>(), rx);
        assert_eq!(buckets.iter().map(|bucket| bucket.2).sum::<u64>(), tx);
    }

    #[test]
    fn website_ports_are_configurable_but_ssh_and_panel_ports_stay_protected() {
        for port in [80, 443] {
            let rule = FirewallRuleInput {
                port,
                protocol: "tcp".into(),
                policy: "allow".into(),
                enabled: true,
            };
            assert_eq!(firewall_rule_validation_error(&rule), None);
        }
        for port in [22, 8282, 8443] {
            let rule = FirewallRuleInput {
                port,
                protocol: "tcp".into(),
                policy: "allow".into(),
                enabled: true,
            };
            assert!(firewall_rule_validation_error(&rule).is_some());
        }
    }
}
