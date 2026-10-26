use super::*;
use axum::http::header::AUTHORIZATION;
use axum::{body::Body, extract::OriginalUri};
use std::path::Path as FsPath;

const PROTOCOL_VERSION: u32 = 2;
const TOKEN_DIR: &str = "node-secrets";

#[derive(Deserialize)]
pub(super) struct PairCodeRequest {
    #[serde(default)]
    replace_existing: bool,
}

#[derive(Deserialize)]
pub(super) struct AddNodeRequest {
    name: String,
    endpoint: String,
    pairing_code: String,
}

#[derive(Serialize)]
struct NodeRecord {
    id: i64,
    name: String,
    endpoint: String,
    node_id: String,
    protocol_version: u32,
    last_seen: Option<i64>,
    created_at: i64,
    workspace_ready: bool,
}

#[derive(Deserialize)]
pub(super) struct PairAcceptRequest {
    protocol_version: u32,
    controller_id: String,
    controller_name: String,
    pairing_code: String,
}

#[derive(Deserialize)]
pub(super) struct ServiceActionRequest {
    action: String,
    unit: String,
    #[serde(default)]
    confirmed: bool,
}

fn token_hash(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn require_secure_transport(headers: &HeaderMap) -> Result<(), (StatusCode, Json<Value>)> {
    if headers
        .get("x-arg-transport")
        .and_then(|value| value.to_str().ok())
        == Some("https")
    {
        Ok(())
    } else {
        Err(error(
            StatusCode::UPGRADE_REQUIRED,
            "ارتباط Node فقط از طریق HTTPS پذیرفته می‌شود.",
        ))
    }
}

fn validate_endpoint(value: &str) -> Result<url::Url, String> {
    if value.len() > 512 || value.chars().any(char::is_control) {
        return Err("آدرس Node معتبر نیست.".into());
    }
    let parsed = url::Url::parse(value.trim_end_matches('/'))
        .map_err(|_| "آدرس Node باید یک URL معتبر HTTPS باشد.")?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path() != "/"
    {
        return Err(
            "برای اتصال Node فقط URL پایهٔ HTTPS مجاز است؛ مسیر، Query و اطلاعات ورود را وارد نکن."
                .into(),
        );
    }
    Ok(parsed)
}

fn valid_workspace_api_path(path: &str) -> bool {
    let path = path.trim_start_matches('/');
    !path.is_empty()
        && path.len() <= 2048
        && !path.contains(['\\', '%'])
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && !["login", "logout", "setup", "nodes", "federation"]
            .iter()
            .any(|blocked| path == *blocked || path.starts_with(&format!("{blocked}/")))
}

fn http_client() -> Result<reqwest::Client, (StatusCode, Json<Value>)> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("ARG-Node/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ساخت اتصال امن Node ناموفق بود.",
            )
        })
}

fn workspace_http_client() -> Result<reqwest::Client, (StatusCode, Json<Value>)> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .connect_timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("ARG-Workspace/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ساخت تونل امن فضای کار ناموفق بود.",
            )
        })
}

fn token_path(
    db: &Connection,
    node_id: i64,
) -> Result<std::path::PathBuf, (StatusCode, Json<Value>)> {
    let db_path: String = db
        .query_row("PRAGMA database_list", [], |row| row.get(2))
        .map_err(db_error)?;
    let parent = FsPath::new(&db_path)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(FsPath::new("."));
    Ok(parent.join(TOKEN_DIR).join(format!("{node_id}.token")))
}

fn workspace_token_path(
    db: &Connection,
    node_id: i64,
) -> Result<std::path::PathBuf, (StatusCode, Json<Value>)> {
    let path = token_path(db, node_id)?;
    Ok(path.with_extension("workspace"))
}

fn write_node_token(path: &FsPath, value: &str) -> Result<(), (StatusCode, Json<Value>)> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let parent = path.parent().ok_or_else(|| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "مسیر امن کلید Node موجود نیست.",
        )
    })?;
    fs::create_dir_all(parent).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ساخت محل کلید Node ناموفق بود.",
        )
    })?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "تنظیم مجوز محل کلید Node ناموفق بود.",
        )
    })?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        token()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ساخت فایل موقت کلید Node ناموفق بود.",
            )
        })?;
    use std::io::Write;
    if file.write_all(value.as_bytes()).is_err() || file.sync_all().is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ذخیرهٔ کلید Node ناموفق بود.",
        ));
    }
    fs::rename(&temporary, path).map_err(|_| {
        let _ = fs::remove_file(&temporary);
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ثبت فایل کلید Node ناموفق بود.",
        )
    })?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "تنظیم مجوز فایل کلید Node ناموفق بود.",
        )
    })?;
    Ok(())
}

