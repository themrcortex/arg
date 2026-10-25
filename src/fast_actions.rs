use super::*;

#[derive(Clone, Serialize)]
struct FastAction {
    id: i64,
    title: String,
    operation: String,
    target: String,
    site_name: Option<String>,
    include_access_log: bool,
    include_error_log: bool,
}

#[derive(Deserialize)]
pub(super) struct FastActionInput {
    title: String,
    operation: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    site_name: Option<String>,
    #[serde(default)]
    include_access_log: bool,
    #[serde(default)]
    include_error_log: bool,
}

fn read_fast_action(db: &Connection, id: i64) -> Result<FastAction, rusqlite::Error> {
    db.query_row(
        "SELECT id,title,operation,target,site_name,include_access_log,include_error_log FROM fast_actions WHERE id=?1",
        [id],
        |row| Ok(FastAction {
            id: row.get(0)?, title: row.get(1)?, operation: row.get(2)?, target: row.get(3)?,
            site_name: row.get(4)?, include_access_log: row.get::<_, i64>(5)? != 0,
            include_error_log: row.get::<_, i64>(6)? != 0,
        }),
    )
}

fn validate_fast_action(input: &FastActionInput) -> Result<(), (StatusCode, Json<Value>)> {
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 48 {
        return Err(error(StatusCode::BAD_REQUEST, "عنوان Fast Action باید بین 1 تا 48 نویسه باشد."));
    }
    let operation = input.operation.as_str();
    let service_operation = operation.strip_prefix("service-");
    let allowed = ["nginx-test", "nginx-reload", "nginx-restart"];
    if !allowed.contains(&operation) && service_operation.map_or(true, |op| !["start", "restart", "reload"].contains(&op)) {
        return Err(error(StatusCode::BAD_REQUEST, "این عملیات در فهرست Fast Actionهای امن نیست."));
    }
    if let Some(op) = service_operation {
        let unit = input.target.trim();
        if !super::valid_service_unit(unit) {
            return Err(error(StatusCode::BAD_REQUEST, "واحد systemd معتبر نیست."));
        }
        if super::protected_service_mutation(unit, op) {
            return Err(error(StatusCode::BAD_REQUEST, "این سرویس برای جلوگیری از قطع دسترسی از Fast Action محافظت می‌شود."));
        }
    } else if !input.target.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "عملیات Nginx به target نیاز ندارد."));
    }
    if input.include_access_log || input.include_error_log {
        let name = input.site_name.as_deref().unwrap_or_default();
        if name.is_empty() || name.len() > 128 || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) {
            return Err(error(StatusCode::BAD_REQUEST, "برای نمایش لاگ، یک سایت Nginx معتبر انتخاب کن."));
        }
    }
    Ok(())
}

pub(super) async fn list(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let mut stmt = db.prepare("SELECT id,title,operation,target,site_name,include_access_log,include_error_log FROM fast_actions ORDER BY id").map_err(db_error)?;
    let rows = stmt.query_map([], |row| Ok(FastAction {
        id: row.get(0)?, title: row.get(1)?, operation: row.get(2)?, target: row.get(3)?,
        site_name: row.get(4)?, include_access_log: row.get::<_, i64>(5)? != 0,
        include_error_log: row.get::<_, i64>(6)? != 0,
    })).map_err(db_error)?;
    Ok(Json(json!({"actions": rows.collect::<Result<Vec<_>,_>>().map_err(db_error)?})))
}

pub(super) async fn create(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<FastActionInput>) -> ApiResult {
    check_origin(&headers)?;
    validate_fast_action(&input)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    db.execute("INSERT INTO fast_actions(title,operation,target,site_name,include_access_log,include_error_log,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?7)", params![input.title.trim(), input.operation, input.target.trim(), input.site_name.as_deref().map(str::trim).filter(|value| !value.is_empty()), input.include_access_log as i64, input.include_error_log as i64, now()]).map_err(db_error)?;
    let action = read_fast_action(&db, db.last_insert_rowid()).map_err(db_error)?;
    audit(&db, &actor, "dashboard.fast_action.create", &format!("id={} operation={}", action.id, action.operation));
    Ok(Json(json!({"action":action})))
}

pub(super) async fn update(State(state): State<AppState>, Path(id): Path<i64>, headers: HeaderMap, Json(input): Json<FastActionInput>) -> ApiResult {
    check_origin(&headers)?;
    validate_fast_action(&input)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    read_fast_action(&db, id).map_err(|_| error(StatusCode::NOT_FOUND, "Fast Action پیدا نشد."))?;
    db.execute("UPDATE fast_actions SET title=?1,operation=?2,target=?3,site_name=?4,include_access_log=?5,include_error_log=?6,updated_at=?7 WHERE id=?8", params![input.title.trim(), input.operation, input.target.trim(), input.site_name.as_deref().map(str::trim).filter(|value| !value.is_empty()), input.include_access_log as i64, input.include_error_log as i64, now(), id]).map_err(db_error)?;
    let action = read_fast_action(&db, id).map_err(db_error)?;
    audit(&db, &actor, "dashboard.fast_action.update", &format!("id={id} operation={}", action.operation));
    Ok(Json(json!({"action":action})))
}

pub(super) async fn delete(State(state): State<AppState>, Path(id): Path<i64>, headers: HeaderMap) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let action = read_fast_action(&db, id).map_err(|_| error(StatusCode::NOT_FOUND, "Fast Action پیدا نشد."))?;
    db.execute("DELETE FROM fast_actions WHERE id=?1", [id]).map_err(db_error)?;
    audit(&db, &actor, "dashboard.fast_action.delete", &format!("id={id} operation={}", action.operation));
    Ok(Json(json!({"ok":true})))
}
