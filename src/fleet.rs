use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TagsInput {
    tags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RolloutPreviewInput {
    branch: String,
    version: String,
    node_ids: Vec<i64>,
    canary_count: usize,
    health_gate: bool,
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS fleet_node_tags(node_id INTEGER NOT NULL REFERENCES federated_nodes(id) ON DELETE CASCADE,tag TEXT NOT NULL,created_by TEXT NOT NULL,created_at INTEGER NOT NULL,PRIMARY KEY(node_id,tag)); CREATE INDEX IF NOT EXISTS fleet_node_tags_tag ON fleet_node_tags(tag); CREATE TABLE IF NOT EXISTS fleet_rollout_previews(id TEXT PRIMARY KEY,branch TEXT NOT NULL,version TEXT NOT NULL,node_ids_json TEXT NOT NULL,canary_count INTEGER NOT NULL,health_gate INTEGER NOT NULL,created_by TEXT NOT NULL,created_at INTEGER NOT NULL,state TEXT NOT NULL DEFAULT 'preview'); CREATE INDEX IF NOT EXISTS fleet_rollout_previews_created ON fleet_rollout_previews(created_at DESC);")
}

fn valid_tag(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
}

fn valid_release(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-_+".contains(&byte))
}

pub(super) async fn set_tags(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(node_id): Path<i64>,
    Json(input): Json<TagsInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let mut tags = input
        .tags
        .into_iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    tags.sort();
    tags.dedup();
    if tags.len() > 16 || tags.iter().any(|tag| !valid_tag(tag)) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "حداکثر ۱۶ برچسب معتبر حروفی/عددی مجاز است.",
        ));
    }
    let mut db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM federated_nodes WHERE id=?1)",
            [node_id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if !exists {
        return Err(error(StatusCode::NOT_FOUND, "Node پیدا نشد."));
    }
    let tx = db.transaction().map_err(db_error)?;
    tx.execute("DELETE FROM fleet_node_tags WHERE node_id=?1", [node_id])
        .map_err(db_error)?;
    for tag in &tags {
        tx.execute(
            "INSERT INTO fleet_node_tags(node_id,tag,created_by,created_at) VALUES(?1,?2,?3,?4)",
            params![node_id, tag, actor, now()],
        )
        .map_err(db_error)?;
    }
    tx.commit().map_err(db_error)?;
    audit(
        &db,
        &actor,
        "fleet.node.tags.update",
        &format!("node={node_id}; count={}", tags.len()),
    );
    Ok(Json(
        json!({"node_id":node_id,"tags":tags,"remote_state_unchanged":true}),
    ))
}

pub(super) async fn list_groups(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let mut statement = db.prepare("SELECT t.tag,n.id,n.name,n.server_uuid,n.connection_state,n.protocol_version,n.last_seen FROM fleet_node_tags t JOIN federated_nodes n ON n.id=t.node_id ORDER BY t.tag,n.name COLLATE NOCASE").map_err(db_error)?;
    let rows = statement.query_map([], |row| Ok(json!({"tag":row.get::<_,String>(0)?,"node_id":row.get::<_,i64>(1)?,"name":row.get::<_,String>(2)?,"server_uuid":row.get::<_,Option<String>>(3)?,"connection_state":row.get::<_,String>(4)? ,"protocol_version":row.get::<_,i64>(5)? ,"last_seen":row.get::<_,Option<i64>>(6)?}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
    Ok(Json(
        json!({"nodes":rows,"execution":"read_only_inventory"}),
    ))
}

pub(super) async fn preview_rollout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<RolloutPreviewInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let mut ids = input.node_ids.clone();
    ids.sort_unstable();
    ids.dedup();
    if !["main", "development"].contains(&input.branch.as_str())
        || !valid_release(&input.version)
        || ids.is_empty()
        || ids.len() > 100
        || ids.len() != input.node_ids.len()
        || input.canary_count == 0
        || input.canary_count > ids.len()
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "پارامترهای پیش‌نمایش انتشار ناوگان معتبر نیستند.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let mut known = Vec::new();
    for id in &ids {
        let node: Option<(String, String, Option<String>)> = db
            .query_row(
                "SELECT name,connection_state,last_seen FROM federated_nodes WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(db_error)?;
        let Some((name, state, last_seen)) = node else {
            return Err(error(
                StatusCode::NOT_FOUND,
                "یکی از Nodeهای انتخاب‌شده پیدا نشد.",
            ));
        };
        known.push(json!({"id":id,"name":name,"connection_state":state,"last_seen":last_seen,"will_execute":false}));
    }
    let id = format!("{:032x}", rand::random::<u128>());
    let at = now();
    db.execute("INSERT INTO fleet_rollout_previews(id,branch,version,node_ids_json,canary_count,health_gate,created_by,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![id,input.branch,input.version,serde_json::to_string(&ids).unwrap_or_else(|_|"[]".into()),input.canary_count as i64,input.health_gate,actor,at]).map_err(db_error)?;
    audit(
        &db,
        &actor,
        "fleet.rollout.preview",
        &format!("preview={id}; nodes={}", ids.len()),
    );
    Ok(Json(
        json!({"id":id,"branch":input.branch,"version":input.version,"canary_count":input.canary_count,"health_gate":input.health_gate,"nodes":known,"state":"preview","executed":false,"warning":"این فقط طرح انتشار است؛ هیچ Nodeای به‌روزرسانی یا تغییر داده نشده است."}),
    ))
}

pub(super) async fn list_rollouts(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let mut statement=db.prepare("SELECT id,branch,version,node_ids_json,canary_count,health_gate,created_by,created_at,state FROM fleet_rollout_previews ORDER BY created_at DESC LIMIT 50").map_err(db_error)?;
    let rows=statement.query_map([],|row|Ok(json!({"id":row.get::<_,String>(0)? ,"branch":row.get::<_,String>(1)? ,"version":row.get::<_,String>(2)? ,"node_ids":serde_json::from_str::<Value>(&row.get::<_,String>(3)?).unwrap_or(json!([])),"canary_count":row.get::<_,i64>(4)? ,"health_gate":row.get::<_,bool>(5)? ,"created_by":row.get::<_,String>(6)? ,"created_at":row.get::<_,i64>(7)? ,"state":row.get::<_,String>(8)? ,"executed":false}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
    Ok(Json(json!({"previews":rows})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tags_and_versions_are_bounded() {
        assert!(valid_tag("parspack-prod"));
        assert!(!valid_tag("../prod"));
        assert!(valid_release("0.6.9-dev.8"));
        assert!(!valid_release("main;reboot"));
    }
}
