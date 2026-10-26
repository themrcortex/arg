use super::*;

const APP_FRAMEWORKS: &[&str] = &["laravel", "node", "python", "wordpress", "static", "custom"];
const APP_SOURCES: &[&str] = &[
    "manual",
    "git",
    "archive",
    "docker",
    "existing-directory",
    "image",
];
static APP_REGISTRY_OPERATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Deserialize)]
pub(super) struct AppListQuery {
    #[serde(default)]
    include_deleted: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AppInput {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_framework")]
    framework: String,
    #[serde(default = "default_runtime_version")]
    runtime_version: String,
    #[serde(default = "default_runtime_version")]
    build_runtime_version: String,
    #[serde(default = "default_source")]
    source_type: String,
    #[serde(default)]
    domain: String,
    #[serde(default)]
    listen_port: Option<u16>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default = "default_desired_state")]
    desired_state: String,
}

#[derive(Deserialize)]
pub(super) struct AppUpdate {
    expected_version: i64,
    #[serde(flatten)]
    app: AppInput,
}

#[derive(Deserialize)]
pub(super) struct AppVersionRequest {
    expected_version: i64,
}

fn default_framework() -> String {
    "custom".to_owned()
}
fn default_source() -> String {
    "manual".to_owned()
}
fn default_runtime_version() -> String {
    "system".to_owned()
}
fn default_desired_state() -> String {
    "none".to_owned()
}

fn valid_name(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.chars().count() <= 80 && !value.chars().any(char::is_control)
}

fn valid_domain(value: &str) -> bool {
    let value = value.trim().trim_end_matches('.');
    if value.is_empty() || value.len() > 253 || value.contains(['*', '/', ':', '@', '\\']) {
        return false;
    }
    if value.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    value.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.as_bytes()[0].is_ascii_alphanumeric()
            && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn normalized_domain(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('.');
    if value.is_empty() {
        None
    } else {
        Some(value.to_ascii_lowercase())
    }
}

fn validate_input(input: &AppInput) -> Result<(String, Option<String>), (StatusCode, Json<Value>)> {
    let name = input.name.trim();
    if !valid_name(name) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام اپلیکیشن باید بین ۱ تا ۸۰ نویسه باشد.",
        ));
    }
    if input.description.chars().count() > 1000
        || input
            .description
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "توضیحات بیش از حد طولانی یا دارای نویسهٔ کنترلی است.",
        ));
    }
    if !APP_FRAMEWORKS.contains(&input.framework.as_str()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "قالب اپلیکیشن پشتیبانی نمی‌شود.",
        ));
    }
    if !APP_SOURCES.contains(&input.source_type.as_str()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نوع منبع اپلیکیشن پشتیبانی نمی‌شود.",
        ));
    }
    if !valid_runtime_version(&input.runtime_version) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نسخهٔ Runtime باید system یا شماره نسخهٔ عددی باشد.",
        ));
    }
    if !valid_runtime_version(&input.build_runtime_version) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نسخهٔ Runtime زمان ساخت باید system یا شماره نسخهٔ عددی باشد.",
        ));
    }
    if !["none", "running", "stopped"].contains(&input.desired_state.as_str()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "وضعیت هدف باید اجرا، توقف یا تعیین‌نشده باشد.",
        ));
    }
    if input.labels.len() > 20
        || input.labels.iter().any(|label| {
            let label = label.trim();
            label.is_empty() || label.chars().count() > 40 || label.chars().any(char::is_control)
        })
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "حداکثر ۲۰ برچسب ۴۰ نویسه‌ای قابل ثبت است.",
        ));
    }
    let mut unique_labels = HashSet::new();
    if input
        .labels
        .iter()
        .map(|label| label.trim().to_lowercase())
        .any(|label| !unique_labels.insert(label))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "برچسب تکراری قابل ثبت نیست.",
        ));
    }
    let domain = normalized_domain(&input.domain);
    if domain
        .as_deref()
        .is_some_and(|domain| !valid_domain(domain))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "دامنه باید hostname یا IP معتبر باشد و نباید پروتکل یا پورت داشته باشد.",
        ));
    }
    Ok((name.to_owned(), domain))
}

