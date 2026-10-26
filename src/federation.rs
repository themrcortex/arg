use super::*;
use axum::http::header::AUTHORIZATION;
use std::path::Path as FsPath;

const PROTOCOL_VERSION: u32 = 1;
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
        json!({"protocol":"arg-node/1","nodes":nodes,"controllers":controllers}),
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
        "protocol=arg-node/1; expires=300s",
    );
    Ok(Json(
        json!({"pairing_code":pairing_code,"protocol":"arg-node/1","expires_at":expires_at,"warning":"این کد را فقط در پنل مورداعتماد وارد کن؛ پس از 5 دقیقه یا اولین استفاده منقضی می‌شود."}),
    ))
}

pub(super) async fn add_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AddNodeRequest>,
) -> ApiResult {
    check_origin(&headers)?;
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
            "Node کد اتصال را نپذیرفت؛ کد ممکن است منقضی یا قبلاً مصرف شده باشد.",
        ));
    }
    let payload: Value = response
        .json()
        .await
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "پاسخ پروتکل Node نامعتبر است."))?;
    if payload["protocol_version"].as_u64() != Some(PROTOCOL_VERSION as u64)
        || payload["node_token"].as_str().is_none()
        || payload["node_id"].as_str().is_none()
        || payload["controller_id"].as_str() != Some(controller_id.as_str())
    {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            "نسخه یا شناسهٔ پروتکل Node سازگار نیست.",
        ));
    }
    let remote_token = payload["node_token"].as_str().unwrap_or_default();
    if remote_token.len() != 64 || !remote_token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(error(StatusCode::BAD_GATEWAY, "Node کلید معتبر صادر نکرد."));
    }
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    let db = state.lock().unwrap();
    let endpoint_text = endpoint.as_str().trim_end_matches('/').to_owned();
    db.execute("INSERT INTO federated_nodes(name,endpoint,node_id,controller_id,protocol_version,created_at,last_seen) VALUES (?1,?2,?3,?4,?5,?6,?6)", params![input.name.trim(), endpoint_text, payload["node_id"].as_str().unwrap_or_default(), controller_id, PROTOCOL_VERSION, now()]).map_err(db_error)?;
    let id = db.last_insert_rowid();
    let secret_path = token_path(&db, id)?;
    if let Err(err) = write_node_token(&secret_path, remote_token) {
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
        json!({"id":id,"name":input.name.trim(),"node_id":payload["node_id"],"protocol":"arg-node/1","capabilities":payload["capabilities"],"message":"Node با اتصال امن جفت شد."}),
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
    let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM federation_pairing_codes WHERE code_hash=?1 AND expires_at>?2)", params![hash, now()], |row| row.get(0)).map_err(db_error)?;
    if !valid {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "کد اتصال منقضی یا قبلاً استفاده شده است.",
        ));
    }
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
    let name = input.controller_name.trim();
    db.execute("INSERT INTO federation_controllers(controller_id,controller_name,token_hash,created_at,last_seen,enabled) VALUES (?1,?2,?3,?4,?4,1)", params![input.controller_id, name, token_hash(&raw), now()]).map_err(db_error)?;
    audit(
        &db,
        name,
        "federation.controller.pair",
        "protocol=arg-node/1; scope=status,systemd.services",
    );
    Ok(Json(
        json!({"protocol_version":PROTOCOL_VERSION,"node_id":node_id,"controller_id":input.controller_id,"node_token":raw,"capabilities":["node.status.read","node.services.read","node.services.manage"]}),
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
        json!({"protocol":"arg-node/1","node_id":node_id,"hostname":hostname,"os":fs::read_to_string("/etc/os-release").unwrap_or_default().lines().find(|line| line.starts_with("PRETTY_NAME=")).map(|line| line.trim_start_matches("PRETTY_NAME=").trim_matches('"')).unwrap_or("Linux"),"architecture":std::env::consts::ARCH,"arg_version":env!("CARGO_PKG_VERSION"),"uptime_seconds":uptime as u64,"load":load,"memory_total_kib":total_kib,"memory_available_kib":available_kib,"capabilities":["node.status.read","node.services.read","node.services.manage"]}),
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
    db.execute("DELETE FROM federated_nodes WHERE id=?1", [id])
        .map_err(db_error)?;
    let _ = fs::remove_file(path);
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
    use super::validate_endpoint;
    #[test]
    fn node_endpoint_requires_a_clean_https_origin() {
        assert!(validate_endpoint("https://node.example.test/").is_ok());
        assert!(validate_endpoint("http://node.example.test").is_err());
        assert!(validate_endpoint("https://user:pass@node.example.test").is_err());
        assert!(validate_endpoint("https://node.example.test/api").is_err());
    }
}