fn read_node_token(path: &FsPath) -> Result<String, (StatusCode, Json<Value>)> {
    let metadata = fs::symlink_metadata(path).map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "کلید Node پیدا نشد؛ اتصال را دوباره جفت کن.",
        )
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "فایل کلید Node ایمن نیست.",
        ));
    }
    let token = fs::read_to_string(path).map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "خواندن کلید Node ناموفق بود.",
        )
    })?;
    let token = token.trim();
    if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "کلید Node نامعتبر است؛ اتصال را دوباره جفت کن.",
        ));
    }
    Ok(token.to_owned())
}

pub(super) async fn list_nodes(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let mut statement = db.prepare("SELECT id,name,endpoint,node_id,protocol_version,last_seen,created_at FROM federated_nodes ORDER BY name COLLATE NOCASE").map_err(db_error)?;
    let nodes = statement
        .query_map([], |row| {
            Ok(NodeRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                endpoint: row.get(2)?,
                node_id: row.get(3)?,
                protocol_version: row.get::<_, i64>(4)? as u32,
                last_seen: row.get(5)?,
                created_at: row.get(6)?,
                workspace_ready: workspace_token_path(&db, row.get(0)?)
                    .ok()
                    .and_then(|path| fs::symlink_metadata(path).ok())
                    .is_some_and(|metadata| {
                        metadata.is_file() && !metadata.file_type().is_symlink()
                    }),
            })
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    let controllers: Vec<Value> = {
        let mut query = db.prepare("SELECT id,controller_name,created_at,last_seen FROM federation_controllers WHERE enabled=1 ORDER BY created_at DESC LIMIT 100").map_err(db_error)?;
        let rows = query.query_map([], |row| Ok(json!({"id":row.get::<_,i64>(0)?,"name":row.get::<_,String>(1)?,"created_at":row.get::<_,i64>(2)?,"last_seen":row.get::<_,Option<i64>>(3)?}))).map_err(db_error)?;
        rows.filter_map(Result::ok).collect()
    };
    Ok(Json(
        json!({"protocol":"arg-node/2","nodes":nodes,"controllers":controllers}),
    ))
}

pub(super) async fn create_pairing_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PairCodeRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let active: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM federation_pairing_codes WHERE expires_at>?1",
            [now()],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if active > 0 && !input.replace_existing {
        return Err(error(
            StatusCode::CONFLICT,
            "یک کد اتصال هنوز فعال است. آن را باطل کن و کد تازه بساز.",
        ));
    }
    db.execute("DELETE FROM federation_pairing_codes", [])
        .map_err(db_error)?;
    let pairing_code = token();
    let expires_at = now() + 300;
    db.execute(
        "INSERT INTO federation_pairing_codes(code_hash,expires_at,created_by) VALUES (?1,?2,?3)",
        params![token_hash(&pairing_code), expires_at, &actor.username],
    )
    .map_err(db_error)?;
    audit(
        &db,
        &actor.username,
        "federation.pairing.create",
        "protocol=arg-node/2; expires=300s",
    );
    Ok(Json(
        json!({"pairing_code":pairing_code,"protocol":"arg-node/2","expires_at":expires_at,"warning":"این کد را فقط در پنل مورداعتماد وارد کن؛ پس از 5 دقیقه یا اولین استفاده منقضی می‌شود."}),
    ))
}

