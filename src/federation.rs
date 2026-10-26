use super::*;
use axum::http::header::AUTHORIZATION;
use axum::{body::Body, extract::OriginalUri};
use std::path::Path as FsPath;

const PROTOCOL_VERSION: u32 = 2;
const CAPABILITIES_VERSION: u32 = 1;
const CONTROLLER_TOKEN_TTL: i64 = 90 * 24 * 60 * 60;
const WORKSPACE_SESSION_TTL: i64 = 12 * 60 * 60;
const TOKEN_DIR: &str = "node-secrets";

#[derive(Deserialize)]
pub(super) struct PairCodeRequest {
    #[serde(default)]
    replace_existing: bool,
    #[serde(default = "default_pairing_mode")]
    access_mode: String,
}

fn default_pairing_mode() -> String { "full".to_owned() }

#[derive(Deserialize)]
pub(super) struct AddNodeRequest {
    name: String,
    endpoint: String,
    pairing_code: String,
    #[serde(default)]
    allow_private_endpoint: bool,
}

#[derive(Serialize)]
struct NodeRecord {
    id: i64,
    workspace_key: String,
    name: String,
    endpoint: String,
    node_id: String,
    server_uuid: Option<String>,
    public_name: String,
    scopes: Vec<String>,
    capabilities: Vec<String>,
    compatibility_state: String,
    connection_state: String,
    last_error: Option<String>,
    protocol_version: u32,
    last_seen: Option<i64>,
    created_at: i64,
    workspace_ready: bool,
    allow_private_endpoint: bool,
    workspace_token_expires_at: Option<i64>,
}

#[derive(Deserialize)]
pub(super) struct PairAcceptRequest {
    protocol_version: u32,
    controller_id: String,
    controller_name: String,
    #[serde(default)]
    server_uuid: Option<String>,
    #[serde(default)]
    capabilities_version: Option<u32>,
    #[serde(default)]
    supported_protocol_versions: Vec<u32>,
    pairing_code: String,
}

#[derive(Deserialize)]
pub(super) struct ServiceActionRequest {
    action: String,
    unit: String,
    #[serde(default)]
    confirmed: bool,
}

#[derive(Deserialize)]
pub(super) struct ControllerNameRequest { name: String }

#[derive(Deserialize)]
pub(super) struct PublicNameRequest { name: String }

#[derive(Deserialize)]
pub(super) struct ServerRoleRequest { role: String, #[serde(default)] confirmed: bool }

#[derive(Deserialize)]
pub(super) struct RotateCredentialRequest { new_token: String }

#[derive(Clone)]
struct ControllerIdentity { id: i64, name: String, scopes: Vec<String>, token_expires_at: Option<i64> }

fn role_valid(role: &str) -> bool {
    matches!(role, "standalone" | "member" | "controller-capable" | "both")
}

fn scopes_for_mode(mode: &str) -> Option<Vec<&'static str>> {
    // A read-only pairing may inspect the remote server and revoke itself, but it
    // must not rotate the credential that authenticates it.
    let mut scopes = vec!["node.status.read", "node.services.read", "server.identity.read", "controller.self_revoke", "workspace.read", "workspace.session.renew"];
    match mode {
        "read-only" => Some(scopes),
        "full" => {
            scopes.extend(["node.services.manage", "server.public_name.sync", "server.credential.rotate", "workspace.write", "workspace.terminal", "workspace.root_terminal"]);
            Some(scopes)
        }
        _ => None,
    }
}

fn known_scope(scope: &str) -> bool {
    ["node.status.read", "node.services.read", "node.services.manage", "server.identity.read", "server.public_name.sync", "server.credential.rotate", "controller.self_revoke", "workspace.read", "workspace.write", "workspace.terminal", "workspace.root_terminal", "workspace.session.renew"].contains(&scope)
}

fn known_capability(capability: &str) -> bool {
    ["node.status.read", "node.services.read", "node.services.manage", "workspace.api.proxy", "workspace.session.renew", "workspace.terminal", "server.identity.read", "server.public_name.sync", "server.credential.rotate", "capabilities.negotiate.v1"].contains(&capability)
}

fn supported_capabilities() -> Vec<&'static str> {
    vec!["capabilities.negotiate.v1", "node.status.read", "node.services.read", "node.services.manage", "workspace.api.proxy", "workspace.session.renew", "workspace.terminal", "server.identity.read", "server.public_name.sync", "server.credential.rotate"]
}

fn decode_scopes(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn local_federation_role(db: &Connection) -> Result<String, rusqlite::Error> {
    if let Some(value) = db.query_row("SELECT value FROM settings WHERE key='federation_role'", [], |row| row.get::<_, String>(0)).optional()? {
        return Ok(value);
    }
    db.execute("INSERT INTO settings(key,value) VALUES('federation_role','both') ON CONFLICT(key) DO NOTHING", [])?;
    db.query_row("SELECT value FROM settings WHERE key='federation_role'", [], |row| row.get(0))
}

fn role_can_pair_outward(role: &str) -> bool { matches!(role, "controller-capable" | "both") }
fn role_can_accept_inbound(role: &str) -> bool { matches!(role, "member" | "both") }

fn controller_scope(identity: &ControllerIdentity, required: &str) -> Result<(), (StatusCode, Json<Value>)> {
    if identity.scopes.iter().any(|scope| scope == required) {
        Ok(())
    } else {
        Err(error(StatusCode::FORBIDDEN, "این اتصال Federation مجوز لازم برای عملیات را ندارد؛ با دسترسی مناسب دوباره جفت‌سازی کن."))
    }
}

pub(super) fn local_server_uuid(db: &Connection) -> Result<String, rusqlite::Error> {
    if let Some(value) = db.query_row("SELECT value FROM settings WHERE key='federation_server_uuid'", [], |row| row.get::<_, String>(0)).optional()? {
        return Ok(value);
    }
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex::encode(bytes);
    let value = format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32]);
    db.execute("INSERT INTO settings(key,value) VALUES('federation_server_uuid',?1) ON CONFLICT(key) DO NOTHING", [&value])?;
    db.query_row("SELECT value FROM settings WHERE key='federation_server_uuid'", [], |row| row.get(0))
}

fn local_public_name(db: &Connection) -> Result<String, rusqlite::Error> {
    if let Some(value) = db.query_row("SELECT value FROM settings WHERE key='federation_public_name'", [], |row| row.get::<_, String>(0)).optional()? {
        return Ok(value);
    }
    let hostname = fs::read_to_string("/etc/hostname").unwrap_or_default();
    let name = hostname.trim().chars().take(80).collect::<String>();
    let name = if name.is_empty() { "ARG Server".to_owned() } else { name };
    db.execute("INSERT INTO settings(key,value) VALUES('federation_public_name',?1) ON CONFLICT(key) DO NOTHING", [&name])?;
    db.query_row("SELECT value FROM settings WHERE key='federation_public_name'", [], |row| row.get(0))
}

fn validate_public_name(value: &str) -> Result<&str, (StatusCode, Json<Value>)> {
    let name = value.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(error(StatusCode::BAD_REQUEST, "نام عمومی باید بین ۱ تا ۸۰ نویسه باشد."));
    }
    Ok(name)
}

fn valid_server_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit()
        })
}

fn workspace_key(server_uuid: Option<&str>, local_id: i64) -> String {
    server_uuid
        .filter(|value| valid_server_uuid(value))
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| format!("legacy:{local_id}"))
}

