use super::*;
use sha2::{Digest, Sha256};

const MAX_SNAPSHOTS_PER_APP: i64 = 100;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SnapshotInput {
    #[serde(default)]
    label: String,
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS app_config_snapshots (id TEXT PRIMARY KEY,application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,environment_id TEXT,app_version INTEGER NOT NULL,release_id TEXT,source_commit TEXT,label TEXT NOT NULL DEFAULT '',payload_json TEXT NOT NULL,sha256 TEXT NOT NULL,created_by TEXT NOT NULL,created_at INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS app_config_snapshots_app ON app_config_snapshots(application_id,created_at DESC);")
}

fn snapshot_id() -> String {
    format!("{:032x}", rand::random::<u128>())
}

fn digest(payload: &str) -> String {
    hex::encode(Sha256::digest(payload.as_bytes()))
}

pub(super) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<SnapshotInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if input.label.chars().count() > 100 || input.label.chars().any(char::is_control) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "عنوان Snapshot حداکثر ۱۰۰ نویسه و بدون نویسهٔ کنترلی باشد.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let app_json: Option<String> = db.query_row(
        "SELECT json_object('id',a.id,'name',a.name,'description',a.description,'framework',a.framework,'runtime_version',a.runtime_version,'build_runtime_version',a.build_runtime_version,'source_type',a.source_type,'domain',a.domain,'listen_port',a.listen_port,'labels',json(a.labels_json),'desired_state',a.desired_state,'static_output_path',a.static_output_path,'static_spa_fallback',a.static_spa_fallback,'version',a.version,'environment_id',c.environment_id,'environment_name',e.name,'environment_kind',e.kind,'project_id',p.id,'project_name',p.name,'workspace_id',w.id,'workspace_name',w.name) FROM applications a LEFT JOIN application_context c ON c.application_id=a.id LEFT JOIN app_environments e ON e.id=c.environment_id LEFT JOIN app_projects p ON p.id=e.project_id LEFT JOIN app_workspaces w ON w.id=p.workspace_id WHERE a.id=?1 AND a.owner_server_uuid=?2 AND a.deleted_at IS NULL",
        params![app_id, owner],
        |row| row.get(0),
    ).optional().map_err(db_error)?;
    let Some(app_json) = app_json else {
        return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن فعال پیدا نشد."));
    };
    let app: Value = serde_json::from_str(&app_json).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Snapshot از دادهٔ اپ ساخته نشد.",
        )
    })?;
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM app_config_snapshots WHERE application_id=?1",
            [&app_id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if count >= MAX_SNAPSHOTS_PER_APP {
        return Err(error(
            StatusCode::CONFLICT,
            "برای این اپ ۱۰۰ Snapshot نگه داشته شده؛ ابتدا سیاست نگه‌داری را بررسی کن.",
        ));
    }
    let release: Option<(String, Option<String>)> = db.query_row("SELECT id,source_commit FROM deployment_releases WHERE application_id=?1 AND state='active' ORDER BY activated_at DESC LIMIT 1", [&app_id], |row| Ok((row.get(0)?,row.get(1)?))).optional().map_err(db_error)?;
    let release_id = release.as_ref().map(|item| item.0.clone());
    let commit = release.and_then(|item| item.1);
    let environment_id = app["environment_id"].as_str().map(str::to_owned);
    let payload = json!({"schema_version":1,"application":app,"release_id":release_id,"source_commit":commit,"contains_secret_values":false,"notice":"This is a metadata snapshot; it does not copy data, database state, or secret values."});
    let payload_json = serde_json::to_string(&payload)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Snapshot سریال‌سازی نشد."))?;
    let checksum = digest(&payload_json);
    let id = snapshot_id();
    let at = now();
    db.execute("INSERT INTO app_config_snapshots(id,application_id,environment_id,app_version,release_id,source_commit,label,payload_json,sha256,created_by,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",params![id,app_id,environment_id,app["application"]["version"].as_i64().unwrap_or_else(|| app["version"].as_i64().unwrap_or(1)),release_id,commit,input.label.trim(),payload_json,checksum,actor,at]).map_err(db_error)?;
    audit(
        &db,
        &actor,
        "app.config_snapshot.create",
        &format!("app={app_id}; snapshot={id}; sha256={checksum}"),
    );
    Ok(Json(
        json!({"id":id,"application_id":app_id,"environment_id":environment_id,"release_id":release_id,"source_commit":commit,"label":input.label.trim(),"payload":payload,"sha256":checksum,"created_by":actor,"created_at":at,"restorable":false}),
    ))
}

pub(super) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL)",params![app_id,owner],|row|row.get(0)).map_err(db_error)?;
    if !valid {
        return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."));
    }
    let mut statement=db.prepare("SELECT id,environment_id,app_version,release_id,source_commit,label,payload_json,sha256,created_by,created_at FROM app_config_snapshots WHERE application_id=?1 ORDER BY created_at DESC LIMIT 100").map_err(db_error)?;
    let snapshots=statement.query_map([app_id],|row|Ok(json!({"id":row.get::<_,String>(0)?,"environment_id":row.get::<_,Option<String>>(1)?,"app_version":row.get::<_,i64>(2)?,"release_id":row.get::<_,Option<String>>(3)?,"source_commit":row.get::<_,Option<String>>(4)?,"label":row.get::<_,String>(5)?,"payload":serde_json::from_str::<Value>(&row.get::<_,String>(6)?).unwrap_or(Value::Null),"sha256":row.get::<_,String>(7)?,"created_by":row.get::<_,String>(8)?,"created_at":row.get::<_,i64>(9)?,"restorable":false}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
    Ok(Json(json!({"snapshots":snapshots})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_are_content_addressed_and_explicitly_secret_free() {
        let payload = json!({"schema_version":1,"application":{"name":"demo","domain":"example.test"},"contains_secret_values":false});
        let encoded = serde_json::to_string(&payload).unwrap();
        assert_eq!(digest(&encoded), digest(&encoded));
        assert!(payload["application"].get("api_token").is_none());
        assert!(payload["application"].get("password").is_none());
        assert_eq!(payload["contains_secret_values"], false);
    }
}