pub(super) async fn add_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AddNodeRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    if input.name.trim().is_empty()
        || input.name.trim().chars().count() > 80
        || input.pairing_code.len() != 64
        || !input
            .pairing_code
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام Node یا کد یک‌بارمصرف معتبر نیست.",
        ));
    }
    let endpoint = validate_endpoint(&input.endpoint)
        .map_err(|message| error(StatusCode::BAD_REQUEST, &message))?;
    let controller_id = token()[..32].to_owned();
    let client = http_client()?;
    let response = client.post(endpoint.join("api/federation/pair").map_err(|_| error(StatusCode::BAD_REQUEST, "آدرس اتصال معتبر نیست."))?)
        .header("X-ARG-Transport", "https")
        .header("X-ARG-Protocol", PROTOCOL_VERSION.to_string())
        .json(&json!({"protocol_version":PROTOCOL_VERSION,"controller_id":controller_id,"controller_name":input.name.trim(),"pairing_code":input.pairing_code}))
        .send().await.map_err(|_| error(StatusCode::BAD_GATEWAY, "ارتباط HTTPS با Node برقرار نشد؛ URL، گواهی TLS و دسترسی شبکه را بررسی کن."))?;
    if !response.status().is_success() {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            "Node کد اتصال را نپذیرفت؛ کد را بررسی کن و مطمئن شو هر دو پنل به نسخهٔ پشتیبان فضای کار راه‌دور به‌روز هستند.",
        ));
    }
    let payload: Value = response
        .json()
        .await
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "پاسخ پروتکل Node نامعتبر است."))?;
    if payload["protocol_version"].as_u64() != Some(PROTOCOL_VERSION as u64)
        || payload["node_token"].as_str().is_none()
        || payload["workspace_token"].as_str().is_none()
        || payload["node_id"].as_str().is_none()
        || payload["controller_id"].as_str() != Some(controller_id.as_str())
    {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            "نسخه یا شناسهٔ پروتکل Node سازگار نیست.",
        ));
    }
    let remote_token = payload["node_token"].as_str().unwrap_or_default();
    let workspace_token = payload["workspace_token"].as_str().unwrap_or_default();
    if remote_token.len() != 64 || !remote_token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(error(StatusCode::BAD_GATEWAY, "Node کلید معتبر صادر نکرد."));
    }
    if workspace_token.len() != 64 || !workspace_token.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            "Node کلید فضای کار معتبر صادر نکرد.",
        ));
    }
    let db = state.lock().unwrap();
    let endpoint_text = endpoint.as_str().trim_end_matches('/').to_owned();
    db.execute("INSERT INTO federated_nodes(name,endpoint,node_id,controller_id,protocol_version,created_at,last_seen) VALUES (?1,?2,?3,?4,?5,?6,?6)", params![input.name.trim(), endpoint_text, payload["node_id"].as_str().unwrap_or_default(), controller_id, PROTOCOL_VERSION, now()]).map_err(db_error)?;
    let id = db.last_insert_rowid();
    let secret_path = token_path(&db, id)?;
    if let Err(err) = write_node_token(&secret_path, remote_token) {
        let _ = db.execute("DELETE FROM federated_nodes WHERE id=?1", [id]);
        return Err(err);
    }
    let workspace_path = workspace_token_path(&db, id)?;
    if let Err(err) = write_node_token(&workspace_path, workspace_token) {
        let _ = fs::remove_file(&secret_path);
        let _ = db.execute("DELETE FROM federated_nodes WHERE id=?1", [id]);
        return Err(err);
    }
    audit(
        &db,
        &actor.username,
        "federation.node.add",
        &format!(
            "node_id={} name={}",
            payload["node_id"].as_str().unwrap_or(""),
            input.name.trim()
        ),
    );
    Ok(Json(
        json!({"id":id,"name":input.name.trim(),"node_id":payload["node_id"],"protocol":"arg-node/2","capabilities":payload["capabilities"],"message":"Node با دسترسی امن فضای کار جفت شد."}),
    ))
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .filter(|token| token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .map(str::to_owned)
}

fn authorize_controller(
    db: &Connection,
    headers: &HeaderMap,
) -> Result<(i64, String), (StatusCode, Json<Value>)> {
    require_secure_transport(headers)?;
    let raw = bearer(headers)
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "ARG Node authentication required"))?;
    let hash = token_hash(&raw);
    let peer = db.query_row("SELECT id,controller_name FROM federation_controllers WHERE token_hash=?1 AND enabled=1", [&hash], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?))).optional().map_err(db_error)?
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "ARG Node credential is invalid or revoked"))?;
    db.execute(
        "UPDATE federation_controllers SET last_seen=?1 WHERE id=?2",
        params![now(), peer.0],
    )
    .map_err(db_error)?;
    Ok(peer)
}