fn resolve_workspace_node_id(
    db: &Connection,
    key: &str,
) -> Result<i64, (StatusCode, Json<Value>)> {
    let id = if let Some(uuid) = key.strip_prefix("legacy:") {
        let Ok(id) = uuid.parse::<i64>() else {
            return Err(error(StatusCode::NOT_FOUND, "Node فضای کار پیدا نشد."));
        };
        if id <= 0 {
            return Err(error(StatusCode::NOT_FOUND, "Node فضای کار پیدا نشد."));
        }
        db.query_row(
            "SELECT id FROM federated_nodes WHERE id=?1 AND (server_uuid IS NULL OR server_uuid='')",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "خواندن Node فضای کار ناموفق بود."))?
    } else if valid_server_uuid(key) {
        db.query_row(
            "SELECT id FROM federated_nodes WHERE server_uuid=?1",
            [key.to_ascii_lowercase()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "خواندن Node فضای کار ناموفق بود."))?
    } else {
        return Err(error(StatusCode::NOT_FOUND, "Node فضای کار پیدا نشد."));
    };
    id.ok_or_else(|| error(StatusCode::NOT_FOUND, "Node فضای کار پیدا نشد."))
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
    if let Some(host) = parsed.host_str() {
        let host = host.trim_start_matches('[').trim_end_matches(']');
        let blocked_name = host.eq_ignore_ascii_case("localhost")
            || host.ends_with(".localhost")
            || host.eq_ignore_ascii_case("metadata.google.internal")
            || host.eq_ignore_ascii_case("metadata.google")
            || host.eq_ignore_ascii_case("instance-data.ec2.internal");
        let blocked_ip = host.parse::<std::net::IpAddr>().ok().is_some_and(|ip| match ip {
            std::net::IpAddr::V4(ip) => ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() || ip.is_link_local(),
            std::net::IpAddr::V6(ip) => ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() || ip.is_unicast_link_local(),
        });
        if blocked_name || blocked_ip {
            return Err("آدرس‌های loopback، link-local و metadata برای اتصال Federation مجاز نیستند.".into());
        }
    }
    Ok(parsed)
}

fn address_is_private(address: std::net::IpAddr) -> bool {
    match address {
        std::net::IpAddr::V4(ip) => ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() || ip.is_multicast() || ip.is_broadcast(),
        std::net::IpAddr::V6(ip) => {
            if ip.is_loopback() || ip.is_unspecified() || ip.is_unicast_link_local() || ip.is_multicast() || ip.is_unique_local() { return true; }
            if let Some(mapped) = ip.to_ipv4() { return address_is_private(std::net::IpAddr::V4(mapped)); }
            false
        }
    }
}

async fn resolve_endpoint_addresses(endpoint: &url::Url, allow_private: bool) -> Result<Vec<std::net::SocketAddr>, String> {
    let host = endpoint.host_str().ok_or_else(|| "میزبان Node موجود نیست.".to_owned())?;
    let port = endpoint.port_or_known_default().ok_or_else(|| "پورت HTTPS معتبر نیست.".to_owned())?;
    let addresses = if let Ok(address) = host.parse::<std::net::IpAddr>() {
        vec![std::net::SocketAddr::new(address, port)]
    } else {
        tokio::time::timeout(std::time::Duration::from_secs(5), tokio::net::lookup_host((host, port)))
            .await.map_err(|_| "مهلت جست‌وجوی DNS برای Node تمام شد.".to_owned())?
            .map_err(|_| "نام میزبان Node به IP تبدیل نشد.".to_owned())?
            .collect::<Vec<_>>()
    };
    if addresses.is_empty() { return Err("میزبان Node هیچ IP معتبری ندارد.".into()); }
    if addresses.iter().any(|address| address_is_private(address.ip()) && !allow_private) {
        return Err("این نام میزبان به شبکهٔ خصوصی resolve می‌شود؛ اگر Node متعلق به شبکهٔ خصوصی مورداعتماد است، اجازهٔ اتصال خصوصی را فعال کن.".into());
    }
    if addresses.iter().any(|address| match address.ip() {
        std::net::IpAddr::V4(ip) => ip.is_loopback() || ip.is_unspecified() || ip.is_link_local() || ip.is_multicast(),
        std::net::IpAddr::V6(ip) => ip.is_loopback() || ip.is_unspecified() || ip.is_unicast_link_local() || ip.is_multicast(),
    }) {
        return Err("آدرس loopback، link-local یا metadata برای اتصال Federation مجاز نیست.".into());
    }
    Ok(addresses)
}

fn http_client_for_endpoint(endpoint: &url::Url, addresses: &[std::net::SocketAddr], timeout: u64) -> Result<reqwest::Client, (StatusCode, Json<Value>)> {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout))
        .connect_timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("ARG-Node/", env!("CARGO_PKG_VERSION")));
    if let Some(host) = endpoint.host_str().filter(|host| host.parse::<std::net::IpAddr>().is_err()) {
        builder = builder.resolve_to_addrs(host, addresses);
    }
    builder.build().map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "ساخت اتصال امن Node ناموفق بود."))
}

fn session_scope_for(path: &str, method: &axum::http::Method) -> &'static str {
    if path == "/api/terminal/root/authorize" || path.starts_with("/api/terminal/root/") {
        "workspace.root_terminal"
    } else if path == "/api/terminal" {
        "workspace.terminal"
    } else if matches!(*method, axum::http::Method::GET | axum::http::Method::HEAD) {
        "workspace.read"
    } else {
        "workspace.write"
    }
}

pub(super) async fn workspace_session_guard(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    let Some(raw) = cookie_token(request.headers()) else { return next.run(request).await };
    let hash = session_hash(&raw);
    let result = {
        let db = state.lock().unwrap();
        db.query_row(
            "SELECT scopes_json,expires_at,actor_name FROM federation_proxy_sessions WHERE token_hash=?1",
            [&hash],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?, row.get::<_, String>(2)?)),
        ).optional()
    };
    let (scopes_json, expires_at, actor) = match result {
        Ok(Some(value)) => value,
        Ok(None) => return next.run(request).await,
        Err(_) => return error(StatusCode::INTERNAL_SERVER_ERROR, "اعتبارسنجی نشست Federation ناموفق بود.").into_response(),
    };
    if expires_at.map_or(true, |expires| expires <= now()) {
        let db = state.lock().unwrap();
        let _ = db.execute("DELETE FROM federation_proxy_sessions WHERE token_hash=?1", [&hash]);
        return error(StatusCode::UNAUTHORIZED, "نشست فضای کار این سرور منقضی شده است؛ Node را دوباره متصل کن.").into_response();
    }
    let required = if path == "/api/terminal" && cookie_named(request.headers(), "arg_root_ticket").is_some() {
        "workspace.root_terminal"
    } else {
        session_scope_for(&path, &method)
    };
    let scopes = decode_scopes(&scopes_json);
    if !scopes.iter().any(|scope| scope == required) {
        return error(StatusCode::FORBIDDEN, "این نشست Federation اجازهٔ انجام این عملیات را ندارد.").into_response();
    }
    if method != axum::http::Method::GET && method != axum::http::Method::HEAD {
        let db = state.lock().unwrap();
        audit(&db, &actor, "federation.workspace.write", &format!("{} {}", method, path));
    }
    next.run(request).await
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