fn valid_runtime_version(value: &str) -> bool {
    value == "system"
        || (!value.is_empty()
            && value.len() <= 32
            && value.contains('.')
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'.')
            && value.split('.').all(|part| !part.is_empty()))
}

fn domain_patterns_overlap(left: &str, right: &str) -> bool {
    fn matches(pattern: &str, domain: &str) -> bool {
        if let Some(suffix) = pattern.strip_prefix("*.") {
            domain.ends_with(&format!(".{suffix}")) && domain != suffix
        } else if let Some(suffix) = pattern.strip_prefix('*') {
            domain.ends_with(suffix) && domain.len() > suffix.len()
        } else if let Some(prefix) = pattern.strip_suffix(".*") {
            domain.starts_with(&format!("{prefix}.")) && domain.len() > prefix.len() + 1
        } else {
            pattern.eq_ignore_ascii_case(domain)
        }
    }
    matches(left, right) || matches(right, left)
}

fn random_uuid() -> String {
    let mut bytes = [0_u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn read_apps(
    db: &Connection,
    owner: &str,
    include_deleted: bool,
) -> Result<Vec<Value>, rusqlite::Error> {
    let sql = if include_deleted {
        "SELECT id,name,description,framework,runtime_version,build_runtime_version,source_type,domain,listen_port,labels_json,desired_state,observed_state,observed_at,lifecycle_state,reconcile_policy,version,created_by,created_at,updated_at,deleted_at FROM applications WHERE owner_server_uuid=?1 ORDER BY deleted_at IS NOT NULL,name COLLATE NOCASE"
    } else {
        "SELECT id,name,description,framework,runtime_version,build_runtime_version,source_type,domain,listen_port,labels_json,desired_state,observed_state,observed_at,lifecycle_state,reconcile_policy,version,created_by,created_at,updated_at,deleted_at FROM applications WHERE owner_server_uuid=?1 AND deleted_at IS NULL ORDER BY name COLLATE NOCASE"
    };
    let mut statement = db.prepare(sql)?;
    let rows = statement.query_map([owner], |row| {
        let labels_json: String = row.get(9)?;
        let labels = serde_json::from_str::<Vec<String>>(&labels_json).unwrap_or_default();
        Ok(json!({
            "id": row.get::<_,String>(0)?, "owner_server_uuid": owner, "name": row.get::<_,String>(1)?,
            "description": row.get::<_,String>(2)?, "framework": row.get::<_,String>(3)?,
            "runtime_version": row.get::<_,String>(4)?,
            "build_runtime_version": row.get::<_,String>(5)?,
            "source_type": row.get::<_,String>(6)?, "domain": row.get::<_,Option<String>>(7)?,
            "listen_port": row.get::<_,Option<i64>>(8)?, "labels": labels,
            "desired_state": row.get::<_,String>(10)?, "observed_state": row.get::<_,String>(11)?,
            "observed_at": row.get::<_,Option<i64>>(12)?, "lifecycle_state": row.get::<_,String>(13)?,
            "reconcile_policy": row.get::<_,String>(14)?, "version": row.get::<_,i64>(15)?,
            "created_by": row.get::<_,String>(16)?, "created_at": row.get::<_,i64>(17)?,
            "updated_at": row.get::<_,i64>(18)?, "deleted_at": row.get::<_,Option<i64>>(19)?,
        }))
    })?;
    rows.collect()
}

fn validate_port_free(
    db: &Connection,
    port: Option<u16>,
    except_id: Option<&str>,
) -> Result<(), (StatusCode, Json<Value>)> {
    let Some(port) = port else { return Ok(()) };
    if port < 1 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "پورت باید بین ۱ تا ۶۵۵۳۵ باشد.",
        ));
    }
    let duplicate: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM applications WHERE listen_port=?1 AND deleted_at IS NULL AND (?2 IS NULL OR id<>?2))",
        params![port as i64, except_id], |row| row.get(0),
    ).map_err(db_error)?;
    if duplicate {
        return Err(error(
            StatusCode::CONFLICT,
            "این پورت قبلاً برای یک اپلیکیشن ARG رزرو شده است.",
        ));
    }
    if listening_tcp_ports().contains(&port) {
        return Err(error(
            StatusCode::CONFLICT,
            "این پورت هم‌اکنون روی سرور در حال Listen است؛ پورت آزاد دیگری انتخاب کن.",
        ));
    }
    Ok(())
}