pub(super) async fn accept_pair(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PairAcceptRequest>,
) -> ApiResult {
    require_secure_transport(&headers)?;
    if input.protocol_version != PROTOCOL_VERSION
        || input.controller_id.len() != 32
        || !input
            .controller_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || input.controller_name.trim().is_empty()
        || input.controller_name.chars().count() > 80
        || input.pairing_code.len() != 64
        || !input
            .pairing_code
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "ARG Node protocol handshake is invalid",
        ));
    }
    let db = state.lock().unwrap();
    let hash = token_hash(&input.pairing_code);
    let user_id: Option<i64> = db.query_row("SELECT u.id FROM federation_pairing_codes p JOIN users u ON u.username=p.created_by AND u.enabled=1 AND u.role='admin' WHERE p.code_hash=?1 AND p.expires_at>?2", params![hash, now()], |row| row.get(0)).optional().map_err(db_error)?;
    let Some(user_id) = user_id else {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "کد اتصال منقضی یا قبلاً استفاده شده است.",
        ));
    };
    let node_id: String = db
        .query_row(
            "SELECT value FROM settings WHERE key='federation_node_id'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?
        .unwrap_or_else(|| token()[..32].to_owned());
    db.execute("INSERT INTO settings(key,value) VALUES('federation_node_id',?1) ON CONFLICT(key) DO NOTHING", [&node_id]).map_err(db_error)?;
    db.execute(
        "DELETE FROM federation_pairing_codes WHERE code_hash=?1",
        [&hash],
    )
    .map_err(db_error)?;
    let raw = token();
    let workspace_token = token();
    let name = input.controller_name.trim();
    db.execute("INSERT INTO federation_controllers(controller_id,controller_name,token_hash,created_at,last_seen,enabled) VALUES (?1,?2,?3,?4,?4,1)", params![input.controller_id, name, token_hash(&raw), now()]).map_err(db_error)?;
    let workspace_hash = session_hash(&workspace_token);
    let at = now();
    db.execute("INSERT INTO sessions(token_hash,user_id,expires_at,created_at,last_seen,remote_ip,user_agent) VALUES (?1,?2,?3,?4,?4,'federation','ARG remote workspace')", params![workspace_hash, user_id, at + 10 * 365 * 24 * 60 * 60, at]).map_err(db_error)?;
    db.execute(
        "INSERT INTO federation_proxy_sessions(controller_id,token_hash,user_id) VALUES (?1,?2,?3)",
        params![input.controller_id, workspace_hash, user_id],
    )
    .map_err(db_error)?;
    audit(
        &db,
        name,
        "federation.controller.pair",
        "protocol=arg-node/2; scope=authenticated ARG workspace",
    );
    Ok(Json(
        json!({"protocol_version":PROTOCOL_VERSION,"node_id":node_id,"controller_id":input.controller_id,"node_token":raw,"workspace_token":workspace_token,"capabilities":["node.status.read","node.services.read","node.services.manage","workspace.api.proxy"]}),
    ))
}

pub(super) async fn status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    let _ = authorize_controller(&db, &headers)?;
    let node_id: String = db
        .query_row(
            "SELECT value FROM settings WHERE key='federation_node_id'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_default();
    drop(db);
    let hostname = fs::read_to_string("/etc/hostname")
        .unwrap_or_default()
        .trim()
        .to_owned();
    let uptime = fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|text| text.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.0);
    let load = fs::read_to_string("/proc/loadavg")
        .unwrap_or_default()
        .split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");
    let memory = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let total_kib = memory
        .lines()
        .find(|line| line.starts_with("MemTotal:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let available_kib = memory
        .lines()
        .find(|line| line.starts_with("MemAvailable:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    Ok(Json(
        json!({"protocol":"arg-node/2","node_id":node_id,"hostname":hostname,"os":fs::read_to_string("/etc/os-release").unwrap_or_default().lines().find(|line| line.starts_with("PRETTY_NAME=")).map(|line| line.trim_start_matches("PRETTY_NAME=").trim_matches('"')).unwrap_or("Linux"),"architecture":std::env::consts::ARCH,"arg_version":env!("CARGO_PKG_VERSION"),"uptime_seconds":uptime as u64,"load":load,"memory_total_kib":total_kib,"memory_available_kib":available_kib,"capabilities":["node.status.read","node.services.read","node.services.manage","workspace.api.proxy"]}),
    ))
}

pub(super) async fn services(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        authorize_controller(&db, &headers)?;
    }
    let result = privileged_request_async(json!({"action":"service-list"})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "خواندن سرویس‌های Node ناموفق بود.",
        ));
    }
    Ok(Json(json!({"services":result["data"]})))
}