fn workspace_http_client(endpoint: &url::Url, addresses: &[std::net::SocketAddr]) -> Result<reqwest::Client, (StatusCode, Json<Value>)> {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .connect_timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("ARG-Workspace/", env!("CARGO_PKG_VERSION")));
    if let Some(host) = endpoint.host_str().filter(|host| host.parse::<std::net::IpAddr>().is_err()) {
        builder = builder.resolve_to_addrs(host, addresses);
    }
    builder.build().map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "ساخت تونل امن فضای کار ناموفق بود."))
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
    let server_uuid = local_server_uuid(&db).map_err(db_error)?;
    let public_name = local_public_name(&db).map_err(db_error)?;
    let role = local_federation_role(&db).map_err(db_error)?;
    let mut statement = db.prepare("SELECT id,name,endpoint,node_id,server_uuid,public_name,scopes_json,capabilities_json,compatibility_state,connection_state,last_error,protocol_version,last_seen,created_at,allow_private_endpoint,workspace_token_expires_at FROM federated_nodes ORDER BY name COLLATE NOCASE").map_err(db_error)?;
    let nodes = statement
        .query_map([], |row| {
            Ok(NodeRecord {
                id: row.get(0)?,
                workspace_key: workspace_key(row.get::<_, Option<String>>(4)?.as_deref(), row.get(0)?),
                name: row.get(1)?,
                endpoint: row.get(2)?,
                node_id: row.get(3)?,
                server_uuid: row.get(4)?,
                public_name: row.get(5)?,
                scopes: decode_scopes(&row.get::<_, String>(6)?),
                capabilities: decode_scopes(&row.get::<_, String>(7)?),
                compatibility_state: row.get(8)?,
                connection_state: row.get(9)?,
                last_error: row.get(10)?,
                protocol_version: row.get::<_, i64>(11)? as u32,
                last_seen: row.get(12)?,
                created_at: row.get(13)?,
                workspace_ready: workspace_token_path(&db, row.get(0)?)
                    .ok()
                    .and_then(|path| fs::symlink_metadata(path).ok())
                    .is_some_and(|metadata| {
                        metadata.is_file() && !metadata.file_type().is_symlink()
                    }),
                allow_private_endpoint: row.get::<_, i64>(14)? != 0,
                workspace_token_expires_at: row.get(15)?,
            })
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    let controllers: Vec<Value> = {
        let mut query = db.prepare("SELECT id,controller_name,local_label,created_at,last_seen,server_uuid,scopes_json,token_expires_at FROM federation_controllers WHERE enabled=1 ORDER BY created_at DESC LIMIT 100").map_err(db_error)?;
        let rows = query.query_map([], |row| Ok(json!({"id":row.get::<_,i64>(0)?,"name":row.get::<_,String>(2)?,"public_name":row.get::<_,String>(1)?,"created_at":row.get::<_,i64>(3)?,"last_seen":row.get::<_,Option<i64>>(4)?,"server_uuid":row.get::<_,Option<String>>(5)?,"scopes":decode_scopes(&row.get::<_,String>(6)?),"token_expires_at":row.get::<_,Option<i64>>(7)?}))).map_err(db_error)?;
        rows.filter_map(Result::ok).collect()
    };
    Ok(Json(
        json!({"protocol":"arg-node/2","capabilities_version":CAPABILITIES_VERSION,"server_uuid":server_uuid,"public_name":public_name,"role":role,"nodes":nodes,"controllers":controllers}),
    ))
}

pub(super) async fn update_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ServerRoleRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if !role_valid(&input.role) {
        return Err(error(StatusCode::BAD_REQUEST, "نقش سرور معتبر نیست."));
    }
    if !input.confirmed {
        return Err(error(StatusCode::BAD_REQUEST, "برای تغییر نقش سرور تأیید صریح لازم است."));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let current = local_federation_role(&db).map_err(db_error)?;
    let outbound: i64 = db.query_row("SELECT COUNT(*) FROM federated_nodes", [], |row| row.get(0)).map_err(db_error)?;
    let inbound: i64 = db.query_row("SELECT COUNT(*) FROM federation_controllers WHERE enabled=1", [], |row| row.get(0)).map_err(db_error)?;
    if !role_can_pair_outward(&input.role) && outbound > 0 {
        return Err(error(StatusCode::CONFLICT, &format!("ابتدا اتصال {outbound} Node راه‌دور را از این پنل جدا کن؛ نقش انتخابی اجازهٔ کنترل Node ندارد.")));
    }
    if !role_can_accept_inbound(&input.role) && inbound > 0 {
        return Err(error(StatusCode::CONFLICT, &format!("ابتدا دسترسی {inbound} کنترل‌کننده را لغو کن؛ نقش انتخابی اجازهٔ پذیرش کنترل‌کننده ندارد.")));
    }
    db.execute("INSERT INTO settings(key,value) VALUES('federation_role',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [&input.role]).map_err(db_error)?;
    audit(&db, &actor.username, "federation.server.role", &format!("{current} -> {}", input.role));
    Ok(Json(json!({"role":input.role,"previous_role":current,"message":"نقش سرور ذخیره شد."})))
}

pub(super) async fn create_pairing_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PairCodeRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let role = local_federation_role(&db).map_err(db_error)?;
    if !role_can_accept_inbound(&role) {
        return Err(error(StatusCode::CONFLICT, "نقش این سرور اجازهٔ پذیرش Controller جدید را نمی‌دهد."));
    }
    let Some(scopes) = scopes_for_mode(&input.access_mode) else {
        return Err(error(StatusCode::BAD_REQUEST, "حالت دسترسی باید full یا read-only باشد."));
    };
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
        "INSERT INTO federation_pairing_codes(code_hash,expires_at,created_by,scopes_json) VALUES (?1,?2,?3,?4)",
        params![token_hash(&pairing_code), expires_at, &actor.username, serde_json::to_string(&scopes).unwrap_or_else(|_| "[]".to_owned())],
    )
    .map_err(db_error)?;
    audit(
        &db,
        &actor.username,
        "federation.pairing.create",
        "protocol=arg-node/2; expires=300s",
    );
    Ok(Json(
        json!({"pairing_code":pairing_code,"protocol":"arg-node/2","access_mode":input.access_mode,"scopes":scopes,"expires_at":expires_at,"warning":"این کد را فقط در پنل مورداعتماد وارد کن؛ پس از 5 دقیقه یا اولین استفاده منقضی می‌شود."}),
    ))
}