fn listening_tcp_ports() -> HashSet<u16> {
    let mut ports = HashSet::new();
    for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        for line in content.lines().skip(1) {
            if let Some(port) = listening_port_from_proc_line(line) {
                ports.insert(port);
            }
        }
    }
    ports
}

fn listening_port_from_proc_line(line: &str) -> Option<u16> {
    let columns = line.split_whitespace().collect::<Vec<_>>();
    if columns.get(3).copied()? != "0A" {
        return None;
    }
    u16::from_str_radix(columns.get(1)?.rsplit(':').next()?, 16)
        .ok()
        .filter(|port| *port > 0)
}

fn check_domain_conflicts(
    db: &Connection,
    owner: &str,
    domain: Option<&str>,
    except_id: Option<&str>,
) -> Result<(), (StatusCode, Json<Value>)> {
    let Some(domain) = domain else { return Ok(()) };
    let mut statement = db.prepare("SELECT id,domain FROM applications WHERE owner_server_uuid=?1 AND domain IS NOT NULL AND deleted_at IS NULL AND (?2 IS NULL OR id<>?2)").map_err(db_error)?;
    let rows = statement
        .query_map(params![owner, except_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(db_error)?;
    for row in rows {
        let (id, existing) = row.map_err(db_error)?;
        if domain_patterns_overlap(domain, &existing) {
            return Err(error(
                StatusCode::CONFLICT,
                &format!("دامنهٔ {domain} از قبل به اپلیکیشن {id} اختصاص یافته است."),
            ));
        }
    }
    Ok(())
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS applications (id TEXT PRIMARY KEY,owner_server_uuid TEXT NOT NULL,name TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',framework TEXT NOT NULL DEFAULT 'custom',runtime_version TEXT NOT NULL DEFAULT 'system',build_runtime_version TEXT NOT NULL DEFAULT 'system',source_type TEXT NOT NULL DEFAULT 'manual',domain TEXT,listen_port INTEGER,labels_json TEXT NOT NULL DEFAULT '[]',desired_state TEXT NOT NULL DEFAULT 'none',observed_state TEXT NOT NULL DEFAULT 'unknown',observed_at INTEGER,lifecycle_state TEXT NOT NULL DEFAULT 'active',reconcile_policy TEXT NOT NULL DEFAULT 'manual',origin_kind TEXT NOT NULL DEFAULT 'manual',origin_ref TEXT,version INTEGER NOT NULL DEFAULT 1,created_by TEXT NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,deleted_at INTEGER); CREATE UNIQUE INDEX IF NOT EXISTS applications_active_name ON applications(owner_server_uuid,name COLLATE NOCASE) WHERE deleted_at IS NULL; CREATE UNIQUE INDEX IF NOT EXISTS applications_active_domain ON applications(owner_server_uuid,domain COLLATE NOCASE) WHERE domain IS NOT NULL AND deleted_at IS NULL; CREATE INDEX IF NOT EXISTS applications_owner_updated ON applications(owner_server_uuid,updated_at DESC); CREATE TABLE IF NOT EXISTS application_services (id TEXT PRIMARY KEY,application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,service_key TEXT NOT NULL,runtime_type TEXT NOT NULL DEFAULT 'unknown',systemd_unit TEXT,desired_state TEXT NOT NULL DEFAULT 'none',observed_state TEXT NOT NULL DEFAULT 'unknown',listen_port INTEGER,last_observed_at INTEGER,metadata_json TEXT NOT NULL DEFAULT '{}',created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,UNIQUE(application_id,service_key)); CREATE INDEX IF NOT EXISTS application_services_app ON application_services(application_id);")?;
    let has_runtime_version = {
        let mut statement = db.prepare("PRAGMA table_info(applications)")?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        columns.iter().any(|column| column == "runtime_version")
    };
    if !has_runtime_version {
        db.execute(
            "ALTER TABLE applications ADD COLUMN runtime_version TEXT NOT NULL DEFAULT 'system'",
            [],
        )?;
    }
    let has_build_runtime_version = {
        let mut statement = db.prepare("PRAGMA table_info(applications)")?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        columns
            .iter()
            .any(|column| column == "build_runtime_version")
    };
    if !has_build_runtime_version {
        db.execute(
            "ALTER TABLE applications ADD COLUMN build_runtime_version TEXT NOT NULL DEFAULT 'system'",
            [],
        )?;
    }
    Ok(())
}

pub(super) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AppListQuery>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let apps = read_apps(&db, &owner, query.include_deleted).map_err(db_error)?;
    Ok(Json(
        json!({"server_uuid":owner,"apps":apps,"features":{"deploy":true,"environment_secrets":false,"storage_plans":false,"automatic_reconciliation":false}}),
    ))
}

pub(super) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AppInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let _operation_guard = APP_REGISTRY_OPERATION_LOCK.lock().await;
    let (name, domain) = validate_input(&input)?;
    let actor;
    let owner;
    {
        let db = state.lock().unwrap();
        actor = require_admin(&db, &headers)?.username;
        owner = federation::local_server_uuid(&db).map_err(db_error)?;
        validate_port_free(&db, input.listen_port, None)?;
        check_domain_conflicts(&db, &owner, domain.as_deref(), None)?;
        let duplicate_name: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM applications WHERE owner_server_uuid=?1 AND name=?2 COLLATE NOCASE AND deleted_at IS NULL)", params![owner,name], |row| row.get(0)).map_err(db_error)?;
        if duplicate_name {
            return Err(error(
                StatusCode::CONFLICT,
                "اپلیکیشنی با این نام از قبل ثبت شده است.",
            ));
        }
    }
    if domain.is_some() {
        let live = privileged_request_async(json!({"action":"nginx-list"})).await;
        match live {
            Ok(value) if value["ok"] == true => {
                for site in value["data"]["sites"].as_array().into_iter().flatten() {
                    let site_name = site["name"].as_str().unwrap_or("سایت موجود");
                    for existing in site["server_names"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        if domain_patterns_overlap(domain.as_deref().unwrap_or_default(), existing)
                        {
                            return Err(error(
                                StatusCode::CONFLICT,
                                &format!(
                                    "دامنهٔ {} در پیکربندی فعال/موجود Nginx ({site_name}) ثبت شده است.",
                                    domain.as_deref().unwrap_or_default()
                                ),
                            ));
                        }
                    }
                }
            }
            Ok(value)
                if value["error"].as_str().is_some_and(|message| {
                    message.contains("Nginx نصب نیست")
                        || message.contains("sites-available پیدا نشد")
                }) => {}
            _ => {
                return Err(error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "برای جلوگیری از تداخل دامنه، وضعیت Nginx قابل بررسی نیست. بدون دامنه اپلیکیشن را ثبت کن یا ابتدا سرویس Nginx را در دسترس قرار بده.",
                ));
            }
        }
    }
    let id = random_uuid();
    let at = now();
    let labels = input
        .labels
        .iter()
        .map(|label| label.trim())
        .collect::<Vec<_>>();
    let db = state.lock().unwrap();
    db.execute("INSERT INTO applications(id,owner_server_uuid,name,description,framework,runtime_version,build_runtime_version,source_type,domain,listen_port,labels_json,desired_state,observed_state,lifecycle_state,reconcile_policy,origin_kind,created_by,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'unknown','active','manual','manual',?13,?14,?14)", params![id,owner,name,input.description.trim(),input.framework,input.runtime_version,input.build_runtime_version,input.source_type,domain,input.listen_port.map(i64::from),serde_json::to_string(&labels).unwrap_or_else(|_| "[]".to_owned()),input.desired_state,actor,at]).map_err(|error_value| if error_value.sqlite_error_code()==Some(rusqlite::ErrorCode::ConstraintViolation) { error(StatusCode::CONFLICT,"نام، دامنه یا پورت با یک اپلیکیشن دیگر تداخل دارد.") } else { db_error(error_value) })?;
    audit(
        &db,
        &actor,
        "application.create",
        &format!(
            "id={id}; server={owner}; framework={}; source={}",
            input.framework, input.source_type
        ),
    );
    let apps = read_apps(&db, &owner, false).map_err(db_error)?;
    let app = apps
        .into_iter()
        .find(|app| app["id"] == id)
        .unwrap_or(Value::Null);
    Ok(Json(
        json!({"app":app,"message":"رکورد اپلیکیشن ساخته شد؛ هنوز هیچ کدی دریافت یا سرویسی اجرا نشده است."}),
    ))
}