fn protected_service(unit: &str) -> bool {
    let base = unit.trim_end_matches(".service");
    [
        "arg",
        "ssh",
        "sshd",
        "networking",
        "systemd-networkd",
        "systemd-resolved",
        "dbus",
        "user@0",
    ]
    .iter()
    .any(|blocked| base == *blocked || base.starts_with(&format!("{blocked}@")))
}

pub(super) async fn service_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ServiceActionRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        let (_, name) = authorize_controller(&db, &headers)?;
        name
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
        "reset-failed",
        "status",
        "logs",
    ];
    if !allowed.contains(&input.action.as_str())
        || !valid_service_unit(&input.unit)
        || protected_service(&input.unit)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "این سرویس یا عملیات از پروتکل Node مسدود است.",
        ));
    }
    if ["stop", "restart", "disable", "disable-now"].contains(&input.action.as_str())
        && !input.confirmed
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "برای این عملیات تأیید صریح لازم است.",
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
                .unwrap_or("عملیات سرویس ناموفق بود."),
        ));
    }
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor,
        "federation.service.action",
        &format!("{} {}", input.action, input.unit),
    );
    Ok(Json(
        json!({"output":result["stdout"].as_str().unwrap_or("")}),
    ))
}

async fn remote_node_request(
    state: &AppState,
    id: i64,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let (endpoint, remote_id, last_token) = {
        let db = state.lock().unwrap();
        let (endpoint, remote_id): (String, String) = db
            .query_row(
                "SELECT endpoint,controller_id FROM federated_nodes WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| error(StatusCode::NOT_FOUND, "Node پیدا نشد."))?;
        let token_path = token_path(&db, id)?;
        (endpoint, remote_id, read_node_token(&token_path)?)
    };
    let endpoint =
        validate_endpoint(&endpoint).map_err(|message| error(StatusCode::BAD_GATEWAY, &message))?;
    let url = endpoint
        .join(path)
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "مسیر پروتکل Node نامعتبر است."))?;
    let client = http_client()?;
    let mut request = client
        .request(method, url)
        .bearer_auth(last_token)
        .header("X-ARG-Transport", "https")
        .header("X-ARG-Protocol", PROTOCOL_VERSION.to_string())
        .header("X-ARG-Controller-ID", remote_id);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.map_err(|_| {
        error(
            StatusCode::BAD_GATEWAY,
            "ارتباط با Node برقرار نشد؛ وضعیت شبکه، سرویس و گواهی TLS را بررسی کن.",
        )
    })?;
    let status = response.status();
    let value = response
        .json::<Value>()
        .await
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "پاسخ Node قابل‌خواندن نیست."))?;
    if !status.is_success() {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            value["error"].as_str().unwrap_or("Node عملیات را نپذیرفت."),
        ));
    }
    let db = state.lock().unwrap();
    db.execute(
        "UPDATE federated_nodes SET last_seen=?1 WHERE id=?2",
        params![now(), id],
    )
    .map_err(db_error)?;
    Ok(value)
}

pub(super) async fn node_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    remote_node_request(
        &state,
        id,
        reqwest::Method::GET,
        "api/federation/status",
        None,
    )
    .await
    .map(Json)
}

pub(super) async fn node_services(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    remote_node_request(
        &state,
        id,
        reqwest::Method::GET,
        "api/federation/services",
        None,
    )
    .await
    .map(Json)
}

pub(super) async fn node_service_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(input): Json<ServiceActionRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    let result = remote_node_request(
        &state,
        id,
        reqwest::Method::POST,
        "api/federation/services/action",
        Some(json!({"action":input.action,"unit":input.unit,"confirmed":input.confirmed})),
    )
    .await?;
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor,
        "federation.node.service.action",
        &format!("node_id={id}; action={} unit={}", input.action, input.unit),
    );
    Ok(Json(result))
}