pub(super) async fn add_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AddNodeRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let (actor, controller_public_name, controller_server_uuid) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?;
        let role = local_federation_role(&db).map_err(db_error)?;
        if !role_can_pair_outward(&role) {
            return Err(error(StatusCode::CONFLICT, "نقش این سرور اجازهٔ اتصال به Node جدید را نمی‌دهد."));
        }
        let name = local_public_name(&db).map_err(db_error)?;
        let server_uuid = local_server_uuid(&db).map_err(db_error)?;
        (actor, name, server_uuid)
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
    let resolved_addresses = resolve_endpoint_addresses(&endpoint, input.allow_private_endpoint).await
        .map_err(|message| error(StatusCode::BAD_REQUEST, &message))?;
    let controller_id = token()[..32].to_owned();
    let client = http_client_for_endpoint(&endpoint, &resolved_addresses, 10)?;
    let response = client.post(endpoint.join("api/federation/pair").map_err(|_| error(StatusCode::BAD_REQUEST, "آدرس اتصال معتبر نیست."))?)
        .header("X-ARG-Transport", "https")
        .header("X-ARG-Protocol", PROTOCOL_VERSION.to_string())
        .json(&json!({"protocol_version":PROTOCOL_VERSION,"supported_protocol_versions":[PROTOCOL_VERSION],"capabilities_version":CAPABILITIES_VERSION,"controller_id":controller_id,"controller_name":controller_public_name,"server_uuid":controller_server_uuid,"pairing_code":input.pairing_code}))
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
    let capabilities = payload["capabilities"].as_array().map(|items| items.iter().filter_map(Value::as_str).map(str::to_owned).filter(|capability| known_capability(capability)).collect::<Vec<_>>()).unwrap_or_default();
    let supports_negotiation = payload["capabilities_version"].as_u64() == Some(CAPABILITIES_VERSION as u64)
        && capabilities.iter().any(|capability| capability == "capabilities.negotiate.v1");
    let granted_scopes = if supports_negotiation {
        payload["scopes"].as_array().map(|items| items.iter().filter_map(Value::as_str).map(str::to_owned).collect::<Vec<_>>()).unwrap_or_default()
    } else {
        vec!["node.status.read".to_owned(), "node.services.read".to_owned(), "server.credential.rotate".to_owned(), "controller.self_revoke".to_owned(), "workspace.read".to_owned()]
    };
    if granted_scopes.iter().any(|scope| !known_scope(scope)) {
        return Err(error(StatusCode::BAD_GATEWAY, "Node مجوز ناشناخته‌ای در handshake فرستاد؛ اتصال را رد کردم."));
    }
    if !granted_scopes.iter().any(|scope| scope == "node.status.read") || !granted_scopes.iter().any(|scope| scope == "workspace.read") {
        return Err(error(StatusCode::BAD_GATEWAY, "Node مجوزهای پایهٔ خواندن را صادر نکرد؛ اتصال ایجاد نشد."));
    }
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
    let remote_uuid = payload["server_uuid"].as_str().filter(|value| valid_server_uuid(value)).map(str::to_ascii_lowercase);
    let remote_public_name = payload["public_name"].as_str().and_then(|value| validate_public_name(value).ok()).unwrap_or("ARG Server");
    let compatibility_state = if supports_negotiation { "compatible" } else { "legacy-read-only" };
    let workspace_expires_at = payload["workspace_expires_at"].as_i64();
    let controller_token_expires_at = payload["controller_token_expires_at"].as_i64();
    db.execute("INSERT INTO federated_nodes(name,endpoint,node_id,controller_id,protocol_version,created_at,last_seen,server_uuid,public_name,scopes_json,capabilities_json,compatibility_state,connection_state,controller_token_expires_at,workspace_token_expires_at,allow_private_endpoint) VALUES (?1,?2,?3,?4,?5,?6,?6,?7,?8,?9,?10,?11,'healthy',?12,?13,?14)", params![input.name.trim(), endpoint_text, payload["node_id"].as_str().unwrap_or_default(), controller_id, PROTOCOL_VERSION, now(), remote_uuid, remote_public_name, serde_json::to_string(&granted_scopes).unwrap_or_else(|_| "[]".to_owned()), serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_owned()), compatibility_state, controller_token_expires_at, workspace_expires_at, input.allow_private_endpoint]).map_err(db_error)?;
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
        json!({"id":id,"name":input.name.trim(),"public_name":remote_public_name,"server_uuid":remote_uuid,"node_id":payload["node_id"],"protocol":"arg-node/2","compatibility_state":compatibility_state,"scopes":granted_scopes,"capabilities":capabilities,"message":if supports_negotiation { "Node با قابلیت‌های مذاکره‌شده جفت شد." } else { "Node قدیمی به‌صورت فقط‌خواندنی جفت شد؛ برای نوشتن هر دو پنل را به‌روزرسانی و دوباره جفت‌سازی کن." }}),
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
    required_scope: &str,
) -> Result<ControllerIdentity, (StatusCode, Json<Value>)> {
    require_secure_transport(headers)?;
    let raw = bearer(headers)
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "ARG Node authentication required"))?;
    let hash = token_hash(&raw);
    let peer = db.query_row("SELECT id,controller_name,scopes_json,token_expires_at FROM federation_controllers WHERE token_hash=?1 AND enabled=1", [&hash], |row| Ok(ControllerIdentity { id: row.get(0)?, name: row.get(1)?, scopes: decode_scopes(&row.get::<_,String>(2)?), token_expires_at: row.get(3)? })).optional().map_err(db_error)?
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "ARG Node credential is invalid or revoked"))?;
    if peer.token_expires_at.is_some_and(|expires| expires <= now()) {
        return Err(error(StatusCode::UNAUTHORIZED, "اعتبارنامهٔ Federation منقضی شده است؛ اتصال را از پنل کنترل‌کننده بچرخان یا دوباره جفت‌سازی کن."));
    }
    controller_scope(&peer, required_scope)?;
    db.execute(
        "UPDATE federation_controllers SET last_seen=?1 WHERE id=?2",
        params![now(), peer.id],
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
        || (!input.supported_protocol_versions.is_empty() && !input.supported_protocol_versions.contains(&PROTOCOL_VERSION))
        || input.capabilities_version.is_some_and(|version| version > CAPABILITIES_VERSION)
        || input.controller_id.len() != 32
        || !input
            .controller_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || input.controller_name.trim().is_empty()
        || input.controller_name.chars().count() > 80
        || input.server_uuid.as_deref().is_some_and(|uuid| !valid_server_uuid(uuid))
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
    let role = local_federation_role(&db).map_err(db_error)?;
    if !role_can_accept_inbound(&role) {
        return Err(error(StatusCode::CONFLICT, "نقش این سرور اجازهٔ پذیرش اتصال Controller را نمی‌دهد."));
    }
    let server_uuid = local_server_uuid(&db).map_err(db_error)?;
    let public_name = local_public_name(&db).map_err(db_error)?;
    let hash = token_hash(&input.pairing_code);
    let pairing: Option<(i64, String)> = db.query_row("SELECT u.id,p.scopes_json FROM federation_pairing_codes p JOIN users u ON u.username=p.created_by AND u.enabled=1 AND u.role='admin' WHERE p.code_hash=?1 AND p.expires_at>?2", params![hash, now()], |row| Ok((row.get(0)?, row.get(1)?))).optional().map_err(db_error)?;
    let Some((user_id, scopes_json)) = pairing else {
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
    let mut scopes = decode_scopes(&scopes_json);
    scopes.retain(|scope| known_scope(scope));
    if !scopes.iter().any(|scope| scope == "workspace.read") {
        return Err(error(StatusCode::BAD_REQUEST, "کد اتصال scope خواندن فضای کار ندارد."));
    }
    let raw = token();
    let workspace_token = token();
    let controller_token_expires_at = now() + CONTROLLER_TOKEN_TTL;
    let workspace_expires_at = now() + WORKSPACE_SESSION_TTL;
    let name = input.controller_name.trim();
    db.execute("INSERT INTO federation_controllers(controller_id,controller_name,token_hash,created_at,last_seen,enabled,local_label,server_uuid,scopes_json,token_expires_at) VALUES (?1,?2,?3,?4,?4,1,?2,?5,?6,?7)", params![input.controller_id, name, token_hash(&raw), now(), input.server_uuid, serde_json::to_string(&scopes).unwrap_or_else(|_| "[]".to_owned()), controller_token_expires_at]).map_err(db_error)?;
    let workspace_hash = session_hash(&workspace_token);
    let at = now();
    db.execute("INSERT INTO sessions(token_hash,user_id,expires_at,created_at,last_seen,remote_ip,user_agent) VALUES (?1,?2,?3,?4,?4,'federation','ARG remote workspace')", params![workspace_hash, user_id, workspace_expires_at, at]).map_err(db_error)?;
    db.execute(
        "INSERT INTO federation_proxy_sessions(controller_id,token_hash,user_id,scopes_json,expires_at,actor_name) VALUES (?1,?2,?3,?4,?5,?6)",
        params![input.controller_id, workspace_hash, user_id, serde_json::to_string(&scopes).unwrap_or_else(|_| "[]".to_owned()), workspace_expires_at, name],
    )
    .map_err(db_error)?;
    audit(
        &db,
        name,
        "federation.controller.pair",
        "protocol=arg-node/2; scope=authenticated ARG workspace",
    );
    Ok(Json(
        json!({"protocol_version":PROTOCOL_VERSION,"minimum_protocol_version":PROTOCOL_VERSION,"maximum_protocol_version":PROTOCOL_VERSION,"capabilities_version":CAPABILITIES_VERSION,"node_id":node_id,"server_uuid":server_uuid,"public_name":public_name,"controller_id":input.controller_id,"node_token":raw,"controller_token_expires_at":controller_token_expires_at,"workspace_token":workspace_token,"workspace_expires_at":workspace_expires_at,"scopes":scopes,"capabilities":supported_capabilities()}),
    ))
}

pub(super) async fn status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    let identity = authorize_controller(&db, &headers, "node.status.read")?;
    audit(&db, &identity.name, "federation.status.read", "remote node status");
    let server_uuid = local_server_uuid(&db).map_err(db_error)?;
    let public_name = local_public_name(&db).map_err(db_error)?;
    let role = local_federation_role(&db).map_err(db_error)?;
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
        json!({"protocol":"arg-node/2","protocol_version":PROTOCOL_VERSION,"minimum_protocol_version":PROTOCOL_VERSION,"maximum_protocol_version":PROTOCOL_VERSION,"capabilities_version":CAPABILITIES_VERSION,"node_id":node_id,"server_uuid":server_uuid,"public_name":public_name,"role":role,"hostname":hostname,"os":fs::read_to_string("/etc/os-release").unwrap_or_default().lines().find(|line| line.starts_with("PRETTY_NAME=")).map(|line| line.trim_start_matches("PRETTY_NAME=").trim_matches('"')).unwrap_or("Linux"),"architecture":std::env::consts::ARCH,"arg_version":env!("CARGO_PKG_VERSION"),"uptime_seconds":uptime as u64,"load":load,"memory_total_kib":total_kib,"memory_available_kib":available_kib,"capabilities":supported_capabilities()}),
    ))
}

