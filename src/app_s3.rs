use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BindingInput {
    bucket_id: i64,
    #[serde(default)]
    prefix: String,
    purpose: String,
    #[serde(default = "default_access")]
    access_mode: String,
}
fn default_access() -> String {
    "read-only".into()
}
fn valid_prefix(value: &str) -> bool {
    value.len() <= 512
        && !value.starts_with('/')
        && !value.contains(['\\', '\0'])
        && value.split('/').all(|part| part != ".." && part != ".")
        && !value.bytes().any(|byte| byte.is_ascii_control())
}
fn valid_purpose(value: &str) -> bool {
    [
        "object-storage",
        "backup-target",
        "artifact-cache",
        "asset-storage",
        "rclone-mount",
    ]
    .contains(&value)
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS application_s3_bindings(id TEXT PRIMARY KEY,application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,bucket_id INTEGER NOT NULL REFERENCES s3_buckets(id) ON DELETE RESTRICT,prefix TEXT NOT NULL DEFAULT '',purpose TEXT NOT NULL,access_mode TEXT NOT NULL DEFAULT 'read-only',created_by TEXT NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,UNIQUE(application_id,bucket_id,prefix,purpose)); CREATE INDEX IF NOT EXISTS application_s3_bindings_app ON application_s3_bindings(application_id);")
}

pub(super) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL)",params![app_id,owner],|r|r.get(0)).map_err(db_error)?;
    if !exists {
        return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."));
    }
    let mut stmt=db.prepare("SELECT x.id,x.bucket_id,x.prefix,x.purpose,x.access_mode,x.created_at,b.bucket_name,b.display_name,c.display_name,c.provider_type,c.endpoint,c.status,c.credential_present,(SELECT m.mount_path FROM s3_mounts m WHERE m.bucket_id=b.id AND m.status='mounted' ORDER BY m.id DESC LIMIT 1) FROM application_s3_bindings x JOIN s3_buckets b ON b.id=x.bucket_id JOIN s3_connections c ON c.id=b.connection_id WHERE x.application_id=?1 ORDER BY x.created_at DESC").map_err(db_error)?;
    let bindings=stmt.query_map([app_id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"bucket_id":r.get::<_,i64>(1)?,"prefix":r.get::<_,String>(2)?,"purpose":r.get::<_,String>(3)?,"access_mode":r.get::<_,String>(4)?,"created_at":r.get::<_,i64>(5)?,"bucket_name":r.get::<_,String>(6)?,"bucket_display_name":r.get::<_,String>(7)?,"connection_name":r.get::<_,String>(8)?,"provider":r.get::<_,String>(9)?,"endpoint":r.get::<_,String>(10)?,"connection_status":r.get::<_,String>(11)?,"credential_present":r.get::<_,bool>(12)?,"mounted_path":r.get::<_,Option<String>>(13)?,"secret_exposed":false,"permission_enforced_by_arg":false}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
    Ok(Json(
        json!({"bindings":bindings,"note":"این اتصال در ARG ثبت می‌شود؛ credential به محیط اپ صادر نمی‌شود. دسترسی prefix را با کلید محدودشده در ارائه‌دهنده کنترل کن؛ access mode اینجا metadata است و به‌تنهایی مجوز S3 را محدود نمی‌کند."}),
    ))
}

pub(super) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<BindingInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !apps::valid_uuid(&app_id)
        || input.bucket_id <= 0
        || !valid_prefix(&input.prefix)
        || !valid_purpose(&input.purpose)
        || !["read-only", "read-write-requested"].contains(&input.access_mode.as_str())
    {
        return Err(error(StatusCode::BAD_REQUEST, "تنظیم اتصال S3 معتبر نیست."));
    }
    let (actor, connection_status, verified, mounted) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let app:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL)",params![app_id,owner],|r|r.get(0)).map_err(db_error)?;
        if !app {
            return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."));
        }
        let row=db.query_row("SELECT c.status,b.verified_at,EXISTS(SELECT 1 FROM s3_mounts m WHERE m.bucket_id=b.id AND m.status='mounted') FROM s3_buckets b JOIN s3_connections c ON c.id=b.connection_id WHERE b.id=?1 AND b.enabled=1",[input.bucket_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<i64>>(1)?,r.get::<_,bool>(2)?))).optional().map_err(db_error)?.ok_or_else(||error(StatusCode::NOT_FOUND,"Bucket فعال پیدا نشد."))?;
        (actor, row.0, row.1, row.2)
    };
    if input.purpose == "rclone-mount" && !mounted {
        return Err(error(
            StatusCode::CONFLICT,
            "برای این Bucket باید ابتدا Mount واقعی را از صفحهٔ S3 فعال کنی.",
        ));
    }
    if connection_status != "connected" && verified.is_none() {
        return Err(error(
            StatusCode::CONFLICT,
            "اتصال Bucket هنوز تست نشده است؛ ابتدا اتصال را بررسی کن.",
        ));
    }
    let id = apps::random_uuid();
    let db = state.lock().unwrap();
    db.execute("INSERT INTO application_s3_bindings(id,application_id,bucket_id,prefix,purpose,access_mode,created_by,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8)",params![id,app_id,input.bucket_id,input.prefix,input.purpose,input.access_mode,actor,now()]).map_err(|_|error(StatusCode::CONFLICT,"این اتصال S3 قبلاً ثبت شده است."))?;
    audit(
        &db,
        &actor,
        "app-s3.bind",
        &format!(
            "app={app_id}; bucket_id={}; purpose={}",
            input.bucket_id, input.purpose
        ),
    );
    Ok(Json(
        json!({"id":id,"application_id":app_id,"bucket_id":input.bucket_id,"prefix":input.prefix,"purpose":input.purpose,"access_mode":input.access_mode,"secret_exposed":false,"provider_acl_required":true}),
    ))
}

pub(super) async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let changed=db.execute("DELETE FROM application_s3_bindings WHERE id=?1 AND application_id IN (SELECT id FROM applications WHERE owner_server_uuid=?2)",params![id,owner]).map_err(db_error)?;
    if changed == 0 {
        return Err(error(StatusCode::NOT_FOUND, "اتصال S3 اپ پیدا نشد."));
    }
    audit(&db, &actor, "app-s3.unbind", &format!("binding={id}"));
    Ok(Json(json!({"removed":true,"remote_data_deleted":false})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefixes_cannot_escape() {
        assert!(valid_prefix("customer-a/assets/"));
        assert!(valid_prefix(""));
        assert!(!valid_prefix("../private"));
        assert!(!valid_prefix("/root"));
        assert!(!valid_prefix("a\\b"));
    }
    #[test]
    fn purposes_are_gated() {
        assert!(valid_purpose("backup-target"));
        assert!(!valid_purpose("mount-root"));
    }
    #[test]
    fn migration_is_additive_and_idempotent() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE applications(id TEXT PRIMARY KEY);CREATE TABLE s3_buckets(id INTEGER PRIMARY KEY);PRAGMA foreign_keys=ON;").unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        let count:i64=db.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='application_s3_bindings'",[],|r|r.get(0)).unwrap();
        assert_eq!(count, 1);
    }
}
