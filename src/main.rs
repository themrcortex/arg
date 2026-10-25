use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::Request,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Path, Query, State,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode, Uri},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream as StdUnixStream;
use std::os::unix::{
    fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    io::AsRawFd,
    process::CommandExt,
};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{self, BufRead, BufReader},
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader as AsyncBufReader},
    net::{UnixListener, UnixStream},
};

const PRIVILEGED_SOCKET: &str = "/run/arg-privileged/manager.sock";
const UPDATE_STATUS_FILE: &str = "/run/arg-update-in-progress";
const PRIVILEGED_REQUEST_MAX_BYTES: usize = 4 * 1024 * 1024;
static PRIVILEGED_REQUESTS: std::sync::LazyLock<Arc<tokio::sync::Semaphore>> =
    std::sync::LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(8)));
static UPDATE_OPERATION_GATE: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());
static APT_OPERATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static ROOT_TERMINAL_PIDS: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();

type AppState = Arc<Mutex<Connection>>;
type ApiResult = Result<Json<Value>, (StatusCode, Json<Value>)>;

mod docker;
mod files;
mod fast_actions;
mod metrics;
mod maintenance;
mod mysql;
mod nginx;
mod routes;
mod s3;
mod storage_mounts;

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
    let mut fields = value.split_whitespace();
    let started = fields.next()?.parse::<i64>().ok()?;
    let owner = fields.next().and_then(|value| value.parse::<u32>().ok());
    let owner_start = fields.next().and_then(|value| value.parse::<u64>().ok());
    let owner_active = owner.is_some_and(|pid| {
        if let Some(expected_start) = owner_start {
            return fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .is_some_and(|stat| {
                    let fields: Vec<_> = stat.split_whitespace().collect();
                    fields.get(2).is_some_and(|state| !matches!(*state, "Z" | "X"))
                        && fields.get(21).and_then(|value| value.parse::<u64>().ok())
                            == Some(expected_start)
                });
        }
        let command = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        let command = String::from_utf8_lossy(&command).replace('\0', " ");
        command.contains("/usr/local/bin/arg")
            && command.split_whitespace().any(|part| part == "update")
    });
    if owner_active {
        return Some(started);
    }
    let unit = fs::read_to_string("/run/arg-update-last-unit").unwrap_or_default();
    let unit = unit.trim();
    let scheduled_unit_active = if unit.starts_with("arg-web-update-")
        && unit["arg-web-update-".len()..].bytes().all(|byte| byte.is_ascii_digit())
    {
        ["service", "timer"].iter().any(|suffix| {
            std::process::Command::new("systemctl")
                .args(["is-active", "--quiet", &format!("{unit}.{suffix}")])
                .status()
                .is_ok_and(|status| status.success())
        })
    } else {
        false
    };
    (owner_active || scheduled_unit_active).then_some(started)
}

fn update_progress_payload(
    raw: Option<&str>,
    marker_started: Option<i64>,
    instance: &str,
) -> Value {
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
        if let Some(started) = marker_started {
            job["started_at"] = json!(started);
        }
    }
    job
}

fn ensure_sessions_metadata(db: &Connection) -> Result<(), rusqlite::Error> {
    let mut statement = db.prepare("PRAGMA table_info(sessions)")?;
    let existing: Vec<String> = statement
        .query_map([], |row| row.get(1))?
        .filter_map(Result::ok)
        .collect();
    drop(statement);
    for (name, definition) in [
        ("created_at", "INTEGER"),
        ("last_seen", "INTEGER"),
        ("remote_ip", "TEXT"),
        ("user_agent", "TEXT"),
    ] {
        if !existing.iter().any(|column| column == name) {
            db.execute_batch(&format!(
                "ALTER TABLE sessions ADD COLUMN {name} {definition}"
            ))?;
        }
    }
    Ok(())
}

fn update_guard_exempt(path: &str) -> bool {
    matches!(
        path,
        "/api/update" | "/api/update/progress" | "/api/update/cancel" | "/api/state" | "/api/logout" | "/api/login"
    )
}

async fn update_guard(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path();
    let exempt = update_guard_exempt(path);
    let writes_api = method != axum::http::Method::GET
        && method != axum::http::Method::HEAD
        && path.starts_with("/api/")
        && !exempt;
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
    cookie_named(headers, "arg_session")
}
fn cookie_named(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            (key == name).then(|| value.to_owned())
        })
}
fn consume_root_terminal_ticket(
    db: &Connection,
    ticket: &str,
    user_id: i64,
    login_session: &str,
) -> bool {
    let hash = session_hash(ticket);
    let valid = db.query_row(
        "SELECT 1 FROM root_terminal_tickets WHERE token_hash=?1 AND user_id=?2 AND session_hash=?3 AND expires_at>?4",
        params![hash, user_id, login_session, now()],
        |_| Ok(()),
    ).is_ok();
    valid
        && db
            .execute(
                "DELETE FROM root_terminal_tickets WHERE token_hash=?1",
                [hash],
            )
            .is_ok()
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
        && !secure_transport(&headers, remote.ip())
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
        || input.password.len() > 1024
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Username must be 3-32 characters and password 12-1024 bytes",
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
    let secure_transport = secure_transport(&headers, remote.ip());
    if !remote.ip().is_loopback() && !secure_transport {
        return Err(error(
            StatusCode::FORBIDDEN,
            "Use the HTTPS panel URL to sign in. For initial setup, connect through an SSH tunnel.",
        ));
    }
    let ip_throttle_key = (!remote.ip().is_loopback())
        .then(|| format!("login-ip:{}", remote.ip()));
    if input.username.len() > 32 || input.password.len() > 1024 {
        let db = state.lock().unwrap();
        if let Some(key) = ip_throttle_key.as_deref() {
            record_login_failure(&db, key);
        }
        return Err(error(StatusCode::UNAUTHORIZED, "Invalid credentials"));
    }
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
    if let Some(key) = ip_throttle_key.as_deref() {
        let ip_attempts: (i64, i64) = db
            .query_row(
                "SELECT count,window_start FROM auth_failures WHERE username=?1",
                [key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0, 0));
        if ip_attempts.0 >= 20 && now() - ip_attempts.1 < 600 {
            return Err(error(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many login attempts. Try again later",
            ));
        }
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
            if let Some(key) = ip_throttle_key.as_deref() {
                record_login_failure(&db, key);
            }
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
        if let Some(key) = ip_throttle_key.as_deref() {
            record_login_failure(&db, key);
        }
        return Err(error(StatusCode::UNAUTHORIZED, "Invalid credentials"));
    }
    let _ = db.execute(
        "DELETE FROM auth_failures WHERE username=?1",
        [&input.username],
    );
    if let Some(key) = ip_throttle_key.as_deref() {
        let _ = db.execute("DELETE FROM auth_failures WHERE username=?1", [key]);
    }
    let raw = token();
    let session_seconds = setting_u64(&db, "session_hours", 24).clamp(1, 72) as i64 * 3600;
    let created_at = now();
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(256)
                .collect::<String>()
        });
    db.execute(
        "INSERT INTO sessions (token_hash,user_id,expires_at,created_at,last_seen,remote_ip,user_agent) VALUES (?1,?2,?3,?4,?4,?5,?6)",
        params![session_hash(&raw), id, created_at + session_seconds, created_at, remote.ip().to_string(), user_agent],
    )
    .map_err(db_error)?;
    audit(&db, &input.username, "login", "Successful login");
    let cookie = format!(
        "arg_session={raw}; HttpOnly; SameSite=Strict; Path=/; Max-Age={session_seconds}{}",
        if secure_transport { "; Secure" } else { "" }
    );
    Ok(([(header::SET_COOKIE, cookie)], Json(json!({"ok": true}))).into_response())
}
fn record_login_failure(db: &Connection, username: &str) {
    if username.len() > 64 {
        return;
    }
    let _ = db.execute("INSERT INTO auth_failures(username,count,window_start) VALUES (?1,1,?2) ON CONFLICT(username) DO UPDATE SET count=CASE WHEN ?2-window_start>=600 THEN 1 ELSE count+1 END,window_start=CASE WHEN ?2-window_start>=600 THEN ?2 ELSE window_start END", params![username,now()]);
}

fn secure_transport(headers: &HeaderMap, peer: std::net::IpAddr) -> bool {
    headers
        .get("x-arg-transport")
        .and_then(|value| value.to_str().ok())
        == Some("https")
        || (peer.is_loopback()
            && headers
                .get("x-forwarded-proto")
                .and_then(|value| value.to_str().ok())
                == Some("https"))
}

async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PasswordChange>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let user = require_user(&db, &headers)?;
    if input.new_password.len() < 12
        || input.new_password.len() > 1024
        || input.current_password.len() > 1024
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Passwords must be at most 1024 bytes and the new password at least 12 characters",
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
    if input.username.len() < 3
        || input.username.len() > 32
        || !input
            .username
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Username must be 3-32 ASCII letters, digits, '_' or '-'",
        ));
    }
    let db = state.lock().unwrap();
    let user = require_user(&db, &headers)?;
    let hash: String = db
        .query_row(
            "SELECT password_hash FROM users WHERE id=?1",
            [user.id],
            |row| row.get(0),
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
    match db.execute(
        "UPDATE users SET username=?1 WHERE id=?2",
        params![input.username, user.id],
    ) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(ref code, _))
            if code.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            return Err(error(
                StatusCode::CONFLICT,
                "That username is already in use",
            ));
        }
        Err(_) => return Err(db_error(rusqlite::Error::InvalidQuery)),
    }
    audit(&db, &input.username, "account.username", "Username changed");
    Ok(Json(json!({"ok":true,"username":input.username})))
}

async fn list_account_sessions(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    let user = require_user(&db, &headers)?;
    let current_hash = cookie_token(&headers)
        .map(|raw| session_hash(&raw))
        .unwrap_or_default();
    let mut statement = db.prepare("SELECT rowid,created_at,last_seen,expires_at,remote_ip,user_agent,token_hash FROM sessions WHERE user_id=?1 AND expires_at>?2 ORDER BY (token_hash=?3) DESC, COALESCE(last_seen,created_at,0) DESC").map_err(db_error)?;
    let rows = statement.query_map(params![user.id, now(), current_hash], |row| Ok(json!({
        "id":row.get::<_,i64>(0)?,"created_at":row.get::<_,Option<i64>>(1)?,"last_seen":row.get::<_,Option<i64>>(2)?,
        "expires_at":row.get::<_,i64>(3)?,"remote_ip":row.get::<_,Option<String>>(4)?,"user_agent":row.get::<_,Option<String>>(5)?,
        "current":row.get::<_,String>(6)? == current_hash
    }))).map_err(db_error)?;
    let sessions: Vec<Value> = rows.filter_map(Result::ok).collect();
    Ok(Json(json!({"sessions":sessions})))
}