pub(super) async fn services(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        let identity = authorize_controller(&db, &headers, "node.services.read")?;
        audit(&db, &identity.name, "federation.services.read", "remote service inventory");
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
        authorize_controller(&db, &headers, "node.services.manage")?.name
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
    required_scope: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let (endpoint, remote_id, last_token, allow_private) = {
        let db = state.lock().unwrap();
        let (endpoint, remote_id, scopes_json, compatibility_state, capabilities_json, allow_private): (String, String, String, String, String, bool) = db
            .query_row(
                "SELECT endpoint,controller_id,scopes_json,compatibility_state,capabilities_json,allow_private_endpoint!=0 FROM federated_nodes WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .map_err(|_| error(StatusCode::NOT_FOUND, "Node پیدا نشد."))?;
        if !decode_scopes(&scopes_json).iter().any(|scope| scope == required_scope) {
            return Err(error(StatusCode::FORBIDDEN, "این Node به پنل فعلی مجوز لازم را نداده است؛ دسترسی را بررسی یا دوباره جفت‌سازی کن."));
        }
        if required_scope.starts_with("workspace.") {
            let capabilities = decode_scopes(&capabilities_json);
            if compatibility_state != "compatible" || !capabilities.iter().any(|capability| capability == "workspace.api.proxy") {
                return Err(error(StatusCode::UPGRADE_REQUIRED, "نسخهٔ Node از تونل امن فضای کار پشتیبانی نمی‌کند؛ هر دو پنل را به‌روزرسانی و دوباره جفت‌سازی کن."));
            }
        }
        let token_path = token_path(&db, id)?;
        (endpoint, remote_id, read_node_token(&token_path)?, allow_private)
    };
    let endpoint =
        validate_endpoint(&endpoint).map_err(|message| error(StatusCode::BAD_GATEWAY, &message))?;
    let addresses = resolve_endpoint_addresses(&endpoint, allow_private).await.map_err(|message| error(StatusCode::BAD_GATEWAY, &message))?;
    let url = endpoint
        .join(path)
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "مسیر پروتکل Node نامعتبر است."))?;
    let client = http_client_for_endpoint(&endpoint, &addresses, 10)?;
    let mut request = client
        .request(method, url)
        .bearer_auth(last_token)
        .header("X-ARG-Transport", "https")
        .header("X-ARG-Protocol", PROTOCOL_VERSION.to_string())
        .header("X-ARG-Controller-ID", remote_id);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(_) => {
            mark_node_connection(state, id, "offline", Some("ارتباط برقرار نشد؛ شبکه، سرویس و گواهی HTTPS را بررسی کن."));
            return Err(error(StatusCode::BAD_GATEWAY, "ارتباط با Node برقرار نشد؛ وضعیت شبکه، سرویس و گواهی TLS را بررسی کن."));
        }
    };
    let status = response.status();
    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(_) => {
            mark_node_connection(state, id, "degraded", Some("پاسخ Node با قالب مورد انتظار سازگار نیست."));
            return Err(error(StatusCode::BAD_GATEWAY, "پاسخ Node قابل‌خواندن نیست."));
        }
    };
    if !status.is_success() {
        let connection_state = if status == StatusCode::UPGRADE_REQUIRED { "incompatible" } else if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN { "untrusted" } else { "degraded" };
        let detail = value["error"].as_str().unwrap_or("Node عملیات را نپذیرفت.").chars().take(240).collect::<String>();
        mark_node_connection(state, id, connection_state, Some(&detail));
        return Err(error(
            StatusCode::BAD_GATEWAY,
            value["error"].as_str().unwrap_or("Node عملیات را نپذیرفت."),
        ));
    }
    let db = state.lock().unwrap();
    db.execute(
        "UPDATE federated_nodes SET last_seen=?1,last_check=?1,connection_state='healthy',last_error=NULL WHERE id=?2",
        params![now(), id],
    )
    .map_err(db_error)?;
    Ok(value)
}

fn mark_node_connection(state: &AppState, id: i64, connection_state: &str, last_error: Option<&str>) {
    if !["healthy", "degraded", "offline", "untrusted", "incompatible"].contains(&connection_state) { return; }
    let db = state.lock().unwrap();
    let _ = db.execute("UPDATE federated_nodes SET connection_state=?1,last_check=?2,last_error=?3 WHERE id=?4", params![connection_state, now(), last_error, id]);
}

async fn ensure_workspace_session(state: &AppState, id: i64) -> Result<String, (StatusCode, Json<Value>)> {
    let (path, expires_at, scopes_json) = {
        let db = state.lock().unwrap();
        let (expires_at, scopes_json): (Option<i64>, String) = db.query_row("SELECT workspace_token_expires_at,scopes_json FROM federated_nodes WHERE id=?1", [id], |row| Ok((row.get(0)?,row.get(1)?))).optional().map_err(db_error)?.ok_or_else(|| error(StatusCode::NOT_FOUND, "Node پیدا نشد."))?;
        (workspace_token_path(&db, id)?, expires_at, scopes_json)
    };
    if expires_at.map_or(false, |expires| expires > now() + 15 * 60) {
        return read_node_token(&path);
    }
    if !decode_scopes(&scopes_json).iter().any(|scope| scope == "workspace.session.renew") {
        return Err(error(StatusCode::UNAUTHORIZED, "نشست فضای کار منقضی شده و این اتصال نسخهٔ تمدید امن را پشتیبانی نمی‌کند؛ Node را دوباره جفت‌سازی کن."));
    }
    let response = remote_node_request(state, id, "workspace.session.renew", reqwest::Method::POST, "api/federation/workspace/renew", None).await?;
    let token = response["workspace_token"].as_str().unwrap_or_default();
    let expires_at = response["expires_at"].as_i64().unwrap_or_default();
    if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) || expires_at <= now() {
        mark_node_connection(state, id, "degraded", Some("پاسخ تمدید نشست فضای کار نامعتبر بود."));
        return Err(error(StatusCode::BAD_GATEWAY, "پاسخ تمدید نشست فضای کار معتبر نیست."));
    }
    write_node_token(&path, token)?;
    let db = state.lock().unwrap();
    db.execute("UPDATE federated_nodes SET workspace_token_expires_at=?1,last_check=?2,connection_state='healthy',last_error=NULL WHERE id=?3", params![expires_at,now(),id]).map_err(db_error)?;
    Ok(token.to_owned())
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
    let value = remote_node_request(
        &state,
        id,
        "node.status.read",
        reqwest::Method::GET,
        "api/federation/status",
        None,
    )
    .await?;
    if let Some(uuid) = value["server_uuid"].as_str().filter(|uuid| valid_server_uuid(uuid)) {
        let db = state.lock().unwrap();
        let public_name = value["public_name"].as_str().and_then(|name| validate_public_name(name).ok());
        let protocol_version = value["protocol_version"].as_u64().unwrap_or(0) as u32;
        let capabilities = value["capabilities"].as_array().map(|items| items.iter().filter_map(Value::as_str).filter(|capability| known_capability(capability)).map(str::to_owned).collect::<Vec<_>>()).unwrap_or_default();
        let negotiated = value["capabilities_version"].as_u64() == Some(CAPABILITIES_VERSION as u64)
            && protocol_version == PROTOCOL_VERSION;
        let compatibility = if negotiated { "compatible" } else { "legacy-read-only" };
        if let Some(public_name) = public_name {
            db.execute("UPDATE federated_nodes SET server_uuid=?1,public_name=?2,capabilities_json=?3,compatibility_state=?4,protocol_version=?5 WHERE id=?6", params![uuid.to_ascii_lowercase(), public_name, serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_owned()), compatibility, protocol_version as i64, id]).map_err(db_error)?;
        } else {
            db.execute("UPDATE federated_nodes SET server_uuid=?1,capabilities_json=?2,compatibility_state=?3,protocol_version=?4 WHERE id=?5", params![uuid.to_ascii_lowercase(), serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_owned()), compatibility, protocol_version as i64, id]).map_err(db_error)?;
        }
    }
    Ok(Json(value))
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
        "node.services.read",
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
        "node.services.manage",
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