/// Authenticated controller-side reverse proxy for the selected ARG workspace.
/// The remote admin session stays in a root-only file and is never sent to the browser.
pub(super) async fn proxy_workspace_api(
    State(state): State<AppState>,
    Path((id, remote_path)): Path<(i64, String)>,
    OriginalUri(uri): OriginalUri,
    request: Request,
) -> Response {
    let method = request.method().clone();
    let headers = request.headers().clone();
    {
        let db = state.lock().unwrap();
        if let Err(failure) = require_admin(&db, &headers) {
            return failure.into_response();
        }
        if method != reqwest::Method::GET && method != reqwest::Method::HEAD {
            if let Err(failure) = check_origin(&headers) {
                return failure.into_response();
            }
        }
    }
    let normalized = remote_path.trim_start_matches('/');
    if !valid_workspace_api_path(normalized) {
        return error(StatusCode::BAD_REQUEST, "مسیر فضای کار Node مجاز نیست.").into_response();
    }
    if !matches!(
        method,
        reqwest::Method::GET
            | reqwest::Method::HEAD
            | reqwest::Method::POST
            | reqwest::Method::PUT
            | reqwest::Method::PATCH
            | reqwest::Method::DELETE
    ) {
        return error(
            StatusCode::METHOD_NOT_ALLOWED,
            "این روش درخواست برای فضای کار Node مجاز نیست.",
        )
        .into_response();
    }
    let (endpoint, token) = {
        let db = state.lock().unwrap();
        let endpoint: String = match db.query_row(
            "SELECT endpoint FROM federated_nodes WHERE id=?1",
            [id],
            |row| row.get(0),
        ) {
            Ok(value) => value,
            Err(_) => return error(StatusCode::NOT_FOUND, "Node پیدا نشد.").into_response(),
        };
        let path = match workspace_token_path(&db, id) {
            Ok(path) => path,
            Err(failure) => return failure.into_response(),
        };
        let token = match read_node_token(&path) {
            Ok(value) => value,
            Err(_) => {
                return error(
                    StatusCode::CONFLICT,
                    "این Node برای فضای کار کامل آماده نیست؛ آن را حذف و دوباره جفت‌سازی کن.",
                )
                .into_response()
            }
        };
        (endpoint, token)
    };
    let Ok(endpoint) = validate_endpoint(&endpoint) else {
        return error(StatusCode::BAD_GATEWAY, "آدرس Node ذخیره‌شده معتبر نیست.").into_response();
    };
    let Some(host) = endpoint.host_str() else {
        return error(StatusCode::BAD_GATEWAY, "میزبان Node موجود نیست.").into_response();
    };
    let port = endpoint
        .port()
        .map(|value| format!(":{value}"))
        .unwrap_or_default();
    let remote_url = format!(
        "{}/api/{}{}",
        endpoint.as_str().trim_end_matches('/'),
        normalized,
        uri.query()
            .map(|value| format!("?{value}"))
            .unwrap_or_default()
    );
    let client = match workspace_http_client() {
        Ok(value) => value,
        Err(failure) => return failure.into_response(),
    };
    let mut outgoing = client
        .request(method.clone(), remote_url)
        .header(header::COOKIE, format!("arg_session={token}"))
        .header(header::ORIGIN, format!("https://{host}{port}"))
        .header("X-ARG-Transport", "https");
    for name in [
        header::CONTENT_TYPE,
        header::ACCEPT,
        header::RANGE,
        header::IF_RANGE,
    ] {
        if let Some(value) = headers.get(&name) {
            outgoing = outgoing.header(name, value);
        }
    }
    if method != reqwest::Method::GET && method != reqwest::Method::HEAD {
        let mut received = 0u64;
        let stream = request
            .into_body()
            .into_data_stream()
            .map(move |chunk| match chunk {
                Ok(bytes) => {
                    received = received.saturating_add(bytes.len() as u64);
                    if received > 512 * 1024 * 1024 {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Request exceeded the 512 MiB workspace limit",
                        ))
                    } else {
                        Ok(bytes)
                    }
                }
                Err(_) => Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionAborted,
                    "Workspace request stream ended unexpectedly",
                )),
            });
        outgoing = outgoing.body(reqwest::Body::wrap_stream(stream));
    }
    let remote = match outgoing.send().await {
        Ok(response) => response,
        Err(_) => {
            return error(
                StatusCode::BAD_GATEWAY,
                "ارتباط با فضای کار Node برقرار نشد؛ وضعیت شبکه و گواهی HTTPS را بررسی کن.",
            )
            .into_response()
        }
    };
    let status = remote.status();
    let headers = remote.headers().clone();
    {
        let db = state.lock().unwrap();
        let _ = db.execute(
            "UPDATE federated_nodes SET last_seen=?1 WHERE id=?2",
            params![now(), id],
        );
    }
    let mut response = Response::builder().status(status);
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_DISPOSITION,
        header::CACHE_CONTROL,
        header::ETAG,
        header::LAST_MODIFIED,
        header::ACCEPT_RANGES,
        header::CONTENT_RANGE,
    ] {
        if let Some(value) = headers.get(&name) {
            response = response.header(name, value);
        }
    }
    let proxy_cookie_path = format!("/api/nodes/{id}/workspace-terminal");
    for value in headers.get_all(header::SET_COOKIE) {
        if let Ok(cookie) = value.to_str() {
            if !cookie.starts_with("arg_root_ticket=") {
                continue;
            }
            let cookie = cookie.replace("Path=/api/terminal", &format!("Path={proxy_cookie_path}"));
            response = response.header(header::SET_COOKIE, cookie);
        }
    }
    response
        .body(Body::from_stream(remote.bytes_stream()))
        .unwrap_or_else(|_| {
            error(
                StatusCode::BAD_GATEWAY,
                "ساخت پاسخ فضای کار Node ناموفق بود.",
            )
            .into_response()
        })
}