async fn revoke_account_session(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let user = require_user(&db, &headers)?;
    let current_hash = cookie_token(&headers)
        .map(|raw| session_hash(&raw))
        .unwrap_or_default();
    let target_hash: Option<String> = db
        .query_row(
            "SELECT token_hash FROM sessions WHERE rowid=?1 AND user_id=?2",
            params![id, user.id],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?;
    let Some(target_hash) = target_hash else {
        return Err(error(StatusCode::NOT_FOUND, "Session not found"));
    };
    if target_hash == current_hash {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Use sign out to close the current session",
        ));
    }
    db.execute(
        "DELETE FROM sessions WHERE rowid=?1 AND user_id=?2",
        params![id, user.id],
    )
    .map_err(db_error)?;
    audit(
        &db,
        &user.username,
        "account.session.revoke",
        "Another account session was revoked",
    );
    Ok(Json(json!({"ok":true})))
}

async fn revoke_other_account_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let user = require_user(&db, &headers)?;
    let current_hash = cookie_token(&headers)
        .map(|raw| session_hash(&raw))
        .unwrap_or_default();
    let removed = db
        .execute(
            "DELETE FROM sessions WHERE user_id=?1 AND token_hash<>?2",
            params![user.id, current_hash],
        )
        .map_err(db_error)?;
    audit(
        &db,
        &user.username,
        "account.sessions.revoke_others",
        "Other account sessions were revoked",
    );
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
    if new_password.len() < 12 || new_password.len() > 1024 || username.len() > 32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Username must be valid and password must be 12-1024 bytes",
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
    db.execute(
        "DELETE FROM network_traffic_hourly WHERE hour_start<?1",
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
#[derive(Deserialize)]
struct PanelTimezoneInput {
    timezone: String,
}
fn valid_panel_timezone(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 64
        || value.starts_with('/')
        || value.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+'))
        })
    {
        return false;
    }
    let Ok(root) = fs::canonicalize("/usr/share/zoneinfo") else {
        return false;
    };
    fs::canonicalize(root.join(value))
        .is_ok_and(|resolved| resolved.starts_with(&root) && resolved.is_file())
}
fn panel_timezone(db: &Connection) -> String {
    db.query_row(
        "SELECT value FROM settings WHERE key='panel_timezone'",
        [],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .filter(|value| valid_panel_timezone(value))
    .unwrap_or_else(|| "UTC".to_owned())
}
async fn get_panel_timezone(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    Ok(Json(json!({"timezone": panel_timezone(&db)})))
}
async fn change_panel_timezone(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PanelTimezoneInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let timezone = input.timezone.trim();
    if !valid_panel_timezone(timezone) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Invalid IANA time zone. Choose a zone such as Asia/Tehran.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    db.execute(
        "INSERT INTO settings(key,value) VALUES ('panel_timezone',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [timezone],
    )
    .map_err(db_error)?;
    audit(&db, &actor.username, "panel.timezone", timezone);
    Ok(Json(json!({"timezone": timezone})))
}

#[derive(Deserialize)]
struct PanelTlsInput {
    certificate_path: String,
    private_key_path: String,
    server_name: String,
    port: u16,
}

async fn get_panel_tls_settings(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    let configured_cert = env::var("ARG_TLS_CERT").unwrap_or_default();
    let configured_key = env::var("ARG_TLS_KEY").unwrap_or_default();
    let mut discovered = None;
    if configured_cert.is_empty() || configured_key.is_empty() {
        let domain = fs::read_to_string("/etc/arg/https-domain").unwrap_or_default().trim().to_owned();
        let mut names = vec!["arg".to_owned()];
        if !domain.is_empty() && domain.bytes().all(|byte| byte.is_ascii_alphanumeric() || b".-".contains(&byte)) && !names.contains(&domain) {
            names.push(domain);
        }
        for name in names {
            let cert = format!("/etc/letsencrypt/live/{name}/fullchain.pem");
            let key = format!("/etc/letsencrypt/live/{name}/privkey.pem");
            if PathBuf::from(&cert).is_file() && PathBuf::from(&key).is_file() {
                discovered = Some((cert, key));
                break;
            }
        }
    }
    let (certificate_path, private_key_path) = match discovered {
        Some((cert, key)) if configured_cert.is_empty() || configured_key.is_empty() => (if configured_cert.is_empty() { cert } else { configured_cert }, if configured_key.is_empty() { key } else { configured_key }),
        _ => (configured_cert, configured_key),
    };
    let bind = env::var("ARG_TLS_BIND").unwrap_or_else(|_| "0.0.0.0:443".to_owned());
    let port = bind.rsplit(':').next().and_then(|part| part.parse::<u16>().ok()).unwrap_or(443);
    let certificate_present = !certificate_path.is_empty() && PathBuf::from(&certificate_path).is_file();
    let private_key_present = !private_key_path.is_empty() && PathBuf::from(&private_key_path).is_file();
    Ok(Json(json!({
        "enabled": certificate_present && private_key_present,
        "certificate_path": certificate_path,
        "private_key_path": private_key_path,
        "port": port,
        "certificate_present": certificate_present,
        "private_key_present": private_key_present,
        "default_certificate_path": if certificate_path.is_empty() { "/etc/arg/tls/fullchain.pem" } else { &certificate_path },
        "default_private_key_path": if private_key_path.is_empty() { "/etc/arg/tls/privkey.pem" } else { &private_key_path }
    })))
}

async fn update_panel_tls_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PanelTlsInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !(1024..=65535).contains(&input.port) {
        return Err(error(StatusCode::BAD_REQUEST, "HTTPS port must be between 1024 and 65535"));
    }
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    let result = privileged_request_async(json!({
        "action": "tls-configure",
        "certificate_path": input.certificate_path,
        "private_key_path": input.private_key_path,
        "server_name": input.server_name,
        "port": input.port
    })).await?;
    if result["ok"].as_bool() != Some(true) {
        let message = result["error"].as_str().unwrap_or("Could not configure HTTPS");
        return Err(error(StatusCode::BAD_REQUEST, message));
    }
    let db = state.lock().unwrap();
    audit(&db, &actor.username, "panel.tls.configure", &format!("HTTPS certificate paths configured on port {}", input.port));
    Ok(Json(json!({"scheduled":true,"port":input.port,"message":"HTTPS settings were saved. ARG will restart shortly; reopen the panel using HTTPS."})))
}

async fn collect_metrics(state: AppState) {
    let mut previous_cpu = metrics::cpu_ticks();
    let mut previous_network = metrics::network_totals();
    let mut previous_traffic = metrics::network_interface_counters();
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
        let current_cpu = metrics::cpu_ticks();
        let current_network = metrics::network_totals();
        let current_traffic = metrics::network_interface_counters();
        let traffic_from = previous_traffic_at;
        let traffic_gap_is_reliable =
            at > traffic_from && at - traffic_from <= (interval_seconds as i64 * 2 + 2);
        let mut traffic_rows = Vec::new();
        if traffic_gap_is_reliable {
            for (name, (rx, tx, eligible)) in &current_traffic {
                if let Some((previous_rx, previous_tx, _)) = previous_traffic.get(name) {
                    let rx_delta = if rx >= previous_rx {
                        rx - previous_rx
                    } else {
                        0
                    };
                    let tx_delta = if tx >= previous_tx {
                        tx - previous_tx
                    } else {
                        0
                    };
                    for (hour, rx_bytes, tx_bytes) in
                        metrics::split_traffic_delta(traffic_from, at, rx_delta, tx_delta)
                    {
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
        let mem = metrics::meminfo();
        let memory_total = *mem.get("MemTotal").unwrap_or(&0);
        let memory_available = *mem.get("MemAvailable").unwrap_or(&0);
        let memory = if memory_total > 0 {
            (100.0 * (1.0 - memory_available as f64 / memory_total as f64)).clamp(0.0, 100.0)
        } else {
            0.0
        };
        let (disk_total, disk_free) = metrics::disk_usage();
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
                    let _ = db.execute(
                        "DELETE FROM network_traffic_hourly WHERE hour_start<?1",
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
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "بازهٔ دلخواه تاریخ معتبر نیست یا از 366 روز بیشتر است.",
                ));
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
        _ => {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "برای بازهٔ دلخواه، زمان شروع و پایان لازم است.",
            ))
        }
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
    #[serde(default)]
    range: Option<String>,
    #[serde(default)]
    granularity: Option<String>,
    #[serde(default)]
    from: Option<i64>,
    #[serde(default)]
    to: Option<i64>,
    #[serde(default)]
    interface: Option<String>,
}

async fn network_traffic_volume(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TrafficVolumeQuery>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    let current = now();
    let range = query.range.as_deref().unwrap_or("24h");
    let (from, to) = if range == "custom" {
        match (query.from, query.to) {
            (Some(from), Some(to))
                if from >= 0 && from < to && to <= current && to - from <= 366 * 86400 =>
            {
                (from, to)
            }
            _ => {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "بازهٔ دلخواه ترافیک معتبر نیست یا از 366 روز بیشتر است.",
                ))
            }
        }
    } else {
        let seconds = match range {
            "24h" => 86400,
            "7d" => 7 * 86400,
            "30d" => 30 * 86400,
            _ => {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "بازهٔ زمانی پشتیبانی نمی‌شود.",
                ))
            }
        };
        (current - seconds, current)
    };
    let granularity = query
        .granularity
        .as_deref()
        .unwrap_or(if to - from <= 2 * 86400 {
            "hour"
        } else {
            "day"
        });
    if !["hour", "day"].contains(&granularity) || (granularity == "hour" && to - from > 90 * 86400)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "برای بازه‌های بیش از 90 روز از گروه‌بندی روزانه استفاده کن.",
        ));
    }
    let selected_interface = query.interface.clone().unwrap_or_else(|| "all".into());
    let interface = if selected_interface == "all" {
        None
    } else {
        Some(selected_interface.clone())
    };
    if interface.as_ref().is_some_and(|name| {
        name.len() > 32
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.:-".contains(&byte))
    }) {
        return Err(error(StatusCode::BAD_REQUEST, "نام کارت شبکه معتبر نیست."));
    }
    let bucket_expr = if granularity == "hour" {
        "hour_start"
    } else {
        "(hour_start / 86400) * 86400"
    };
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
    Ok(Json(
        json!({"from":from,"to":to,"granularity":granularity,"interface":selected_interface,"interfaces":interface_query,"buckets":buckets,"fresh_at":fresh_at,"enabled":setting_u64(&db,"history_enabled",1)==1,"timezone":panel_timezone(&db)}),
    ))
}
#[derive(Deserialize, Default)]
struct TerminalQuery {
    mode: Option<String>,
}