pub(super) async fn rotate_node_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    check_origin(&headers)?;
    let (actor, endpoint, old_token, pending_token, current_path, pending_path, allow_private) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let (endpoint, scopes_json, allow_private): (String, String, bool) = db.query_row("SELECT endpoint,scopes_json,allow_private_endpoint!=0 FROM federated_nodes WHERE id=?1", [id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional().map_err(db_error)?.ok_or_else(|| error(StatusCode::NOT_FOUND, "Node پیدا نشد."))?;
        if !decode_scopes(&scopes_json).iter().any(|scope| scope == "server.credential.rotate") {
            return Err(error(StatusCode::FORBIDDEN, "این اتصال اجازهٔ چرخاندن کلید را ندارد؛ دوباره جفت‌سازی کن."));
        }
        let current_path = token_path(&db, id)?;
        let pending_path = current_path.with_extension("pending");
        let old_token = read_node_token(&current_path)?;
        let pending_token = match read_node_token(&pending_path) {
            Ok(value) => value,
            Err(_) => {
                let value = token();
                write_node_token(&pending_path, &value)?;
                value
            }
        };
        (actor, endpoint, old_token, pending_token, current_path, pending_path, allow_private)
    };
    let endpoint = validate_endpoint(&endpoint).map_err(|message| error(StatusCode::BAD_GATEWAY, &message))?;
    let addresses = resolve_endpoint_addresses(&endpoint, allow_private).await.map_err(|message| error(StatusCode::BAD_GATEWAY, &message))?;
    let url = endpoint.join("api/federation/controller/rotate").map_err(|_| error(StatusCode::BAD_GATEWAY, "مسیر چرخش کلید معتبر نیست."))?;
    let client = http_client_for_endpoint(&endpoint, &addresses, 10)?;
    let send = |credential: String| {
        let client = client.clone();
        let url = url.clone();
        let new_token = pending_token.clone();
        async move {
            client.post(url).header("X-ARG-Transport", "https").header("X-ARG-Protocol", PROTOCOL_VERSION.to_string()).bearer_auth(credential).json(&json!({"new_token":new_token})).send().await
        }
    };
    let first = send(old_token.clone()).await;
    let response = match first {
        Ok(response) if response.status().is_success() => response,
        _ => match send(pending_token.clone()).await {
            Ok(response) if response.status().is_success() => response,
            _ => match send(old_token).await {
                Ok(response) if response.status().is_success() => response,
                _ => return Err(error(StatusCode::BAD_GATEWAY, "چرخش کلید کامل نشد؛ فایل pending امن باقی مانده و با اجرای دوباره عملیات همان کلید با تلاش ایمن پیگیری می‌شود.")),
            }
        }
    };
    let payload: Value = response.json().await.map_err(|_| error(StatusCode::BAD_GATEWAY, "پاسخ چرخش کلید معتبر نیست؛ کلید pending برای بازیابی نگهداری شد."))?;
    let expires_at = payload["expires_at"].as_i64().unwrap_or(now() + CONTROLLER_TOKEN_TTL);
    write_node_token(&current_path, &pending_token)?;
    let _ = fs::remove_file(&pending_path);
    let db = state.lock().unwrap();
    db.execute("UPDATE federated_nodes SET controller_token_expires_at=?1,last_check=?2,connection_state='healthy',last_error=NULL WHERE id=?3", params![expires_at,now(),id]).map_err(db_error)?;
    audit(&db, &actor, "federation.node.credential.rotate", &format!("node_id={id}"));
    Ok(Json(json!({"rotated":true,"expires_at":expires_at,"message":"کلید Node با موفقیت چرخانده شد."})))
}