pub(super) async fn proxy_workspace_terminal(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> Response {
    let expected_scheme = if headers
        .get("x-arg-transport")
        .and_then(|value| value.to_str().ok())
        == Some("https")
    {
        "https"
    } else {
        "http"
    };
    let origin_ok = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<url::Url>().ok())
        .zip(
            headers
                .get(header::HOST)
                .and_then(|value| value.to_str().ok()),
        )
        .is_some_and(|(origin, host)| {
            origin.host_str().is_some()
                && origin.authority() == host
                && origin.scheme() == expected_scheme
        });
    if !origin_ok {
        return (StatusCode::FORBIDDEN, "Invalid WebSocket origin").into_response();
    }
    {
        let db = state.lock().unwrap();
        if require_admin(&db, &headers).is_err() {
            return (StatusCode::FORBIDDEN, "Administrator access required").into_response();
        }
    }
    let (endpoint, session) = {
        let db = state.lock().unwrap();
        let endpoint = match db.query_row(
            "SELECT endpoint FROM federated_nodes WHERE id=?1",
            [id],
            |row| row.get::<_, String>(0),
        ) {
            Ok(value) => value,
            Err(_) => return (StatusCode::NOT_FOUND, "Node not found").into_response(),
        };
        let path = match workspace_token_path(&db, id) {
            Ok(path) => path,
            Err(_) => return (StatusCode::CONFLICT, "Workspace pairing required").into_response(),
        };
        let session = match read_node_token(&path) {
            Ok(value) => value,
            Err(_) => return (StatusCode::CONFLICT, "Workspace pairing required").into_response(),
        };
        (endpoint, session)
    };
    let root_ticket = cookie_named(&headers, "arg_root_ticket");
    let query = uri
        .query()
        .map(|value| format!("?{value}"))
        .unwrap_or_default();
    {
        let db = state.lock().unwrap();
        let _ = db.execute(
            "UPDATE federated_nodes SET last_seen=?1 WHERE id=?2",
            params![now(), id],
        );
    }
    ws.on_upgrade(move |mut browser| async move {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let url = format!("{}/api/terminal{}", endpoint.trim_end_matches('/'), query).replace("https://", "wss://");
        let mut request = match url.into_client_request() { Ok(request) => request, Err(_) => { let _ = browser.send(axum::extract::ws::Message::text("\r\nWorkspace terminal URL is invalid.\r\n")).await; return; } };
        let mut cookie = format!("arg_session={session}");
        if let Some(ticket) = root_ticket { cookie.push_str(&format!("; arg_root_ticket={ticket}")); }
        if let Ok(value) = axum::http::HeaderValue::from_str(&cookie) { request.headers_mut().insert(header::COOKIE, value); }
        let remote_origin = endpoint.trim_end_matches('/');
        if let Ok(origin) = axum::http::HeaderValue::from_str(remote_origin) { request.headers_mut().insert(header::ORIGIN, origin); }
        request.headers_mut().insert("x-forwarded-proto", axum::http::HeaderValue::from_static("https"));
        let Ok((remote, _)) = tokio_tungstenite::connect_async(request).await else {
            let _ = browser.send(axum::extract::ws::Message::text("\r\nCould not connect to remote ARG terminal.\r\n")).await;
            let _ = browser.close().await;
            return;
        };
        let (mut remote_sink, mut remote_stream) = remote.split();
        loop {
            tokio::select! {
                message = browser.recv() => match message {
                    Some(Ok(axum::extract::ws::Message::Text(value))) => if remote_sink.send(tokio_tungstenite::tungstenite::Message::Text(value.to_string().into())).await.is_err() { break; },
                    Some(Ok(axum::extract::ws::Message::Binary(value))) => if remote_sink.send(tokio_tungstenite::tungstenite::Message::Binary(value)).await.is_err() { break; },
                    Some(Ok(axum::extract::ws::Message::Ping(value))) => if remote_sink.send(tokio_tungstenite::tungstenite::Message::Ping(value)).await.is_err() { break; },
                    Some(Ok(axum::extract::ws::Message::Pong(value))) => if remote_sink.send(tokio_tungstenite::tungstenite::Message::Pong(value)).await.is_err() { break; },
                    Some(Ok(axum::extract::ws::Message::Close(_))) | None | Some(Err(_)) => { let _ = remote_sink.send(tokio_tungstenite::tungstenite::Message::Close(None)).await; break; },
                },
                message = remote_stream.next() => match message {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(value))) => if browser.send(axum::extract::ws::Message::text(value.to_string())).await.is_err() { break; },
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(value))) => if browser.send(axum::extract::ws::Message::Binary(value)).await.is_err() { break; },
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(value))) => if browser.send(axum::extract::ws::Message::Ping(value)).await.is_err() { break; },
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Pong(value))) => if browser.send(axum::extract::ws::Message::Pong(value)).await.is_err() { break; },
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) | None | Some(Err(_)) => { let _ = browser.close().await; break; },
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Frame(_))) => {},
                }
            }
        }
    })
}