#[derive(Deserialize)]
struct RootTerminalAuthorization {
    password: String,
}

async fn authorize_root_terminal(
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<RootTerminalAuthorization>,
) -> Result<Response, (StatusCode, Json<Value>)> {
    check_origin(&headers)?;
    if input.password.is_empty() || input.password.len() > 1024 {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "Invalid administrator credentials",
        ));
    }
    let (actor, ticket) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?;
        let login_session = cookie_token(&headers)
            .map(|value| session_hash(&value))
            .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "Login required"))?;
        let throttle_key = format!("root-terminal:{}", actor.id);
        let attempts: (i64, i64) = db
            .query_row(
                "SELECT count,window_start FROM auth_failures WHERE username=?1",
                [&throttle_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or((0, 0));
        if attempts.0 >= 8 && now() - attempts.1 < 600 {
            return Err(error(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many Root Terminal password attempts. Try again later.",
            ));
        }
        let password_hash: String = db
            .query_row(
                "SELECT password_hash FROM users WHERE id=?1 AND enabled=1",
                [actor.id],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        let parsed = PasswordHash::new(&password_hash).map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Invalid stored password hash",
            )
        })?;
        if Argon2::default()
            .verify_password(input.password.as_bytes(), &parsed)
            .is_err()
        {
            record_login_failure(&db, &throttle_key);
            return Err(error(
                StatusCode::UNAUTHORIZED,
                "Administrator password is incorrect",
            ));
        }
        let _ = db.execute(
            "DELETE FROM auth_failures WHERE username=?1",
            [&throttle_key],
        );
        let _ = db.execute(
            "DELETE FROM root_terminal_tickets WHERE expires_at<=?1",
            [now()],
        );
        let ticket = token();
        db.execute(
            "INSERT INTO root_terminal_tickets(token_hash,user_id,session_hash,expires_at) VALUES (?1,?2,?3,?4)",
            params![session_hash(&ticket), actor.id, login_session, now() + 60],
        ).map_err(db_error)?;
        (actor, ticket)
    };
    let secure = secure_transport(&headers, remote.ip());
    let mut cookie = format!(
        "arg_root_ticket={ticket}; HttpOnly; SameSite=Strict; Path=/api/terminal; Max-Age=60"
    );
    if secure {
        cookie.push_str("; Secure");
    }
    let mut response = Json(json!({"authorized":true,"expires_in":60})).into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, cookie.parse().unwrap());
    {
        let db = state.lock().unwrap();
        audit(
            &db,
            &actor.username,
            "terminal.root.authorize",
            "fresh password verification",
        );
    }
    Ok(response)
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
            Err(_) => {
                return (StatusCode::FORBIDDEN, "Administrator access required").into_response()
            }
        };
        if root_mode {
            let Some(ticket) = cookie_named(&headers, "arg_root_ticket") else {
                return (
                    StatusCode::UNAUTHORIZED,
                    "Re-enter the administrator password to open Root Terminal",
                )
                    .into_response();
            };
            let login_session = cookie_token(&headers)
                .map(|value| session_hash(&value))
                .unwrap_or_default();
            if !consume_root_terminal_ticket(&db, &ticket, actor.id, &login_session) {
                return (
                    StatusCode::UNAUTHORIZED,
                    "Root Terminal authorization expired; verify your password again",
                )
                    .into_response();
            }
            audit(
                &db,
                &actor.username,
                "terminal.root.open",
                "one-time reauthenticated root terminal opened",
            );
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

async fn run_terminal(
    mut socket: WebSocket,
    state: AppState,
    terminal_session_hash: String,
    root_mode: bool,
) {
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
        let Some(tty) = master
            .lock()
            .unwrap()
            .tty_name()
            .map(|path| path.to_string_lossy().into_owned())
        else {
            return;
        };
        let response = tokio::task::spawn_blocking(move || {
            privileged_request(json!({"action":"terminal-root-start","tty":tty}))
        })
        .await;
        let failure = match response {
            Ok(Ok(value)) if value["ok"] == true => {
                root_pid = value["pid"].as_u64().map(|pid| pid as u32);
                None
            }
            Ok(Ok(value)) => Some(
                value["error"]
                    .as_str()
                    .unwrap_or("Privileged manager rejected the root terminal")
                    .to_owned(),
            ),
            Ok(Err((_, body))) => Some(
                body.0["error"]
                    .as_str()
                    .unwrap_or("Privileged manager is unavailable")
                    .to_owned(),
            ),
            Err(_) => Some("Could not start the root terminal helper".to_owned()),
        };
        if let Some(message) = failure {
            let _ = socket
                .send(Message::Text(
                    format!("\r\n\x1b[31mRoot terminal failed: {message}\x1b[0m\r\n").into(),
                ))
                .await;
            return;
        }
    } else {
        let executable = match env::current_exe() {
            Ok(path) => path,
            Err(_) => return,
        };
        let mut command = CommandBuilder::new(executable);
        command.arg("terminal-shell");
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command
            .cwd(env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/var/lib/arg")));
        match pair.slave.spawn_command(command) {
            Ok(process) => child = Some(process),
            Err(_) => return,
        }
    }
    drop(pair.slave);
    let writer_result = { master.lock().unwrap().take_writer() };
    let writer = match writer_result {
        Ok(writer) => Arc::new(Mutex::new(writer)),
        Err(_) => {
            if let Some(child) = child.as_mut() {
                let _ = child.kill();
            }
            if let Some(pid) = root_pid {
                let _ = privileged_request_async(json!({"action":"terminal-root-stop","pid":pid}))
                    .await;
            }
            return;
        }
    };
    let reader_result = { master.lock().unwrap().try_clone_reader() };
    let reader = match reader_result {
        Ok(reader) => reader,
        Err(_) => {
            if let Some(child) = child.as_mut() {
                let _ = child.kill();
            }
            if let Some(pid) = root_pid {
                let _ = privileged_request_async(json!({"action":"terminal-root-stop","pid":pid}))
                    .await;
            }
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
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
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
    } else if protected_firewall_ports().contains(&input.port) {
        Some("این پورت در حال استفاده برای SSH یا پنل ARG است و برای جلوگیری از قطع دسترسی محافظت شده است.")
    } else if !["tcp", "udp"].contains(&input.protocol.as_str()) {
        Some("پروتکل TCP یا UDP را انتخاب کن.")
    } else if !["allow", "deny"].contains(&input.policy.as_str()) {
        Some("مشخص کن این قانون ترافیک را مجاز یا مسدود کند.")
    } else {
        None
    }
}
fn protected_firewall_ports() -> HashSet<u16> {
    let sshd = std::process::Command::new("sshd")
        .arg("-T")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned());
    protected_firewall_ports_from(
        env::var("ARG_BIND").ok().as_deref(),
        env::var("ARG_TLS_BIND").ok().as_deref(),
        sshd.as_deref(),
    )
}
fn protected_firewall_ports_from(
    bind: Option<&str>,
    tls_bind: Option<&str>,
    sshd_output: Option<&str>,
) -> HashSet<u16> {
    let mut ports = HashSet::from([22, 8282, 8443]);
    for value in [bind, tls_bind].into_iter().flatten() {
        if let Ok(address) = value.parse::<SocketAddr>() {
            ports.insert(address.port());
        }
    }
    if let Some(output) = sshd_output {
        for line in output.lines() {
            let value = line
                .strip_prefix("port ")
                .or_else(|| {
                    line.strip_prefix("listenaddress ")
                        .and_then(|value| value.rsplit_once(':').map(|(_, port)| port))
                })
                .and_then(|value| value.parse::<u16>().ok());
            if let Some(value) = value {
                ports.insert(value);
            }
        }
    }
    ports
}
fn protected_service_mutation(unit: &str, operation: &str) -> bool {
    let protected = matches!(
        unit,
        "arg.service"
            | "ssh.service"
            | "sshd.service"
            | "systemd-networkd.service"
            | "NetworkManager.service"
            | "networking.service"
            | "systemd-resolved.service"
    );
    protected
        && [
            "stop",
            "restart",
            "try-restart",
            "disable",
            "disable-now",
            "mask",
            "set-property",
        ]
        .contains(&operation)
}
fn http_listener_addr(bind: SocketAddr, tls_enabled: bool) -> SocketAddr {
    if tls_enabled {
        SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, bind.port()))
    } else {
        bind
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
    let mut protected_ports: Vec<u16> = protected_firewall_ports().into_iter().collect();
    protected_ports.sort_unstable();
    Ok(Json(json!({
        "rules": rows.collect::<Result<Vec<_>,_>>().map_err(db_error)?,
        "protected_ports": protected_ports,
    })))
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
    let mem = metrics::meminfo();
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
    let (cpu_total, cpu_idle) = metrics::cpu_ticks();
    let (process_rss_bytes, process_cpu_percent) = metrics::process_usage();
    let (disk_total_bytes, disk_free_bytes) = metrics::disk_usage();
    let storage_inventory = storage_mounts::inventory();
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
        "storage": storage_inventory,
        "interfaces": metrics::network(),
        "ports": metrics::listening_ports()
    })))
}
async fn storage_volumes(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_user(&db, &headers)?;
    }
    Ok(Json(storage_mounts::inventory()))
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
        || input.password.len() > 1024
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
async fn update_status(branch: &str) -> Value {
    let branch = if valid_update_branch(branch) { branch } else { "main" };
    let output = match privileged_request_async(json!({"action":"update-check","branch":branch})).await {
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
    let remote_version = revisions
        .get(2)
        .copied()
        .filter(|value| *value != "unknown");
    json!({"status": if local == remote { "current" } else { "available" }, "local": &local[..7], "remote": &remote[..7], "local_version": env!("CARGO_PKG_VERSION"), "remote_version": remote_version, "branch": branch})
}
async fn panel_update_status(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<PanelUpdateQuery>) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_user(&db, &headers)?;
    }
    let mut status = if update_started_at().is_some() {
        json!({"status":"unavailable","error":"ARG_UPDATE_IN_PROGRESS"})
    } else {
        update_status(&query.branch).await
    };
    status["instance"] = json!(metrics::process_instance_id());
    Ok(Json(status))
}
async fn panel_update_progress(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_user(&db, &headers)?;
    }
    let fallback_started = update_started_at();
    let output = privileged_request_async(json!({"action":"update-progress"})).await;
    let raw = output
        .ok()
        .and_then(|value| value["stdout"].as_str().map(str::to_owned));
    Ok(Json(update_progress_payload(
        raw.as_deref(),
        fallback_started,
        metrics::process_instance_id(),
    )))
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
            output["error"]
                .as_str()
                .unwrap_or("Update details are temporarily unavailable"),
        ));
    }
    Ok(Json(
        json!({"logs":output["stdout"].as_str().unwrap_or("")}),
    ))
}