pub(super) async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let app = read_apps(&db, &owner, true)
        .map_err(db_error)?
        .into_iter()
        .find(|app| app["id"] == id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."))?;
    Ok(Json(json!({"app":app})))
}

pub(super) async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<AppUpdate>,
) -> ApiResult {
    check_origin(&headers)?;
    let _operation_guard = APP_REGISTRY_OPERATION_LOCK.lock().await;
    if input.expected_version < 1 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نسخهٔ مورد انتظار معتبر نیست.",
        ));
    }
    let (name, domain) = validate_input(&input.app)?;
    let actor;
    let owner;
    {
        let db = state.lock().unwrap();
        actor = require_admin(&db, &headers)?.username;
        owner = federation::local_server_uuid(&db).map_err(db_error)?;
        validate_port_free(&db, input.app.listen_port, Some(&id))?;
        check_domain_conflicts(&db, &owner, domain.as_deref(), Some(&id))?;
        let current: Option<(i64, Option<i64>)> = db
            .query_row(
                "SELECT version,deleted_at FROM applications WHERE id=?1 AND owner_server_uuid=?2",
                params![id, owner],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(db_error)?;
        let Some((version, deleted)) = current else {
            return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."));
        };
        if deleted.is_some() {
            return Err(error(
                StatusCode::CONFLICT,
                "اپلیکیشن حذف نرم شده است؛ ابتدا آن را بازیابی کن.",
            ));
        }
        if version != input.expected_version {
            return Err(error(
                StatusCode::CONFLICT,
                "نسخهٔ اپلیکیشن تغییر کرده است؛ صفحه را تازه کن و تغییرات را دوباره اعمال کن.",
            ));
        }
    }
    if domain.is_some() {
        let live = privileged_request_async(json!({"action":"nginx-list"})).await;
        match live {
            Ok(value) if value["ok"] == true => {
                for site in value["data"]["sites"].as_array().into_iter().flatten() {
                    let site_name = site["name"].as_str().unwrap_or("سایت موجود");
                    for existing in site["server_names"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        if domain_patterns_overlap(domain.as_deref().unwrap_or_default(), existing)
                        {
                            return Err(error(
                                StatusCode::CONFLICT,
                                &format!(
                                    "دامنهٔ {} در پیکربندی Nginx سایت {site_name} ثبت شده است.",
                                    domain.as_deref().unwrap_or_default()
                                ),
                            ));
                        }
                    }
                }
            }
            Ok(value)
                if value["error"].as_str().is_some_and(|message| {
                    message.contains("Nginx نصب نیست")
                        || message.contains("sites-available پیدا نشد")
                }) => {}
            _ => {
                return Err(error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "برای جلوگیری از تداخل دامنه، وضعیت Nginx قابل بررسی نیست.",
                ));
            }
        }
    }
    let at = now();
    let labels = input
        .app
        .labels
        .iter()
        .map(|label| label.trim())
        .collect::<Vec<_>>();
    let db = state.lock().unwrap();
    let changed = db.execute("UPDATE applications SET name=?1,description=?2,framework=?3,runtime_version=?4,build_runtime_version=?5,source_type=?6,domain=?7,listen_port=?8,labels_json=?9,desired_state=?10,version=version+1,updated_at=?11 WHERE id=?12 AND owner_server_uuid=?13 AND version=?14 AND deleted_at IS NULL", params![name,input.app.description.trim(),input.app.framework,input.app.runtime_version,input.app.build_runtime_version,input.app.source_type,domain,input.app.listen_port.map(i64::from),serde_json::to_string(&labels).unwrap_or_else(|_|"[]".to_owned()),input.app.desired_state,at,id,owner,input.expected_version]).map_err(db_error)?;
    if changed == 0 {
        return Err(error(
            StatusCode::CONFLICT,
            "اپلیکیشن هم‌زمان تغییر کرده است؛ صفحه را تازه کن.",
        ));
    }
    audit(
        &db,
        &actor,
        "application.update",
        &format!("id={id}; version={}", input.expected_version + 1),
    );
    let app = read_apps(&db, &owner, false)
        .map_err(db_error)?
        .into_iter()
        .find(|app| app["id"] == id)
        .unwrap_or(Value::Null);
    Ok(Json(json!({"app":app})))
}