pub(super) async fn remove_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    let _: Value = remote_node_request(
        &state,
        id,
        reqwest::Method::DELETE,
        "api/federation/controller",
        None,
    )
    .await?;
    let db = state.lock().unwrap();
    let path = token_path(&db, id)?;
    let workspace_path = workspace_token_path(&db, id)?;
    db.execute("DELETE FROM federated_nodes WHERE id=?1", [id])
        .map_err(db_error)?;
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(workspace_path);
    audit(
        &db,
        &actor,
        "federation.node.remove",
        &format!("node_id={id}"),
    );
    Ok(Json(json!({"removed":true})))
}

pub(super) async fn revoke_controller(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult {
    let db = state.lock().unwrap();
    let (id, name) = authorize_controller(&db, &headers)?;
    db.execute("DELETE FROM federation_controllers WHERE id=?1", [id])
        .map_err(db_error)?;
    audit(
        &db,
        &name,
        "federation.controller.revoke",
        "self-revoked from controller",
    );
    Ok(Json(json!({"revoked":true})))
}

pub(super) async fn revoke_controller_admin(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let name: String = db
        .query_row(
            "SELECT controller_name FROM federation_controllers WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "کنترل‌کننده پیدا نشد."))?;
    db.execute("DELETE FROM federation_controllers WHERE id=?1", [id])
        .map_err(db_error)?;
    audit(
        &db,
        &actor.username,
        "federation.controller.revoke",
        &format!("controller={name}"),
    );
    Ok(Json(json!({"revoked":true,"name":name})))
}

#[cfg(test)]
mod tests {
    use super::{valid_workspace_api_path, validate_endpoint};
    #[test]
    fn node_endpoint_requires_a_clean_https_origin() {
        assert!(validate_endpoint("https://node.example.test/").is_ok());
        assert!(validate_endpoint("http://node.example.test").is_err());
        assert!(validate_endpoint("https://user:pass@node.example.test").is_err());
        assert!(validate_endpoint("https://node.example.test/api").is_err());
    }

    #[test]
    fn workspace_api_proxy_rejects_traversal_and_control_plane_routes() {
        assert!(valid_workspace_api_path("dashboard"));
        assert!(valid_workspace_api_path("files/list"));
        assert!(valid_workspace_api_path("nginx/sites/example.com"));
        assert!(!valid_workspace_api_path("../login"));
        assert!(!valid_workspace_api_path("files/%2e%2e/login"));
        assert!(!valid_workspace_api_path("nodes/1/proxy/dashboard"));
        assert!(!valid_workspace_api_path("federation/pair"));
        assert!(!valid_workspace_api_path("files//list"));
    }
}