#[derive(Deserialize)]
struct AptOperation {
    action: String,
    #[serde(default)]
    packages: Vec<String>,
}
#[derive(Deserialize)]
struct ServiceOperation {
    action: String,
    unit: String,
}

async fn list_packages(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let result = privileged_request_async(json!({"action":"apt-list"})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            result["error"]
                .as_str()
                .unwrap_or("Package list unavailable"),
        ));
    }
    Ok(Json(result["data"].clone()))
}
async fn run_apt_operation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AptOperation>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    let _update_guard = UPDATE_OPERATION_GATE.read().await;
    let _apt_guard = APT_OPERATION_LOCK.lock().await;
    let allowed = [
        "update",
        "upgrade",
        "full-upgrade",
        "autoremove",
        "clean",
        "autoclean",
        "install",
        "remove",
    ];
    if !allowed.contains(&input.action.as_str())
        || input.packages.len() > 25
        || input.packages.iter().any(|p| !valid_package_name(p))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Invalid APT action or package name",
        ));
    }
    if ["install", "remove"].contains(&input.action.as_str()) && input.packages.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Select at least one package",
        ));
    }
    let result = privileged_request_async(
        json!({"action":"apt-run","operation":input.action,"packages":input.packages}),
    )
    .await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"].as_str().unwrap_or("APT operation failed"),
        ));
    }
    {
        let db = state.lock().unwrap();
        audit(&db, &actor.username, "apt.operation", &input.action);
    }
    Ok(Json(
        json!({"output":result["stdout"].as_str().unwrap_or("")}),
    ))
}
async fn list_system_services(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let result = privileged_request_async(json!({"action":"service-list"})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            result["error"]
                .as_str()
                .unwrap_or("Service list unavailable"),
        ));
    }
    Ok(Json(result["data"].clone()))
}
async fn run_service_operation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ServiceOperation>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    let _update_guard = UPDATE_OPERATION_GATE.read().await;
    let allowed = [
        "start",
        "stop",
        "restart",
        "try-restart",
        "reload",
        "reload-or-restart",
        "enable",
        "disable",
        "enable-now",
        "disable-now",
        "reenable",
        "mask",
        "unmask",
        "reset-failed",
        "status",
        "logs",
        "daemon-reload",
    ];
    if !allowed.contains(&input.action.as_str())
        || (input.action != "daemon-reload" && !valid_service_unit(&input.unit))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Invalid service action or unit",
        ));
    }
    let result = privileged_request_async(
        json!({"action":"service-action","operation":input.action,"unit":input.unit}),
    )
    .await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("Service operation failed"),
        ));
    }
    {
        let db = state.lock().unwrap();
        audit(
            &db,
            &actor.username,
            "systemd.service.action",
            &format!("{} {}", input.action, input.unit),
        );
    }
    Ok(Json(
        json!({"output":result["stdout"].as_str().unwrap_or("")}),
    ))
}
fn valid_package_name(name: &str) -> bool {
    let (base, arch) = name.split_once(':').unwrap_or((name, ""));
    let valid_base = !base.is_empty()
        && base.len() <= 128
        && base.as_bytes()[0].is_ascii_alphanumeric()
        && base
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"+.-".contains(&b));
    let valid_arch = !name.contains(':')
        || (!arch.is_empty()
            && arch
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-".contains(&b)));
    valid_base && valid_arch
}
fn valid_service_unit(unit: &str) -> bool {
    unit.len() <= 255
        && unit
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && unit.ends_with(".service")
        && unit
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@_.:-".contains(&b))
        && !unit.contains("..")
}
fn shared_web_service(unit: &str) -> bool {
    matches!(unit, "nginx.service" | "apache2.service" | "httpd.service")
        || (unit.starts_with("php") && unit.ends_with("-fpm.service"))
}
#[derive(Deserialize)]
struct PanelUpdateQuery {
    #[serde(default = "default_update_build_mode")]
    mode: String,
    #[serde(default = "default_update_branch")]
    branch: String,
}

fn default_update_build_mode() -> String {
    "complete".into()
}

fn default_update_branch() -> String {
    "main".into()
}

fn valid_update_build_mode(mode: &str) -> bool {
    matches!(mode, "fast" | "complete")
}

fn valid_update_branch(branch: &str) -> bool {
    matches!(branch, "main" | "development")
}