pub(super) async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<AppVersionRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let _operation_guard = APP_REGISTRY_OPERATION_LOCK.lock().await;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let changed=db.execute("UPDATE applications SET lifecycle_state='archived',deleted_at=?1,version=version+1,updated_at=?1 WHERE id=?2 AND owner_server_uuid=?3 AND version=?4 AND deleted_at IS NULL",params![now(),id,owner,input.expected_version]).map_err(db_error)?;
    if changed == 0 {
        return Err(error(
            StatusCode::CONFLICT,
            "اپلیکیشن پیدا نشد یا نسخهٔ آن تغییر کرده است؛ فهرست را تازه کن.",
        ));
    }
    audit(
        &db,
        &actor,
        "application.delete",
        &format!("id={id}; soft-delete"),
    );
    Ok(Json(
        json!({"deleted":true,"soft_delete":true,"message":"رکورد آرشیو شد؛ هیچ فایل یا سرویسی حذف نشده است."}),
    ))
}

pub(super) async fn restore(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<AppVersionRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let _operation_guard = APP_REGISTRY_OPERATION_LOCK.lock().await;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let (domain,port):(Option<String>,Option<u16>)=db.query_row("SELECT domain,listen_port FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NOT NULL",params![id,owner],|row|Ok((row.get(0)?,row.get::<_,Option<i64>>(1)?.map(|value|value as u16)))).optional().map_err(db_error)?.ok_or_else(||error(StatusCode::NOT_FOUND,"اپلیکیشن آرشیوشده پیدا نشد."))?;
    check_domain_conflicts(&db, &owner, domain.as_deref(), Some(&id))?;
    validate_port_free(&db, port, Some(&id))?;
    let changed=db.execute("UPDATE applications SET lifecycle_state='active',deleted_at=NULL,version=version+1,updated_at=?1 WHERE id=?2 AND owner_server_uuid=?3 AND version=?4 AND deleted_at IS NOT NULL",params![now(),id,owner,input.expected_version]).map_err(|error_value|if error_value.sqlite_error_code()==Some(rusqlite::ErrorCode::ConstraintViolation){error(StatusCode::CONFLICT,"نام یا دامنهٔ این اپلیکیشن اکنون به رکورد دیگری اختصاص یافته است.")}else{db_error(error_value)})?;
    if changed == 0 {
        return Err(error(
            StatusCode::CONFLICT,
            "اپلیکیشن پیدا نشد یا نسخهٔ آن تغییر کرده است؛ فهرست را تازه کن.",
        ));
    }
    audit(&db, &actor, "application.restore", &format!("id={id}"));
    let app = read_apps(&db, &owner, false)
        .map_err(db_error)?
        .into_iter()
        .find(|app| app["id"] == id)
        .unwrap_or(Value::Null);
    Ok(Json(
        json!({"app":app,"message":"رکورد بازیابی شد؛ وضعیت سرویس همچنان نیازمند بررسی مستقل است."}),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        domain_patterns_overlap, listening_tcp_ports, migrate, normalized_domain, valid_domain,
        valid_runtime_version, AppInput, AppUpdate,
    };

    #[test]
    fn domain_validation_normalizes_hosts_and_rejects_url_fragments() {
        assert!(valid_domain("app.example.com"));
        assert!(valid_domain("127.0.0.1"));
        assert!(!valid_domain("https://app.example.com"));
        assert!(!valid_domain("app.example.com:8080"));
        assert!(!valid_domain("-bad.example.com"));
        assert_eq!(
            normalized_domain(" APP.Example.com. ").as_deref(),
            Some("app.example.com")
        );
    }

    #[test]
    fn create_and_flattened_update_payloads_deserialize_with_defaults() {
        let create: AppInput = serde_json::from_value(serde_json::json!({
            "name":"Shop", "domain":"shop.example.com", "labels":["prod"]
        }))
        .unwrap();
        assert_eq!(create.framework, "custom");
        assert_eq!(create.runtime_version, "system");
        assert_eq!(create.source_type, "manual");
        assert_eq!(create.desired_state, "none");
        let update: AppUpdate = serde_json::from_value(serde_json::json!({
            "expected_version":3, "name":"Shop", "framework":"laravel",
            "source_type":"git", "domain":"", "labels":[], "desired_state":"running"
        }))
        .unwrap();
        assert_eq!(update.expected_version, 3);
        assert_eq!(update.app.framework, "laravel");
        assert!(serde_json::from_value::<AppInput>(
            serde_json::json!({"name":"x","command":"rm -rf /"})
        )
        .is_err());
    }

    #[test]
    fn wildcard_domain_conflicts_are_detected() {
        assert!(domain_patterns_overlap("*.example.com", "api.example.com"));
        assert!(domain_patterns_overlap("api.example.com", "*.example.com"));
        assert!(domain_patterns_overlap("*example.com", "badexample.com"));
        assert!(domain_patterns_overlap("example.*", "example.net"));
        assert!(!domain_patterns_overlap("*.example.com", "example.com"));
        assert!(!domain_patterns_overlap("a.example.com", "b.example.com"));
    }

    #[test]
    fn app_schema_is_additive_and_migration_idempotent() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE users(id INTEGER PRIMARY KEY,username TEXT); CREATE TABLE nginx_site_config_state(name TEXT PRIMARY KEY,content TEXT); INSERT INTO nginx_site_config_state VALUES('old','server_name old.example.com;');").unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        let preserved: String = db
            .query_row(
                "SELECT content FROM nginx_site_config_state WHERE name='old'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(preserved, "server_name old.example.com;");
        assert!(db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='application_services')", [], |row| row.get::<_,bool>(0)).unwrap());
        assert!(db.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('applications') WHERE name='runtime_version')", [], |row| row.get::<_, bool>(0)).unwrap());
    }

    #[test]
    fn old_application_schema_gets_runtime_defaults_without_losing_rows() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE applications(id TEXT PRIMARY KEY,owner_server_uuid TEXT NOT NULL,name TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',framework TEXT NOT NULL DEFAULT 'custom',source_type TEXT NOT NULL DEFAULT 'manual',domain TEXT,listen_port INTEGER,labels_json TEXT NOT NULL DEFAULT '[]',desired_state TEXT NOT NULL DEFAULT 'none',observed_state TEXT NOT NULL DEFAULT 'unknown',observed_at INTEGER,lifecycle_state TEXT NOT NULL DEFAULT 'active',reconcile_policy TEXT NOT NULL DEFAULT 'manual',origin_kind TEXT NOT NULL DEFAULT 'manual',origin_ref TEXT,version INTEGER NOT NULL DEFAULT 1,created_by TEXT NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,deleted_at INTEGER); INSERT INTO applications(id,owner_server_uuid,name,created_by,created_at,updated_at) VALUES('old','server','Existing','admin',1,1);").unwrap();
        migrate(&db).unwrap();
        let values: (String, String) = db
            .query_row(
                "SELECT runtime_version,build_runtime_version FROM applications WHERE id='old'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(values, ("system".into(), "system".into()));
    }

    #[test]
    fn application_runtime_versions_reject_command_syntax() {
        assert!(valid_runtime_version("system"));
        assert!(valid_runtime_version("8.5"));
        assert!(valid_runtime_version("22.14.1"));
        assert!(!valid_runtime_version("8..5"));
        assert!(!valid_runtime_version("$(id)"));
        assert!(!valid_runtime_version("8.5; touch /tmp/x"));
    }

    #[test]
    fn listener_inventory_is_a_valid_port_set() {
        assert!(listening_tcp_ports().iter().all(|port| *port > 0));
        assert_eq!(
            super::listening_port_from_proc_line(
                "0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000 1000 0 42"
            ),
            Some(8080)
        );
        assert_eq!(
            super::listening_port_from_proc_line(
                "0: 0100007F:1F90 00000000:0000 01 00000000:00000000 00:00000000 00000000 1000 0 42"
            ),
            None
        );
    }
}