/// Authenticated controller-side reverse proxy for the selected ARG workspace.
/// The remote admin session stays in a root-only file and is never sent to the browser.
pub(super) async fn proxy_workspace_api(
    State(state): State<AppState>,
    Path((workspace_key, remote_path)): Path<(String, String)>,
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
    let (id, endpoint, allow_private) = {
        let db = state.lock().unwrap();
        let id = match resolve_workspace_node_id(&db, &workspace_key) {
            Ok(value) => value,
            Err(failure) => return failure.into_response(),
        };
        let (endpoint, scopes_json, capabilities_json, compatibility_state, allow_private): (String, String, String, String, bool) = match db.query_row(
            "SELECT endpoint,scopes_json,capabilities_json,compatibility_state,allow_private_endpoint!=0 FROM federated_nodes WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
        ) {
            Ok(value) => value,
            Err(_) => return error(StatusCode::NOT_FOUND, "Node پیدا نشد.").into_response(),
        };
        let required = if method == reqwest::Method::GET || method == reqwest::Method::HEAD { "workspace.read" } else { "workspace.write" };
        if !decode_scopes(&scopes_json).iter().any(|scope| scope == required) {
            return error(StatusCode::FORBIDDEN, "مجوز این نشست برای این عملیات کافی نیست.").into_response();
        }
        if compatibility_state != "compatible" || !decode_scopes(&capabilities_json).iter().any(|capability| capability == "workspace.api.proxy") {
            return error(StatusCode::UPGRADE_REQUIRED, "این Node از تونل امن فضای کار پشتیبانی نمی‌کند؛ نسخه‌ها را به‌روز و اتصال را دوباره جفت‌سازی کن.").into_response();
        }
        (id, endpoint, allow_private)
    };
    let token = match ensure_workspace_session(&state, id).await {
        Ok(token) => token,
        Err(failure) => return failure.into_response(),
    };
    let Ok(endpoint) = validate_endpoint(&endpoint) else {
        return error(StatusCode::BAD_GATEWAY, "آدرس Node ذخیره‌شده معتبر نیست.").into_response();
    };
    let addresses = match resolve_endpoint_addresses(&endpoint, allow_private).await {
        Ok(addresses) => addresses,
        Err(message) => return error(StatusCode::BAD_GATEWAY, &message).into_response(),
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
    let client = match workspace_http_client(&endpoint, &addresses) {
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
            mark_node_connection(&state, id, "offline", Some("تونل فضای کار در دسترس نیست."));
            return error(
                StatusCode::BAD_GATEWAY,
                "ارتباط با فضای کار Node برقرار نشد؛ وضعیت شبکه و گواهی HTTPS را بررسی کن.",
            )
            .into_response()
        }
    };
    let status = remote.status();
    let headers = remote.headers().clone();
    if !status.is_success() {
        let connection_state = if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN { "untrusted" } else if status == StatusCode::UPGRADE_REQUIRED { "incompatible" } else { "degraded" };
        mark_node_connection(&state, id, connection_state, Some("درخواست فضای کار توسط Node رد شد."));
    }
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
    Path(workspace_key): Path<String>,
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
    let root_ticket = cookie_named(&headers, "arg_root_ticket");
    {
        let db = state.lock().unwrap();
        if require_admin(&db, &headers).is_err() {
            return (StatusCode::FORBIDDEN, "Administrator access required").into_response();
        }
    }
    let (id, endpoint, allow_private) = {
        let db = state.lock().unwrap();
        let id = match resolve_workspace_node_id(&db, &workspace_key) {
            Ok(value) => value,
            Err(_) => return (StatusCode::NOT_FOUND, "Node not found").into_response(),
        };
        let (endpoint, scopes_json, capabilities_json, compatibility_state, allow_private): (String, String, String, String, bool) = match db.query_row(
            "SELECT endpoint,scopes_json,capabilities_json,compatibility_state,allow_private_endpoint!=0 FROM federated_nodes WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
        ) {
            Ok(value) => value,
            Err(_) => return (StatusCode::NOT_FOUND, "Node not found").into_response(),
        };
        let terminal_scope = if root_ticket.is_some() { "workspace.root_terminal" } else { "workspace.terminal" };
        if !decode_scopes(&scopes_json).iter().any(|scope| scope == terminal_scope) {
            return (StatusCode::FORBIDDEN, "Remote terminal is not permitted for this pairing").into_response();
        }
        if compatibility_state != "compatible" || !decode_scopes(&capabilities_json).iter().any(|capability| capability == "workspace.terminal") {
            return (StatusCode::UPGRADE_REQUIRED, "Update and re-pair this Node to enable the secure terminal").into_response();
        }
        (id, endpoint, allow_private)
    };
    let session = match ensure_workspace_session(&state, id).await {
        Ok(value) => value,
        Err(_) => return (StatusCode::BAD_GATEWAY, "Remote workspace session could not be renewed").into_response(),
    };
    let parsed_endpoint = match validate_endpoint(&endpoint) {
        Ok(value) => value,
        Err(_) => return (StatusCode::BAD_GATEWAY, "Remote Node endpoint is invalid").into_response(),
    };
    let addresses = match resolve_endpoint_addresses(&parsed_endpoint, allow_private).await {
        Ok(value) => value,
        Err(_) => return (StatusCode::BAD_GATEWAY, "Remote Node DNS or network target failed security validation").into_response(),
    };
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
        let Some(address) = addresses.first().copied() else {
            let _ = browser.send(axum::extract::ws::Message::text("\r\nRemote Node has no validated address.\r\n")).await;
            return;
        };
        let socket = match tokio::net::TcpStream::connect(address).await {
            Ok(socket) => socket,
            Err(_) => {
                let _ = browser.send(axum::extract::ws::Message::text("\r\nCould not connect to the validated remote address.\r\n")).await;
                let _ = browser.close().await;
                return;
            }
        };
        let Ok((remote, _)) = tokio_tungstenite::client_async_tls_with_config(request, socket, None, None).await else {
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
        "controller.self_revoke",
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
    let identity = authorize_controller(&db, &headers, "controller.self_revoke")?;
    db.execute("DELETE FROM federation_controllers WHERE id=?1", [identity.id])
        .map_err(db_error)?;
    audit(
        &db,
        &identity.name,
        "federation.controller.revoke",
        "self-revoked from controller",
    );
    Ok(Json(json!({"revoked":true})))
}

pub(super) async fn rotate_controller_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<RotateCredentialRequest>,
) -> ApiResult {
    require_secure_transport(&headers)?;
    if input.new_token.len() != 64 || !input.new_token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(error(StatusCode::BAD_REQUEST, "کلید جدید قالب معتبر ندارد."));
    }
    let supplied = bearer(&headers).ok_or_else(|| error(StatusCode::UNAUTHORIZED, "اعتبارنامهٔ Node لازم است."))?;
    let supplied_hash = token_hash(&supplied);
    let new_hash = token_hash(&input.new_token);
    let db = state.lock().unwrap();
    let current: Option<(i64, String, String, Option<i64>)> = db.query_row(
        "SELECT id,controller_name,scopes_json,token_expires_at FROM federation_controllers WHERE token_hash=?1 AND enabled=1",
        [&supplied_hash],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
    ).optional().map_err(db_error)?;
    let identity = if let Some((id, name, scopes_json, expires_at)) = current {
        let identity = ControllerIdentity { id, name, scopes: decode_scopes(&scopes_json), token_expires_at: expires_at };
        if identity.token_expires_at.map_or(false, |expires| expires <= now()) {
            return Err(error(StatusCode::UNAUTHORIZED, "اعتبارنامهٔ فعلی منقضی شده است."));
        }
        controller_scope(&identity, "server.credential.rotate")?;
        if supplied_hash != new_hash {
            db.execute("UPDATE federation_controllers SET token_hash=?1,token_expires_at=?2 WHERE id=?3", params![new_hash, now() + CONTROLLER_TOKEN_TTL, id]).map_err(db_error)?;
        } else {
            db.execute("UPDATE federation_controllers SET token_expires_at=?1 WHERE id=?2", params![now() + CONTROLLER_TOKEN_TTL, id]).map_err(db_error)?;
        }
        identity
    } else {
        let identity = db.query_row(
            "SELECT id,controller_name,scopes_json,token_expires_at FROM federation_controllers WHERE token_hash=?1 AND enabled=1",
            [&new_hash],
            |row| Ok(ControllerIdentity { id: row.get(0)?, name: row.get(1)?, scopes: decode_scopes(&row.get::<_,String>(2)?), token_expires_at: row.get(3)? }),
        ).optional().map_err(db_error)?.ok_or_else(|| error(StatusCode::UNAUTHORIZED, "اعتبارنامهٔ Federation نامعتبر یا لغوشده است."))?;
        controller_scope(&identity, "server.credential.rotate")?;
        db.execute("UPDATE federation_controllers SET token_expires_at=?1 WHERE id=?2", params![now() + CONTROLLER_TOKEN_TTL, identity.id]).map_err(db_error)?;
        identity
    };
    let expires_at = now() + CONTROLLER_TOKEN_TTL;
    audit(&db, &identity.name, "federation.controller.credential.rotate", "credential rotated; raw secret omitted");
    Ok(Json(json!({"rotated":true,"expires_at":expires_at,"message":"کلید اتصال چرخانده شد."})))
}