async fn schedule_panel_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(input): Query<PanelUpdateQuery>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_update_build_mode(&input.mode) {
        return Err(error(StatusCode::BAD_REQUEST, "Update build mode is invalid."));
    }
    if !valid_update_branch(&input.branch) {
        return Err(error(StatusCode::BAD_REQUEST, "Update branch is invalid."));
    }
    let _exclusive = UPDATE_OPERATION_GATE.write().await;
    if update_started_at().is_some() {
        return Err(error(
            StatusCode::CONFLICT,
            "ARG_UPDATE_IN_PROGRESS: another update is already running.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let output = privileged_request(json!({"action":"update-run","mode":input.mode,"branch":input.branch}))?;
    if output["ok"] != true {
        let detail = output["error"]
            .as_str()
            .unwrap_or("Update could not be scheduled; check service logs");
        let status = if detail.contains("ARG_UPDATE_IN_PROGRESS") {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_REQUEST
        };
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

async fn cancel_panel_update(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    let output = privileged_request_async(json!({"action":"update-cancel"})).await?;
    if output["ok"] != true {
        let detail = output["error"].as_str().unwrap_or("ARG update could not be cancelled.");
        return Err(error(StatusCode::CONFLICT, detail));
    }
    let db = state.lock().unwrap();
    audit(&db, &actor, "panel.update.cancel", "administrator cancelled the active update");
    Ok(Json(json!({"cancelled":true,"message":output["stdout"].as_str().unwrap_or("ARG update cancelled.")})))
}

async fn rollback_panel_update(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    let output = privileged_request_async(json!({"action":"update-rollback"})).await?;
    if output["ok"] != true {
        return Err(error(StatusCode::BAD_GATEWAY, output["error"].as_str().unwrap_or("ARG rollback failed.")));
    }
    let db = state.lock().unwrap();
    audit(&db, &actor, "panel.update.rollback", "administrator restored the previous ARG binary");
    Ok(Json(json!({"rolled_back":true,"message":output["stdout"].as_str().unwrap_or("Previous ARG version restored.")})))
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

fn valid_terminal_pty_path(path: &str) -> bool {
    path.strip_prefix("/dev/pts/").is_some_and(|number| {
        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn start_root_terminal(request: &Value) -> Result<u32, String> {
    let tty_path = request["tty"]
        .as_str()
        .ok_or("Terminal device was not provided")?;
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
        if entry.is_null() {
            return Err("ARG service account was not found".into());
        }
        (*entry).pw_uid
    };
    if metadata.uid() != arg_uid {
        return Err("Terminal device is not owned by the ARG service account".into());
    }

    // Keep the privileged helper detached from this PTY; only the spawned
    // shell should claim it as a controlling terminal after setsid().
    let tty = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY)
            .open(tty_path)
    }
    .map_err(|error| error.to_string())?;
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
        .env(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )
        .current_dir("/")
        .stdin(std::process::Stdio::from(tty))
        .stdout(std::process::Stdio::from(stdin))
        .stderr(std::process::Stdio::from(stdout));
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::ioctl(libc::STDIN_FILENO, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start root shell: {error}"))?;
    let pid = child.id();
    let pids = ROOT_TERMINAL_PIDS.get_or_init(|| Mutex::new(HashSet::new()));
    pids.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(pid);
    std::thread::spawn(move || {
        let _ = child.wait();
        if let Some(pids) = ROOT_TERMINAL_PIDS.get() {
            pids.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&pid);
        }
    });
    Ok(pid)
}

fn stop_root_terminal(request: &Value) -> Result<(), String> {
    let pid = request["pid"]
        .as_u64()
        .filter(|pid| *pid > 1 && *pid <= i32::MAX as u64)
        .ok_or("Invalid root terminal process id")? as u32;
    let pids = ROOT_TERMINAL_PIDS.get_or_init(|| Mutex::new(HashSet::new()));
    let removed = pids
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&pid);
    if !removed {
        return Ok(());
    }
    let result = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    if result != 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error.to_string());
        }
    }
    Ok(())
}

fn valid_tls_source_path(value: &str, private_key: bool) -> bool {
    let expected = if private_key { "privkey.pem" } else { "fullchain.pem" };
    if value == format!("/etc/arg/tls/{expected}") {
        return true;
    }
    let Some(remainder) = value.strip_prefix("/etc/letsencrypt/live/") else {
        return false;
    };
    let Some((name, file)) = remainder.split_once('/') else {
        return false;
    };
    !name.is_empty()
        && name.len() <= 128
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        && !name.starts_with('.')
        && file == expected
}

fn tls_openssl_output(args: &[&str]) -> Result<Vec<u8>, String> {
    let output = std::process::Command::new("openssl")
        .args(args)
        .output()
        .map_err(|error| format!("Could not run OpenSSL to validate the certificate: {error}"))?;
    if !output.status.success() {
        return Err("The certificate or private key is invalid, unreadable, or expired.".into());
    }
    Ok(output.stdout)
}

fn configure_panel_tls(request: &Value) -> Value {
    let failure = |message: String| json!({"ok":false,"error":message});
    let Some(certificate_source) = request["certificate_path"].as_str() else {
        return failure("Certificate path is required.".into());
    };
    let Some(private_key_source) = request["private_key_path"].as_str() else {
        return failure("Private key path is required.".into());
    };
    let Some(server_name) = request["server_name"].as_str().filter(|name| {
        !name.is_empty() && name.len() <= 253 && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b".-:".contains(&byte))
    }) else {
        return failure("Enter the panel domain name or IP address covered by the certificate.".into());
    };
    let Some(port) = request["port"].as_u64().filter(|port| (1024..=65535).contains(port)) else {
        return failure("HTTPS port must be between 1024 and 65535.".into());
    };
    let current_bind = fs::read_to_string("/etc/arg/arg.env").unwrap_or_default().lines()
        .find_map(|line| line.strip_prefix("ARG_TLS_BIND="))
        .and_then(|bind| bind.rsplit(':').next()?.parse::<u16>().ok());
    if current_bind != Some(port as u16)
        && std::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port as u16)).is_err()
    {
        return failure(format!("HTTPS port {port} is already in use. Choose another free port, for example 8443."));
    }
    if !valid_tls_source_path(certificate_source, false)
        || !valid_tls_source_path(private_key_source, true)
    {
        return failure("Use /etc/arg/tls/fullchain.pem and /etc/arg/tls/privkey.pem, or the matching files under /etc/letsencrypt/live/<certificate-name>/.".into());
    }
    let certificate_source = match PathBuf::from(certificate_source).canonicalize() {
        Ok(path) => path,
        Err(_) => return failure("Certificate file was not found. Upload or copy it to the server first.".into()),
    };
    let private_key_source = match PathBuf::from(private_key_source).canonicalize() {
        Ok(path) => path,
        Err(_) => return failure("Private key file was not found. Copy it to the server first.".into()),
    };
    if !certificate_source.starts_with("/etc/arg/tls") && !certificate_source.starts_with("/etc/letsencrypt/archive") {
        return failure("Certificate symlink resolves outside the allowed certificate directories.".into());
    }
    if !private_key_source.starts_with("/etc/arg/tls") && !private_key_source.starts_with("/etc/letsencrypt/archive") {
        return failure("Private-key symlink resolves outside the allowed certificate directories.".into());
    }
    let cert_meta = match fs::metadata(&certificate_source) { Ok(meta) => meta, Err(_) => return failure("Could not read the certificate file.".into()) };
    let key_meta = match fs::metadata(&private_key_source) { Ok(meta) => meta, Err(_) => return failure("Could not read the private-key file.".into()) };
    if !cert_meta.is_file() || !key_meta.is_file() || cert_meta.len() > 1024 * 1024 || key_meta.len() > 128 * 1024 {
        return failure("Certificate or private key is not a regular file or exceeds the allowed size.".into());
    }
    if let Err(message) = tls_openssl_output(&["x509", "-in", certificate_source.to_string_lossy().as_ref(), "-checkend", "0", "-noout"]) {
        return failure(message);
    }
    let hostname_check = if server_name.parse::<std::net::IpAddr>().is_ok() { "-checkip" } else { "-checkhost" };
    if let Err(_) = tls_openssl_output(&["x509", "-in", certificate_source.to_string_lossy().as_ref(), hostname_check, server_name, "-noout"]) {
        return failure("The certificate is not valid for the panel's domain/IP. Use a certificate whose SAN includes this address.".into());
    }
    let cert_public_key = match tls_openssl_output(&["x509", "-in", certificate_source.to_string_lossy().as_ref(), "-pubkey", "-noout"]) {
        Ok(value) => value,
        Err(message) => return failure(message),
    };
    let key_public_key = match tls_openssl_output(&["pkey", "-in", private_key_source.to_string_lossy().as_ref(), "-pubout"]) {
        Ok(value) => value,
        Err(message) => return failure(message),
    };
    if cert_public_key != key_public_key {
        return failure("The certificate does not match the selected private key.".into());
    }

    let tls_dir = PathBuf::from("/etc/arg/tls");
    if let Err(error) = fs::create_dir_all(&tls_dir) { return failure(format!("Could not prepare ARG certificate storage: {error}")); }
    if let Err(error) = fs::set_permissions(&tls_dir, fs::Permissions::from_mode(0o750)) { return failure(format!("Could not secure ARG certificate directory: {error}")); }
    let directory_owner = std::process::Command::new("chown").args(["root:arg", tls_dir.to_string_lossy().as_ref()]).status();
    if !directory_owner.map(|status| status.success()).unwrap_or(false) { return failure("Could not set ARG certificate directory ownership.".into()); }
    let staged_nonce = token();
    let staged_cert = tls_dir.join(format!(".fullchain.{}.{}.tmp", std::process::id(), staged_nonce));
    let staged_key = tls_dir.join(format!(".privkey.{}.{}.tmp", std::process::id(), token()));
    if let Err(error) = fs::copy(&certificate_source, &staged_cert) { return failure(format!("Could not stage certificate: {error}")); }
    if let Err(error) = fs::copy(&private_key_source, &staged_key) {
        let _ = fs::remove_file(&staged_cert);
        return failure(format!("Could not stage private key: {error}"));
    }
    for path in [&staged_cert, &staged_key] {
        if let Err(error) = fs::set_permissions(path, fs::Permissions::from_mode(0o640)) {
            let _ = fs::remove_file(&staged_cert); let _ = fs::remove_file(&staged_key);
            return failure(format!("Could not secure certificate permissions: {error}"));
        }
        let owner = std::process::Command::new("chown").args(["root:arg", path.to_string_lossy().as_ref()]).status();
        if !owner.map(|status| status.success()).unwrap_or(false) {
            let _ = fs::remove_file(&staged_cert); let _ = fs::remove_file(&staged_key);
            return failure("Could not set certificate ownership to root:arg.".into());
        }
    }
    let cert_target = tls_dir.join("fullchain.pem");
    let key_target = tls_dir.join("privkey.pem");
    if let Err(error) = fs::rename(&staged_cert, &cert_target) { let _ = fs::remove_file(&staged_key); return failure(format!("Could not install certificate: {error}")); }
    if let Err(error) = fs::rename(&staged_key, &key_target) { return failure(format!("Could not install private key: {error}")); }

    let env_file = PathBuf::from("/etc/arg/arg.env");
    let existing = match fs::read_to_string(&env_file) {
        Ok(contents) => contents,
        Err(error) => return failure(format!("Could not read ARG environment configuration: {error}")),
    };
    let mut lines: Vec<&str> = existing.lines().filter(|line| {
        !["ARG_TLS_CERT", "ARG_TLS_KEY", "ARG_TLS_BIND"].iter().any(|key| line.starts_with(&format!("{key}=")))
    }).collect();
    lines.extend(["ARG_TLS_CERT=/etc/arg/tls/fullchain.pem", "ARG_TLS_KEY=/etc/arg/tls/privkey.pem"]);
    let tls_bind = format!("ARG_TLS_BIND=0.0.0.0:{port}");
    let mut env_contents = lines.join("\n");
    env_contents.push('\n');
    env_contents.push_str(&tls_bind);
    env_contents.push('\n');
    let env_tmp = PathBuf::from(format!("/etc/arg/.arg.env.tls.{}.{}.tmp", std::process::id(), token()));
    let write_result = fs::OpenOptions::new().write(true).create_new(true).mode(0o640).open(&env_tmp)
        .and_then(|mut file| file.write_all(env_contents.as_bytes()));
    if let Err(error) = write_result { return failure(format!("Could not stage ARG configuration: {error}")); }
    let owner = std::process::Command::new("chown").args(["root:arg", env_tmp.to_string_lossy().as_ref()]).status();
    if !owner.map(|status| status.success()).unwrap_or(false) { let _ = fs::remove_file(&env_tmp); return failure("Could not set ARG configuration ownership.".into()); }
    if let Err(error) = fs::rename(&env_tmp, &env_file) { return failure(format!("Could not activate ARG HTTPS settings: {error}")); }

    if std::process::Command::new("ufw").args(["status"]).output().ok().map(|output| output.status.success() && String::from_utf8_lossy(&output.stdout).contains("Status: active")).unwrap_or(false) {
        let _ = std::process::Command::new("ufw").args(["allow", "80/tcp", "comment", "ARG HTTPS challenge"]).status();
        let _ = std::process::Command::new("ufw").args(["allow", &format!("{port}/tcp"), "comment", "ARG HTTPS panel"]).status();
    }
    let restart = std::process::Command::new("systemd-run").args([
        "--quiet", "--collect", "--unit=arg-tls-restart", "--on-active=2s",
        "--property=Type=oneshot", "/usr/bin/systemctl", "restart", "arg.service",
    ]).output();
    match restart {
        Ok(output) if output.status.success() => json!({"ok":true,"port":port,"message":"TLS settings saved; ARG restart scheduled."}),
        Ok(output) => failure(format!("TLS settings were saved, but restart scheduling failed: {}", String::from_utf8_lossy(&output.stderr).trim())),
        Err(error) => failure(format!("TLS settings were saved, but systemd-run is unavailable: {error}")),
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
        Some("update-logs") => "update-logs",
        Some("update-progress") => "update-progress",
        Some("update-cancel") => "update-cancel",
        Some("update-rollback") => "update-rollback",
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
        Some("nginx-site-traffic") => "nginx-site-traffic",
        Some("nginx-site-repository") => "nginx-site-repository",
        Some("nginx-browse") => "nginx-browse",
        Some("apt-list") => "apt-list",
        Some("apt-run") => "apt-run",
        Some("service-list") => "service-list",
        Some("service-action") => "service-action",
        Some("service-resources") => "service-resources",
        Some("service-limits") => "service-limits",
        Some("docker-status") => "docker-status",
        Some("docker-operation") => "docker-operation",
        Some("file-list") => "file-list",
        Some("file-disk-scan") => "file-disk-scan",
        Some("file-read") => "file-read",
        Some("file-download") => "file-download",
        Some("file-action") => "file-action",
        Some("terminal-root-start") => "terminal-root-start",
        Some("terminal-root-stop") => "terminal-root-stop",
        Some("maintenance-repair") => "maintenance-repair",
        Some("maintenance-reboot") => "maintenance-reboot",
        Some("maintenance-uninstall") => "maintenance-uninstall",
        Some("mysql-status") => "mysql-status",
        Some("mysql-install") => "mysql-install",
        Some("mysql-query") => "mysql-query",
        Some("mysql-catalog") => "mysql-catalog",
        Some("mysql-service") => "mysql-service",
        Some("tls-configure") => "tls-configure",
        Some("s3-secret-write") => "s3-secret-write",
        Some("s3-secret-update-config") => "s3-secret-update-config",
        Some("s3-secret-delete") => "s3-secret-delete",
        Some("s3-list-buckets") => "s3-list-buckets",
        Some("s3-test") => "s3-test",
        Some("s3-create-bucket") => "s3-create-bucket",
        Some("s3-list-objects") => "s3-list-objects",
        Some("s3-scan-usage") => "s3-scan-usage",
        Some("s3-delete-object") => "s3-delete-object",
        Some("s3-upload-object") => "s3-upload-object",
        Some("s3-download-object") => "s3-download-object",
        Some("s3-upload-local-file") => "s3-upload-local-file",
        Some("s3-download-local-file") => "s3-download-local-file",
        Some("s3-mount-start") => "s3-mount-start",
        Some("s3-mount-stop") => "s3-mount-stop",
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
    if action == "tls-configure" {
        return configure_panel_tls(request);
    }
    if action == "maintenance-repair" {
        let branch_output = std::process::Command::new("git")
            .args(["-C", "/opt/arg/source", "branch", "--show-current"])
            .output();
        let fallback_branch = branch_output
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .filter(|branch| ["main", "development"].contains(&branch.as_str()))
            .unwrap_or_else(|| "main".to_owned());
        let branch = request["branch"]
            .as_str()
            .filter(|branch| valid_update_branch(branch))
            .unwrap_or(&fallback_branch);
        return match std::process::Command::new("/usr/local/lib/arg/update-helper")
            .args(["run", "complete", &branch])
            .output()
        {
            Ok(output) if output.status.success() => json!({"ok":true,"scheduled":true,"message":"ARG repair was scheduled with the protected complete-build updater. Follow its progress in the update overlay.","output":String::from_utf8_lossy(&output.stdout)}),
            Ok(output) => json!({"ok":false,"error":String::from_utf8_lossy(&output.stderr).trim(),"output":String::from_utf8_lossy(&output.stdout)}),
            Err(error) => json!({"ok":false,"error":error.to_string()}),
        };
    }
    if action == "maintenance-reboot" {
        return match std::process::Command::new("systemctl")
            .args(["--no-block", "reboot"])
            .spawn()
        {
            Ok(_) => json!({"ok":true,"message":"Server reboot was queued."}),
            Err(error) => json!({"ok":false,"error":error.to_string()}),
        };
    }
    if action == "maintenance-uninstall" {
        return match std::process::Command::new("systemd-run").args([
            "--quiet",
            "--collect",
            "--unit=arg-web-uninstall",
            "--property=Type=oneshot",
            "/usr/local/bin/arg", "delete", "--purge-confirmed",
        ]).output() {
            Ok(output) if output.status.success() => json!({"ok":true,"message":"Full ARG removal was queued. The panel will shut down when removal begins."}),
            Ok(output) => json!({"ok":false,"error":String::from_utf8_lossy(&output.stderr).trim()}),
            Err(error) => json!({"ok":false,"error":format!("Could not schedule ARG removal: {error}")}),
        };
    }
    if action == "docker-status" {
        return docker::execute_docker_status();
    }
    if action == "docker-operation" {
        return docker::execute_docker_operation(request);
    }
    if action.starts_with("file-") {
        return files::execute_privileged_request(action, request);
    }
    if action.starts_with("apt-") {
        return execute_apt_request(action, request);
    }
    if action.starts_with("service-") {
        return execute_service_request(action, request);
    }
    if action.starts_with("nginx-") {
        return nginx::execute_privileged_request(action, request);
    }
    if action.starts_with("s3-") {
        return s3::execute_privileged_request(action, request);
    }
    if action.starts_with("mysql-") {
        return mysql::execute_privileged_request(action, request);
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
        let mut update_args = vec![action.trim_start_matches("update-")];
        if action == "update-check" {
            let branch = request["branch"].as_str().unwrap_or("main");
            if !valid_update_branch(branch) {
                return json!({"ok":false,"error":"Update branch is invalid."});
            }
            update_args.push(branch);
        }
        (
            "/usr/local/lib/arg/update-helper",
            update_args,
        )
    } else {
        (
            "/usr/local/lib/arg/ufw-helper",
            args.iter().map(String::as_str).collect(),
        )
    };
    let update_params = if action == "update-run" {
        let mode = request["mode"].as_str().unwrap_or("complete");
        if !valid_update_build_mode(mode) {
            return json!({"ok":false,"error":"Update build mode is invalid."});
        }
        let branch = request["branch"].as_str().unwrap_or("main");
        if !valid_update_branch(branch) {
            return json!({"ok":false,"error":"Update branch is invalid."});
        }
        Some((mode, branch))
    } else {
        None
    };
    let mut helper_command = std::process::Command::new(helper);
    helper_command.args(helper_args);
    if let Some((mode, branch)) = update_params {
        helper_command.arg(mode);
        helper_command.arg(branch);
    }
    let output = match helper_command.output() {
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
    let result = if stderr.trim().is_empty() {
        stdout
    } else if stdout.trim().is_empty() {
        stderr
    } else {
        format!("{stdout}\n{stderr}")
    };
    Ok((output.status.success(), result))
}
fn execute_apt_request(action: &str, request: &Value) -> Value {
    if action == "apt-list" {
        let mut command = std::process::Command::new("dpkg-query");
        command.args([
            "-W",
            "-f=${binary:Package}\t${Version}\t${db:Status-Abbrev}\t${binary:Summary}\n",
        ]);
        return match command_result(&mut command) {
            Ok((true, output)) => {
                let packages: Vec<Value> = output
                    .lines()
                    .filter_map(|line| {
                        let mut fields = line.splitn(4, '\t');
                        let name = fields.next()?;
                        let version = fields.next()?;
                        let status = fields.next()?;
                        let summary = fields.next().unwrap_or("");
                        status
                            .starts_with("ii")
                            .then(|| json!({"name":name,"version":version,"summary":summary}))
                    })
                    .collect();
                json!({"ok":true,"data":{"packages":packages}})
            }
            Ok((false, output)) => json!({"ok":false,"error":output}),
            Err(error) => json!({"ok":false,"error":error}),
        };
    }
    let Some(operation) = request["operation"].as_str() else {
        return json!({"ok":false,"error":"Missing APT operation"});
    };
    let allowed = [
        "update",
        "upgrade",
        "full-upgrade",
        "autoremove",
        "clean",
        "autoclean",
        "install",
        "remove",
    ];
    if !allowed.contains(&operation) {
        return json!({"ok":false,"error":"Unsupported APT operation"});
    }
    let packages: Vec<&str> = request["packages"]
        .as_array()
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if packages.len() > 25
        || packages.iter().any(|name| !valid_package_name(name))
        || ["install", "remove"].contains(&operation) && packages.is_empty()
    {
        return json!({"ok":false,"error":"Invalid package list"});
    }
    let mut command = std::process::Command::new("apt-get");
    command
        .env("DEBIAN_FRONTEND", "noninteractive")
        .env("APT_LISTCHANGES_FRONTEND", "none")
        .arg("-y");
    match operation {
        "full-upgrade" => {
            command.arg("full-upgrade");
        }
        "autoremove" => {
            command.arg("autoremove");
        }
        _ => {
            command.arg(operation);
        }
    }
    if operation == "install" || operation == "remove" {
        command.arg("--").args(packages);
    }
    match command_result(&mut command) {
        Ok((true, output)) => json!({"ok":true,"stdout":output}),
        Ok((false, output)) => json!({"ok":false,"error":output}),
        Err(error) => json!({"ok":false,"error":error}),
    }
}
fn execute_service_request(action: &str, request: &Value) -> Value {
    if action == "service-list" {
        let mut files = std::process::Command::new("systemctl");
        files.args([
            "list-unit-files",
            "--type=service",
            "--no-legend",
            "--no-pager",
        ]);
        let mut units = std::process::Command::new("systemctl");
        units.args([
            "list-units",
            "--all",
            "--type=service",
            "--no-legend",
            "--no-pager",
        ]);
        let (Ok((true, file_output)), Ok((true, active_output))) =
            (command_result(&mut files), command_result(&mut units))
        else {
            return json!({"ok":false,"error":"Could not list systemd services"});
        };
        let active: HashMap<String, (String, String, String)> = active_output
            .lines()
            .filter_map(|line| {
                let mut f = line.split_whitespace();
                let unit = f.next()?;
                let _load = f.next()?;
                let state = f.next()?;
                let sub = f.next()?;
                let desc = line
                    .splitn(5, char::is_whitespace)
                    .nth(4)
                    .unwrap_or("")
                    .trim()
                    .to_owned();
                Some((unit.to_owned(), (state.to_owned(), sub.to_owned(), desc)))
            })
            .collect();
        let services: Vec<Value> = file_output.lines().filter_map(|line| {
            let mut f=line.split_whitespace(); let unit=f.next()?; let enabled=f.next()?;
            if !valid_service_unit(unit) { return None; }
            let (state,sub,description)=active.get(unit).cloned().unwrap_or_else(||("inactive".into(),"dead".into(),String::new()));
            Some(json!({"unit":unit,"enabled":enabled,"state":state,"sub_state":sub,"description":if description.is_empty(){"System service"}else{description.as_str()}}))
        }).collect();
        return json!({"ok":true,"data":{"services":services}});
    }
    if action == "service-resources" {
        let Some(unit) = request["unit"].as_str().filter(|u| valid_service_unit(u)) else {
            return json!({"ok":false,"error":"Invalid service unit"});
        };
        let mut command = std::process::Command::new("systemctl");
        command.args(["show", "--no-pager", "--property=CPUUsageNSec,MemoryCurrent,CPUQuotaPerSecUSec,MemoryMax", unit]);
        return match command_result(&mut command) {
            Ok((true, output)) => {
                let values: HashMap<String, String> = output.lines().filter_map(|line| line.split_once('=')).map(|(key,value)|(key.to_owned(),value.to_owned())).collect();
                json!({"ok":true,"data":{"unit":unit,"cpu_usage_ns":values.get("CPUUsageNSec").and_then(|v|v.parse::<u64>().ok()),"memory_current_bytes":values.get("MemoryCurrent").and_then(|v|v.parse::<u64>().ok()),"cpu_quota_per_second_usec":values.get("CPUQuotaPerSecUSec").and_then(|v|v.parse::<u64>().ok()),"memory_max_bytes":values.get("MemoryMax").and_then(|v|v.parse::<u64>().ok())}})
            },
            Ok((false, output)) => json!({"ok":false,"error":output}),
            Err(error) => json!({"ok":false,"error":error}),
        };
    }
    if action == "service-limits" {
        let Some(unit) = request["unit"].as_str().filter(|u| valid_service_unit(u)) else {
            return json!({"ok":false,"error":"Invalid service unit"});
        };
        if protected_service_mutation(unit, "set-property") {
            return json!({"ok":false,"error":format!("Resource limits cannot be applied to protected service {unit}")});
        }
        if shared_web_service(unit) {
            return json!({"ok":false,"error":format!("{unit} is shared by multiple websites. Attach a dedicated application service to this site instead.")});
        }
        let cpu = request["cpu_quota_percent"].as_f64();
        if cpu.is_some_and(|value| !value.is_finite() || !(1.0..=10000.0).contains(&value)) {
            return json!({"ok":false,"error":"CPU quota must be between 1 and 10000 percent"});
        }
        let memory = request["memory_max_bytes"].as_u64();
        if memory.is_some_and(|value| !(16 * 1024 * 1024..=1_099_511_627_776).contains(&value)) {
            return json!({"ok":false,"error":"Memory limit must be between 16 MiB and 1 TiB"});
        }
        let [cpu_property, memory_property] = service_resource_properties(cpu, memory);
        let mut command = std::process::Command::new("systemctl");
        command.args(["set-property", unit, &cpu_property, &memory_property]);
        return match command_result(&mut command) {
            Ok((true, output)) => json!({"ok":true,"stdout":output}),
            Ok((false, output)) => json!({"ok":false,"error":output}),
            Err(error) => json!({"ok":false,"error":error}),
        };
    }
    let Some(operation) = request["operation"].as_str() else {
        return json!({"ok":false,"error":"Missing systemd action"});
    };
    let allowed = [
        "start",
        "stop",
        "restart",
        "try-restart",
        "reload",
        "reload-or-restart",
        "enable",
        "disable",
        "enable-now",
        "disable-now",
        "reenable",
        "mask",
        "unmask",
        "reset-failed",
        "status",
        "logs",
        "daemon-reload",
    ];
    if !allowed.contains(&operation) {
        return json!({"ok":false,"error":"Unsupported systemd action"});
    }
    if operation == "daemon-reload" {
        let mut command = std::process::Command::new("systemctl");
        command.arg("daemon-reload");
        return match command_result(&mut command) {
            Ok((true, output)) => json!({"ok":true,"stdout":output}),
            Ok((false, output)) => json!({"ok":false,"error":output}),
            Err(error) => json!({"ok":false,"error":error}),
        };
    }
    let Some(unit) = request["unit"].as_str().filter(|u| valid_service_unit(u)) else {
        return json!({"ok":false,"error":"Invalid service unit"});
    };
    if protected_service_mutation(unit, operation) {
        return json!({"ok":false,"error":format!("Refusing to {operation} protected service {unit}. This could disconnect administrators or stop ARG itself; use the service's own safe maintenance flow.")});
    }
    if operation == "logs" {
        let mut command = std::process::Command::new("journalctl");
        command.args([
            "--no-pager",
            "--output=short-iso",
            "--lines=100",
            "--unit",
            unit,
        ]);
        return match command_result(&mut command) {
            Ok((true, output)) => json!({"ok":true,"stdout":output}),
            Ok((false, output)) => json!({"ok":false,"error":output}),
            Err(error) => json!({"ok":false,"error":error}),
        };
    }
    let mut command = std::process::Command::new("systemctl");
    if operation == "status" {
        command.args(["--no-pager", "--full", "status", unit]);
        return match command_result(&mut command) {
            Ok((_, output)) => json!({"ok":true,"stdout":output}),
            Err(error) => json!({"ok":false,"error":error}),
        };
    }
    match operation {
        "enable-now" => {
            command.arg("enable").arg("--now");
        }
        "disable-now" => {
            command.arg("disable").arg("--now");
        }
        _ => {
            command.arg(operation);
        }
    }
    command.arg(unit);
    match command_result(&mut command) {
        Ok((true, output)) => json!({"ok":true,"stdout":output}),
        Ok((false, output)) => json!({"ok":false,"error":output}),
        Err(error) => json!({"ok":false,"error":error}),
    }
}

fn service_resource_properties(cpu_percent: Option<f64>, memory_bytes: Option<u64>) -> [String; 2] {
    [
        cpu_percent
            .map(|value| format!("CPUQuota={value:.2}%"))
            .unwrap_or_else(|| "CPUQuota=infinity".into()),
        // systemd size values are bytes when no suffix is present; appending `B`
        // is rejected by some versions with “The string did not match the expected pattern.”
        memory_bytes
            .map(|value| format!("MemoryMax={value}"))
            .unwrap_or_else(|| "MemoryMax=infinity".into()),
    ]
}

async fn serve_privileged_connection(mut stream: UnixStream) {
    if !privileged_peer_allowed(&stream) {
        return;
    }
    let mut line = String::new();
    let mut limited = (&mut stream).take((PRIVILEGED_REQUEST_MAX_BYTES + 1) as u64);
    let read_result = AsyncBufReader::new(&mut limited).read_line(&mut line).await;
    drop(limited);
    let response = match read_result {
        Ok(size) if size > 0 && size <= PRIVILEGED_REQUEST_MAX_BYTES => {
            match serde_json::from_str::<Value>(&line) {
                Ok(request) => match PRIVILEGED_REQUESTS.clone().try_acquire_owned() {
                    Ok(permit) => tokio::task::spawn_blocking(move || {
                        let _permit = permit;
                        execute_privileged_request(&request)
                    })
                    .await
                    .unwrap_or_else(|_| json!({"ok":false,"error":"Privileged task failed"})),
                    Err(_) => json!({"ok":false,"error":"Privileged helper is busy; retry shortly"}),
                },
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

fn backup_database(source_path: &std::path::Path, destination_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    if source_path == destination_path || !source_path.is_file() {
        return Err("Database backup paths are invalid".into());
    }
    let source = Connection::open_with_flags(source_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut destination = Connection::open(destination_path)?;
    rusqlite::backup::Backup::new(&source, &mut destination)?
        .run_to_completion(100, std::time::Duration::from_millis(20), None)?;
    #[cfg(unix)]
    fs::set_permissions(destination_path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn backup_database_cli() -> Result<(), Box<dyn std::error::Error>> {
    if unsafe { libc::geteuid() } != 0 {
        return Err("Database backups must be created by root".into());
    }
    let arguments: Vec<_> = env::args_os().skip(2).collect();
    if arguments.len() != 2 {
        return Err("Usage: arg database-backup <source> <destination>".into());
    }
    backup_database(
        std::path::Path::new(&arguments[0]),
        std::path::Path::new(&arguments[1]),
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::args().nth(1).as_deref() == Some("terminal-shell") {
        return run_terminal_shell();
    }
    if env::args().nth(1).as_deref() == Some("privileged-helper") {
        return run_privileged_helper().await;
    }
    if env::args().nth(1).as_deref() == Some("database-backup") {
        return backup_database_cli();
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
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY,username TEXT NOT NULL UNIQUE,password_hash TEXT NOT NULL,role TEXT NOT NULL,enabled INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS sessions (token_hash TEXT PRIMARY KEY,user_id INTEGER NOT NULL REFERENCES users(id),expires_at INTEGER NOT NULL,created_at INTEGER,last_seen INTEGER,remote_ip TEXT,user_agent TEXT); CREATE TABLE IF NOT EXISTS audit (id INTEGER PRIMARY KEY,at INTEGER NOT NULL,actor TEXT NOT NULL,action TEXT NOT NULL,detail TEXT NOT NULL); CREATE TABLE IF NOT EXISTS auth_failures (username TEXT PRIMARY KEY,count INTEGER NOT NULL,window_start INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS metric_samples (at INTEGER PRIMARY KEY,cpu REAL NOT NULL,memory REAL NOT NULL,disk REAL NOT NULL,rx_rate REAL NOT NULL,tx_rate REAL NOT NULL); CREATE TABLE IF NOT EXISTS network_traffic_hourly (hour_start INTEGER NOT NULL,interface TEXT NOT NULL,rx_bytes INTEGER NOT NULL,tx_bytes INTEGER NOT NULL,eligible_for_total INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(hour_start,interface)); CREATE INDEX IF NOT EXISTS network_traffic_hourly_time ON network_traffic_hourly(hour_start); CREATE TABLE IF NOT EXISTS managed_firewall_rules (id INTEGER PRIMARY KEY,port INTEGER NOT NULL,protocol TEXT NOT NULL,policy TEXT NOT NULL,enabled INTEGER NOT NULL,UNIQUE(port,protocol)); CREATE TABLE IF NOT EXISTS nginx_site_metadata (name TEXT PRIMARY KEY,description TEXT NOT NULL DEFAULT ''); CREATE TABLE IF NOT EXISTS nginx_site_config_state (name TEXT PRIMARY KEY,content TEXT NOT NULL,settings_json TEXT,content_hash TEXT NOT NULL,updated_at INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS nginx_site_runtime (site_name TEXT PRIMARY KEY,unit TEXT NOT NULL,cpu_quota_percent REAL,memory_max_bytes INTEGER,last_cpu_usage_ns INTEGER,last_sampled_at INTEGER,updated_at INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS fast_actions (id INTEGER PRIMARY KEY,title TEXT NOT NULL,operation TEXT NOT NULL,target TEXT NOT NULL DEFAULT '',site_name TEXT,include_access_log INTEGER NOT NULL DEFAULT 0,include_error_log INTEGER NOT NULL DEFAULT 0,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS nginx_site_actions (id INTEGER PRIMARY KEY,site_name TEXT NOT NULL,title TEXT NOT NULL,icon TEXT NOT NULL,operation TEXT NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,UNIQUE(site_name,title));")?;
    s3::migrate(&db)?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS root_terminal_tickets (token_hash TEXT PRIMARY KEY,user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,session_hash TEXT NOT NULL,expires_at INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS root_terminal_ticket_expiry ON root_terminal_tickets(expires_at);")?;
    db.execute(
        "DELETE FROM root_terminal_tickets WHERE expires_at<=?1",
        [now()],
    )?;
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
    let app = routes::router(state);
    let tls_cert = env::var("ARG_TLS_CERT").ok();
    let tls_key = env::var("ARG_TLS_KEY").ok();
    let bind = env::var("ARG_BIND").unwrap_or_else(|_| "127.0.0.1:8282".into());
    let addr: SocketAddr = bind.parse()?;
    let http_addr = http_listener_addr(addr, tls_cert.is_some() && tls_key.is_some());
    println!("ARG listening at http://{http_addr}");
    let listener = tokio::net::TcpListener::bind(http_addr).await?;
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
                    request.headers_mut().insert(
                        axum::http::header::HeaderName::from_static("x-arg-transport"),
                        axum::http::HeaderValue::from_static("https"),
                    );
                    next.run(request).await
                },
            ));
            let https_app = https_app.into_make_service_with_connect_info::<SocketAddr>();
            let https_server = axum_server::bind_rustls(tls_bind, tls).serve(https_app);
            let http_app = app
                .layer(axum::middleware::from_fn(clear_trusted_transport_marker))
                .into_make_service_with_connect_info::<SocketAddr>();
            tokio::select! {
                result = axum::serve(listener, http_app) => result?,
                result = https_server => result?,
            }
        }
        (None, None) => {
            let http_app = app
                .layer(axum::middleware::from_fn(clear_trusted_transport_marker))
                .into_make_service_with_connect_info::<SocketAddr>();
            axum::serve(listener, http_app).await?;
        }
        _ => return Err("Both ARG_TLS_CERT and ARG_TLS_KEY must be set".into()),
    }
    Ok(())
}
async fn state_handler(state: State<AppState>, headers: HeaderMap) -> ApiResult {
    self::state(state, headers).await
}

async fn clear_trusted_transport_marker(
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    request.headers_mut().remove("x-arg-transport");
    next.run(request).await
}

#[cfg(test)]
mod update_progress_tests {
    use super::{
        backup_database, default_update_build_mode, firewall_rule_validation_error,
        update_progress_payload, valid_update_build_mode, valid_tls_source_path, Connection,
        FirewallRuleInput,
    };

    #[test]
    fn update_build_mode_only_accepts_supported_choices() {
        assert!(valid_update_build_mode("fast"));
        assert!(valid_update_build_mode("complete"));
        assert!(!valid_update_build_mode("clean"));
        assert!(!valid_update_build_mode("../../tmp"));
        assert_eq!(default_update_build_mode(), "complete");
    }

    #[test]
    fn cancellation_endpoint_remains_available_while_update_lock_is_active() {
        assert!(super::update_guard_exempt("/api/update/cancel"));
        assert!(super::update_guard_exempt("/api/update/progress"));
        assert!(!super::update_guard_exempt("/api/update/rollback"));
        assert!(!super::update_guard_exempt("/api/nginx/restart"));
    }

    #[test]
    fn firewall_protects_effective_ssh_and_arg_ports() {
        let ports = super::protected_firewall_ports_from(
            Some("0.0.0.0:9282"),
            Some("[::]:9443"),
            Some("port 2207\nlistenaddress 0.0.0.0:2208\nport 22\n"),
        );
        assert!(ports.contains(&22));
        assert!(ports.contains(&2207));
        assert!(ports.contains(&2208));
        assert!(ports.contains(&9282));
        assert!(ports.contains(&9443));
        assert!(!ports.contains(&80));
    }

    #[test]
    fn critical_services_cannot_be_stopped_or_disabled_from_panel() {
        assert!(super::shared_web_service("nginx.service"));
        assert!(super::shared_web_service("php8.3-fpm.service"));
        assert!(!super::shared_web_service("customer-app.service"));
        for unit in [
            "arg.service",
            "ssh.service",
            "sshd.service",
            "NetworkManager.service",
            "systemd-networkd.service",
        ] {
            assert!(super::protected_service_mutation(unit, "stop"));
            assert!(super::protected_service_mutation(unit, "disable-now"));
            assert!(super::protected_service_mutation(unit, "set-property"));
            assert!(!super::protected_service_mutation(unit, "status"));
        }
        assert!(!super::protected_service_mutation(
            "nginx.service",
            "restart"
        ));
    }

    #[test]
    fn systemd_resource_limits_use_supported_property_values() {
        assert_eq!(
            super::service_resource_properties(Some(75.5), Some(536_870_912)),
            ["CPUQuota=75.50%", "MemoryMax=536870912"]
        );
        assert_eq!(
            super::service_resource_properties(None, None),
            ["CPUQuota=infinity", "MemoryMax=infinity"]
        );
    }

    #[test]
    fn root_terminal_ticket_is_bound_to_user_and_consumed_once() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE root_terminal_tickets(token_hash TEXT PRIMARY KEY,user_id INTEGER NOT NULL,session_hash TEXT NOT NULL,expires_at INTEGER NOT NULL);").unwrap();
        let ticket = "one-time-ticket";
        db.execute("INSERT INTO root_terminal_tickets(token_hash,user_id,session_hash,expires_at) VALUES (?1,7,'session-a',?2)", rusqlite::params![super::session_hash(ticket), super::now() + 60]).unwrap();
        assert!(!super::consume_root_terminal_ticket(
            &db,
            ticket,
            8,
            "session-a"
        ));
        assert!(!super::consume_root_terminal_ticket(
            &db,
            ticket,
            7,
            "session-b"
        ));
        assert!(super::consume_root_terminal_ticket(
            &db,
            ticket,
            7,
            "session-a"
        ));
        assert!(!super::consume_root_terminal_ticket(
            &db,
            ticket,
            7,
            "session-a"
        ));
    }

    #[test]
    fn public_http_listener_becomes_loopback_when_tls_is_configured() {
        let bind = "0.0.0.0:8282".parse().unwrap();
        assert_eq!(
            super::http_listener_addr(bind, true).ip().to_string(),
            "127.0.0.1"
        );
        assert_eq!(super::http_listener_addr(bind, true).port(), 8282);
        assert_eq!(super::http_listener_addr(bind, false), bind);
    }

    #[test]
    fn forwarded_https_is_trusted_only_from_loopback_or_tls_listener() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-forwarded-proto",
            axum::http::HeaderValue::from_static("https"),
        );
        let public_ip = "203.0.113.10".parse().unwrap();
        let loopback = "127.0.0.1".parse().unwrap();
        assert!(!super::secure_transport(&headers, public_ip));
        assert!(super::secure_transport(&headers, loopback));
        headers.insert(
            "x-arg-transport",
            axum::http::HeaderValue::from_static("https"),
        );
        assert!(super::secure_transport(&headers, public_ip));
    }

    #[test]
    fn docker_inputs_reject_shell_metacharacters_and_invalid_ports() {
        assert!(super::docker::valid_docker_object("web_app-1"));
        assert!(!super::docker::valid_docker_object("../etc"));
        assert!(!super::docker::valid_docker_object("web;id"));
        assert!(super::docker::valid_docker_image(
            "registry.example/app:v1.2"
        ));
        assert!(!super::docker::valid_docker_image("app; touch /tmp/pwned"));
        assert!(super::docker::valid_docker_port("8080:80/tcp"));
        assert!(super::docker::valid_docker_port("5353:53/udp"));
        assert!(!super::docker::valid_docker_port("65536:80/tcp"));
        assert!(!super::docker::valid_docker_port("80:80/icmp"));
    }

    #[test]
    fn nginx_file_content_supports_current_and_legacy_manager_payloads() {
        let response =
            serde_json::json!({"site":{"name":"example.conf","content":"server { listen 80; }"}});
        assert_eq!(
            super::nginx::nginx_site_content(&response).unwrap(),
            "server { listen 80; }"
        );
        let legacy = serde_json::json!({"name":"example.conf","content":"legacy config"});
        assert_eq!(
            super::nginx::nginx_site_content(&legacy).unwrap(),
            "legacy config"
        );
        let malformed = serde_json::json!({"site":{"name":"example.conf"}});
        assert!(super::nginx::nginx_site_content(&malformed).is_err());
    }

    #[test]
    fn root_terminal_accepts_only_numeric_pty_device_paths() {
        assert!(super::valid_terminal_pty_path("/dev/pts/17"));
        assert!(!super::valid_terminal_pty_path("/dev/pts/../../etc/passwd"));
        assert!(!super::valid_terminal_pty_path("/dev/tty0"));
    }

    #[test]
    fn panel_timezone_accepts_iana_names_and_rejects_path_traversal() {
        assert!(super::valid_panel_timezone("UTC"));
        assert!(super::valid_panel_timezone("Asia/Tehran"));
        assert!(!super::valid_panel_timezone("../etc/passwd"));
        assert!(!super::valid_panel_timezone("/etc/localtime"));
    }

    #[test]
    fn active_job_locks_only_while_authoritative_state_is_running() {
        let running = update_progress_payload(
            Some(
                r#"{"job_id":"job-1","state":"running","phase":"building","started_at":100,"message":""}"#,
            ),
            Some(100),
            "instance-a",
        );
        assert_eq!(running["updating"], true);
        assert_eq!(running["phase"], "building");

        let succeeded = update_progress_payload(
            Some(
                r#"{"job_id":"job-1","state":"succeeded","phase":"complete","started_at":100,"message":"done"}"#,
            ),
            Some(100),
            "instance-b",
        );
        assert_eq!(succeeded["updating"], false);
        assert_eq!(succeeded["state"], "succeeded");
    }

    #[test]
    fn failed_and_orphaned_jobs_are_recoverable_not_locked() {
        let failed = update_progress_payload(
            Some(
                r#"{"job_id":"job-2","state":"failed","phase":"failed","started_at":100,"message":"build failed"}"#,
            ),
            None,
            "instance-a",
        );
        assert_eq!(failed["updating"], false);
        assert_eq!(failed["state"], "failed");

        let orphaned = update_progress_payload(
            Some(
                r#"{"job_id":"job-3","state":"running","phase":"building","started_at":100,"message":""}"#,
            ),
            None,
            "instance-a",
        );
        assert_eq!(orphaned["updating"], false);
        assert_eq!(orphaned["state"], "failed");
        assert!(orphaned["message"]
            .as_str()
            .unwrap()
            .contains("final result"));
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
        let row: (String, i64, Option<i64>) = db
            .query_row(
                "SELECT token_hash,expires_at,created_at FROM sessions",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, ("hash".to_owned(), 9999999999, None));
    }

    #[test]
    fn updater_database_backup_preserves_a_consistent_sqlite_snapshot() {
        let directory = std::env::temp_dir().join(format!(
            "arg-db-backup-{}-{}",
            std::process::id(),
            super::now()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let source_path = directory.join("source.db");
        let destination_path = directory.join("backup.db");
        {
            let source = Connection::open(&source_path).unwrap();
            source.execute_batch("CREATE TABLE state (value TEXT NOT NULL); INSERT INTO state VALUES ('before-update');").unwrap();
        }
        backup_database(&source_path, &destination_path).unwrap();
        let backup = Connection::open(&destination_path).unwrap();
        let value: String = backup.query_row("SELECT value FROM state", [], |row| row.get(0)).unwrap();
        assert_eq!(value, "before-update");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn network_deltas_split_at_utc_hour_boundaries_without_losing_bytes() {
        let buckets = super::metrics::split_traffic_delta(3598, 3602, 101, 57);
        assert_eq!(buckets, vec![(0, 50, 28), (3600, 51, 29)]);
        assert_eq!(buckets.iter().map(|bucket| bucket.1).sum::<u64>(), 101);
        assert_eq!(buckets.iter().map(|bucket| bucket.2).sum::<u64>(), 57);
        assert!(super::metrics::split_traffic_delta(10, 10, 500, 200).is_empty());
    }

    #[test]
    fn daily_traffic_buckets_keep_known_rx_and_tx_totals() {
        let rx = 1536u64 * 1024 * 1024;
        let tx = 512u64 * 1024 * 1024;
        let buckets = super::metrics::split_traffic_delta(0, 24 * 3600, rx, tx);
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

    #[test]
    fn updater_accepts_only_stable_or_development_branches() {
        assert!(super::valid_update_branch("main"));
        assert!(super::valid_update_branch("development"));
        for invalid in ["", "master", "feature/test", "main;id", "../main"] {
            assert!(!super::valid_update_branch(invalid));
        }
        assert_eq!(super::default_update_branch(), "main");
    }

    #[test]
    fn tls_certificate_paths_are_restricted_to_supported_system_locations() {
        assert!(valid_tls_source_path("/etc/arg/tls/fullchain.pem", false));
        assert!(valid_tls_source_path("/etc/arg/tls/privkey.pem", true));
        assert!(valid_tls_source_path("/etc/letsencrypt/live/example.com/fullchain.pem", false));
        assert!(valid_tls_source_path("/etc/letsencrypt/live/arg/privkey.pem", true));
        assert!(!valid_tls_source_path("/etc/letsencrypt/live/example.com/privkey.pem", false));
        assert!(!valid_tls_source_path("/etc/letsencrypt/live/../arg/privkey.pem", true));
        assert!(!valid_tls_source_path("/home/admin/cert.pem", false));
        assert!(!valid_tls_source_path("/etc/arg/tls/privkey.pem", false));
    }
}
