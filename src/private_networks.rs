use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PolicyInput {
    source_app_id: String,
    destination_node_id: i64,
    destination_service: String,
    port: u16,
    #[serde(default)]
    confirmed: bool,
}

fn valid_service(value: &str) -> bool {
    (1..=100).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

fn valid_policy_target(service: &str, port: u16) -> bool {
    port != 0 && valid_service(service)
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS private_network_policy_drafts(id TEXT PRIMARY KEY,source_server_uuid TEXT NOT NULL,source_app_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,destination_server_uuid TEXT NOT NULL,destination_service TEXT NOT NULL,port INTEGER NOT NULL,protocol TEXT NOT NULL DEFAULT 'tcp',created_by TEXT NOT NULL,created_at INTEGER NOT NULL,state TEXT NOT NULL DEFAULT 'awaiting_overlay',enforced INTEGER NOT NULL DEFAULT 0,UNIQUE(source_app_id,destination_server_uuid,destination_service,port,protocol)); CREATE INDEX IF NOT EXISTS private_network_policy_source ON private_network_policy_drafts(source_app_id,created_at DESC);")
}

pub(super) async fn list(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let mut statement=db.prepare("SELECT p.id,p.source_app_id,a.name,p.destination_server_uuid,p.destination_service,p.port,p.protocol,p.created_by,p.created_at,p.state,p.enforced,n.public_name FROM private_network_policy_drafts p JOIN applications a ON a.id=p.source_app_id LEFT JOIN federated_nodes n ON n.server_uuid=p.destination_server_uuid WHERE p.source_server_uuid=?1 ORDER BY p.created_at DESC LIMIT 200").map_err(db_error)?;
    let rows=statement.query_map([owner],|row|Ok(json!({"id":row.get::<_,String>(0)? ,"source_app_id":row.get::<_,String>(1)? ,"source_app_name":row.get::<_,String>(2)? ,"destination_server_uuid":row.get::<_,String>(3)? ,"destination_service":row.get::<_,String>(4)? ,"port":row.get::<_,i64>(5)? ,"protocol":row.get::<_,String>(6)? ,"created_by":row.get::<_,String>(7)? ,"created_at":row.get::<_,i64>(8)? ,"state":row.get::<_,String>(9)? ,"enforced":row.get::<_,bool>(10)? ,"destination_public_name":row.get::<_,Option<String>>(11)?}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
    Ok(Json(
        json!({"policies":rows,"overlay_ready":false,"enforcement":"not_configured"}),
    ))
}

pub(super) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PolicyInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !input.confirmed
        || !valid_policy_target(&input.destination_service, input.port)
        || !apps::valid_uuid(&input.source_app_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "برای پیش‌نویس سیاست، اپلیکیشن، سرویس، پورت و تأیید لازم است.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let source_uuid = federation::local_server_uuid(&db).map_err(db_error)?;
    let source_exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL)",params![input.source_app_id,source_uuid],|row|row.get(0)).map_err(db_error)?;
    if !source_exists {
        return Err(error(
            StatusCode::NOT_FOUND,
            "اپلیکیشن مبدأ در VPS جاری پیدا نشد.",
        ));
    }
    let destination:Option<(String,String)>=db.query_row("SELECT server_uuid,connection_state FROM federated_nodes WHERE id=?1 AND server_uuid IS NOT NULL",[input.destination_node_id],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(db_error)?;
    let Some((destination_uuid, connection_state)) = destination else {
        return Err(error(
            StatusCode::NOT_FOUND,
            "Node مقصد pairing معتبر ندارد.",
        ));
    };
    if destination_uuid == source_uuid {
        return Err(error(StatusCode::BAD_REQUEST, "مقصد باید VPS دیگری باشد."));
    }
    let id = format!("{:032x}", rand::random::<u128>());
    let insert=db.execute("INSERT INTO private_network_policy_drafts(id,source_server_uuid,source_app_id,destination_server_uuid,destination_service,port,protocol,created_by,created_at,state,enforced) VALUES(?1,?2,?3,?4,?5,?6,'tcp',?7,?8,'awaiting_overlay',0)",params![id,source_uuid,input.source_app_id,destination_uuid,input.destination_service,input.port,actor,now()]);
    if let Err(problem) = insert {
        return Err(
            if problem.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                error(StatusCode::CONFLICT, "این سیاست از قبل ثبت شده است.")
            } else {
                db_error(problem)
            },
        );
    }
    audit(
        &db,
        &actor,
        "private_network.policy.draft",
        &format!(
            "id={id}; source_app={}; destination={destination_uuid}:{}",
            input.source_app_id, input.port
        ),
    );
    Ok(Json(
        json!({"id":id,"state":"awaiting_overlay","enforced":false,"destination_connection_state":connection_state,"message":"این فقط پیش‌نویس است. WireGuard overlay و firewall policy در این انتشار فعال نیستند؛ ترافیکی مجاز یا مسدود نشده است."}),
    ))
}

pub(super) async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let changed=db.execute("DELETE FROM private_network_policy_drafts WHERE id=?1 AND source_server_uuid=?2 AND enforced=0",params![id,owner]).map_err(db_error)?;
    if changed == 0 {
        return Err(error(StatusCode::NOT_FOUND, "پیش‌نویس سیاست شبکه پیدا نشد."));
    }
    audit(
        &db,
        &actor,
        "private_network.policy.delete",
        &format!("id={id}"),
    );
    Ok(Json(json!({"deleted":true,"network_untouched":true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_targets_and_ports_are_bounded() {
        assert!(valid_service("postgres-primary"));
        assert!(!valid_service("127.0.0.1; reboot"));
        assert!(valid_policy_target("postgres-primary", 5432));
        assert!(!valid_policy_target("postgres-primary", 0));
    }
}