pub(super) async fn renew_workspace_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult {
    let db = state.lock().unwrap();
    let identity = authorize_controller(&db, &headers, "workspace.session.renew")?;
    let (user_id, scopes_json): (i64, String) = db.query_row(
        "SELECT user_id,scopes_json FROM federation_proxy_sessions WHERE controller_id=(SELECT controller_id FROM federation_controllers WHERE id=?1) ORDER BY expires_at DESC LIMIT 1",
        [identity.id],
        |row| Ok((row.get(0)?,row.get(1)?)),
    ).optional().map_err(db_error)?.ok_or_else(|| error(StatusCode::CONFLICT, "نشست فضای کار موجود نیست؛ اتصال را دوباره جفت‌سازی کن."))?;
    let mut scopes = decode_scopes(&scopes_json);
    scopes.retain(|scope| known_scope(scope));
    if !scopes.iter().any(|scope| scope == "workspace.read") {
        return Err(error(StatusCode::FORBIDDEN, "اتصال موجود اجازهٔ فضای کار را ندارد."));
    }
    let workspace_token = token();
    let expires_at = now() + WORKSPACE_SESSION_TTL;
    let hash = session_hash(&workspace_token);
    let at = now();
    let user_enabled: bool = db.query_row("SELECT enabled=1 AND role='admin' FROM users WHERE id=?1", [user_id], |row| row.get(0)).optional().map_err(db_error)?.unwrap_or(false);
    if !user_enabled { return Err(error(StatusCode::CONFLICT, "کاربر مدیر پیوندخورده به این اتصال غیرفعال است؛ دوباره جفت‌سازی کن.")); }
    db.execute("INSERT INTO sessions(token_hash,user_id,expires_at,created_at,last_seen,remote_ip,user_agent) VALUES (?1,?2,?3,?4,?4,'federation','ARG workspace renewal')", params![hash,user_id,expires_at,at]).map_err(db_error)?;
    db.execute("INSERT INTO federation_proxy_sessions(controller_id,token_hash,user_id,scopes_json,expires_at,actor_name) VALUES ((SELECT controller_id FROM federation_controllers WHERE id=?1),?2,?3,?4,?5,?6)", params![identity.id,hash,user_id,serde_json::to_string(&scopes).unwrap_or_else(|_| "[]".to_owned()),expires_at,identity.name]).map_err(db_error)?;
    audit(&db, &identity.name, "federation.workspace.session.renew", "new scoped workspace session issued");
    Ok(Json(json!({"workspace_token":workspace_token,"expires_at":expires_at,"scopes":scopes})))
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

pub(super) async fn rename_controller_admin(
    State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i64>, Json(input): Json<ControllerNameRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let name = validate_public_name(&input.name)?;
    let changed = db.execute("UPDATE federation_controllers SET local_label=?1 WHERE id=?2 AND enabled=1", params![name, id]).map_err(db_error)?;
    if changed == 0 { return Err(error(StatusCode::NOT_FOUND, "پنل متصل پیدا نشد.")); }
    audit(&db, &actor.username, "federation.controller.label", &format!("controller_id={id}; label={name}"));
    Ok(Json(json!({"name":name,"message":"برچسب محلی پنل ذخیره شد."})))
}

pub(super) async fn update_public_name(
    State(state): State<AppState>, headers: HeaderMap, Json(input): Json<PublicNameRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let name = validate_public_name(&input.name)?.to_owned();
    let (actor, peers, total) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?;
        db.execute("INSERT INTO settings(key,value) VALUES('federation_public_name',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [&name]).map_err(db_error)?;
        audit(&db, &actor.username, "federation.server.public_name", &name);
        let total: usize = db.query_row("SELECT COUNT(*) FROM federated_nodes", [], |row| row.get::<_, i64>(0)).map_err(db_error)?.max(0) as usize;
        let mut statement = db.prepare("SELECT id,endpoint,scopes_json,allow_private_endpoint!=0 FROM federated_nodes ORDER BY id LIMIT 100").map_err(db_error)?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, bool>(3)?))).map_err(db_error)?;
        let mut peers = Vec::new();
        for row in rows {
            let (id, endpoint, scopes, allow_private) = row.map_err(db_error)?;
            if !decode_scopes(&scopes).iter().any(|scope| scope == "server.public_name.sync") { continue; }
            if let Ok(path) = token_path(&db, id) {
                if let Ok(token) = read_node_token(&path) { peers.push((id, endpoint, token, allow_private)); }
            }
        }
        (actor, peers, total)
    };
    let peers = peers.into_iter().take(32).collect::<Vec<_>>();
    use futures_util::StreamExt as _;
    let results = futures_util::stream::iter(peers.into_iter().map(|(id, endpoint, token, allow_private)| {
        let name = name.clone();
        async move {
            let Ok(base) = validate_endpoint(&endpoint) else { return false };
            let Ok(addresses) = resolve_endpoint_addresses(&base, allow_private).await else { return false };
            let Ok(client) = http_client_for_endpoint(&base, &addresses, 10) else { return false };
            let Ok(url) = base.join("api/federation/controller/name") else { return false };
            client.put(url).header("X-ARG-Transport", "https").header("X-ARG-Protocol", PROTOCOL_VERSION.to_string()).bearer_auth(token)
                .json(&json!({"name":name,"request_id":format!("public-name-{id}-{}", now())}))
                .send().await.is_ok_and(|response| response.status().is_success())
        }
    })).buffer_unordered(16).collect::<Vec<_>>().await;
    let synced = results.into_iter().filter(|success| *success).count();
    let pending = total.saturating_sub(synced);
    let message = if pending == 0 { "نام عمومی ذخیره و با پنل‌های متصل همگام شد.".to_owned() }
        else { format!("نام عمومی ذخیره شد؛ همگام‌سازی با {pending} پنل برقرار نشد. پس از برقراری ارتباط، دوباره ذخیره کن.") };
    audit(&state.lock().unwrap(), &actor.username, "federation.server.public_name.sync", &format!("synced={synced}; pending={pending}"));
    Ok(Json(json!({"name":name,"synced":synced,"pending":pending,"message":message})))
}

pub(super) async fn sync_controller_public_name(
    State(state): State<AppState>, headers: HeaderMap, Json(input): Json<PublicNameRequest>,
) -> ApiResult {
    let name = validate_public_name(&input.name)?;
    let db = state.lock().unwrap();
    let identity = authorize_controller(&db, &headers, "server.public_name.sync")?;
    db.execute("UPDATE federation_controllers SET controller_name=?1 WHERE id=?2", params![name, identity.id]).map_err(db_error)?;
    audit(&db, &identity.name, "federation.controller.public_name.sync", name);
    Ok(Json(json!({"name":name,"updated":true})))
}

#[cfg(test)]
mod tests {
    use super::{address_is_private, resolve_workspace_node_id, role_valid, scopes_for_mode, session_scope_for, valid_server_uuid, valid_workspace_api_path, validate_endpoint, validate_public_name, workspace_key};
    use rusqlite::Connection;
    #[test]
    fn server_identity_and_public_names_are_validated() {
        assert!(valid_server_uuid("123e4567-e89b-42d3-a456-426614174000"));
        assert!(!valid_server_uuid("legacy-node-id"));
        assert!(validate_public_name(" VPS West ").is_ok());
        assert!(validate_public_name("  ").is_err());
        assert!(validate_public_name(&"x".repeat(81)).is_err());
        assert!(validate_public_name("name\nforged").is_err());
    }

    #[test]
    fn workspace_keys_resolve_stable_identity_and_legacy_pairings() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE federated_nodes(id INTEGER PRIMARY KEY,server_uuid TEXT); INSERT INTO federated_nodes VALUES(7,NULL); INSERT INTO federated_nodes VALUES(8,'123e4567-e89b-42d3-a456-426614174000');").unwrap();

        assert_eq!(workspace_key(None, 7), "legacy:7");
        assert_eq!(workspace_key(Some("123e4567-e89b-42d3-a456-426614174000"), 8), "123e4567-e89b-42d3-a456-426614174000");
        assert_eq!(resolve_workspace_node_id(&db, "legacy:7").unwrap(), 7);
        assert_eq!(resolve_workspace_node_id(&db, "123e4567-e89b-42d3-a456-426614174000").unwrap(), 8);
        assert!(resolve_workspace_node_id(&db, "legacy:8").is_err());
        assert!(resolve_workspace_node_id(&db, "not-a-server-id").is_err());
    }
    #[test]
    fn node_endpoint_requires_a_clean_https_origin() {
        assert!(validate_endpoint("https://node.example.test/").is_ok());
        assert!(validate_endpoint("http://node.example.test").is_err());
        assert!(validate_endpoint("https://user:pass@node.example.test").is_err());
        assert!(validate_endpoint("https://node.example.test/api").is_err());
        assert!(validate_endpoint("https://127.0.0.1").is_err());
        assert!(validate_endpoint("https://169.254.169.254").is_err());
        assert!(validate_endpoint("https://metadata.google.internal").is_err());
    }

    #[test]
    fn federation_roles_and_pairing_scopes_are_explicit() {
        assert!(role_valid("standalone"));
        assert!(role_valid("member"));
        assert!(role_valid("controller-capable"));
        assert!(role_valid("both"));
        assert!(!role_valid("admin"));
        let read_only = scopes_for_mode("read-only").unwrap();
        assert!(read_only.contains(&"workspace.read"));
        assert!(!read_only.contains(&"workspace.write"));
        assert!(!read_only.contains(&"server.credential.rotate"));
        let full = scopes_for_mode("full").unwrap();
        assert!(full.contains(&"workspace.write"));
        assert!(full.contains(&"workspace.root_terminal"));
        assert!(full.contains(&"server.credential.rotate"));
        assert!(scopes_for_mode("unrestricted").is_none());
    }

    #[test]
    fn proxied_session_permissions_follow_action_type() {
        assert_eq!(session_scope_for("/api/dashboard", &axum::http::Method::GET), "workspace.read");
        assert_eq!(session_scope_for("/api/files/action", &axum::http::Method::POST), "workspace.write");
        assert_eq!(session_scope_for("/api/terminal", &axum::http::Method::GET), "workspace.terminal");
        assert_eq!(session_scope_for("/api/terminal/root/authorize", &axum::http::Method::POST), "workspace.root_terminal");
    }

    #[test]
    fn private_address_classification_covers_metadata_ranges() {
        assert!(address_is_private("10.1.2.3".parse().unwrap()));
        assert!(address_is_private("172.16.2.3".parse().unwrap()));
        assert!(address_is_private("192.168.1.2".parse().unwrap()));
        assert!(address_is_private("169.254.169.254".parse().unwrap()));
        assert!(address_is_private("::1".parse().unwrap()));
        assert!(!address_is_private("1.1.1.1".parse().unwrap()));
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
