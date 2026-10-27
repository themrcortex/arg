use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkInput {
    container_id: String,
    service_name: String,
    #[serde(default)]
    confirmed: bool,
}

fn valid_container_id(value: &str) -> bool {
    (12..=64).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_service_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS app_container_links (id INTEGER PRIMARY KEY,application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,container_id TEXT NOT NULL UNIQUE,service_name TEXT NOT NULL,created_by TEXT NOT NULL,created_at INTEGER NOT NULL,UNIQUE(application_id,service_name)); CREATE INDEX IF NOT EXISTS app_container_links_app ON app_container_links(application_id);")
}

async fn inventory() -> Result<Vec<Value>, (StatusCode, Json<Value>)> {
    let result = privileged_request_async(json!({"action":"docker-status"})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "وضعیت Docker از helper خوانده نشد.",
        ));
    }
    if result["data"]["installed"] != true {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Docker روی این VPS نصب یا در دسترس نیست.",
        ));
    }
    Ok(result["data"]["containers"]
        .as_array()
        .cloned()
        .unwrap_or_default())
}

fn owned_app(db: &Connection, app_id: &str) -> Result<String, (StatusCode, Json<Value>)> {
    let owner = federation::local_server_uuid(db).map_err(db_error)?;
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT source_type,name FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
            params![app_id, owner],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(db_error)?;
    let Some((source, _)) = row else {
        return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن فعال پیدا نشد."));
    };
    if source != "docker" {
        return Err(error(
            StatusCode::CONFLICT,
            "اتصال کانتینر به اپلیکیشنی که منبع آن Docker نیست مجاز نیست.",
        ));
    }
    Ok(owner)
}

pub(super) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let links = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
        owned_app(&db, &app_id)?;
        let mut statement=db.prepare("SELECT container_id,service_name,created_by,created_at FROM app_container_links WHERE application_id=?1 ORDER BY service_name COLLATE NOCASE").map_err(db_error)?;
        let collected=statement.query_map([&app_id],|row|Ok(json!({"container_id":row.get::<_,String>(0)?,"service_name":row.get::<_,String>(1)?,"created_by":row.get::<_,String>(2)?,"created_at":row.get::<_,i64>(3)?}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
        collected
    };
    let containers = inventory().await?;
    let rows = links
        .into_iter()
        .map(|mut link| {
            let id = link["container_id"].as_str().unwrap_or_default();
            if let Some(container) = containers.iter().find(|item| {
                item["id"].as_str().is_some_and(|candidate| {
                    candidate == id || candidate.starts_with(id) || id.starts_with(candidate)
                })
            }) {
                link["container"] = container.clone();
                link["state"] = container["state"].clone();
            } else {
                link["container"] = Value::Null;
                link["state"] = json!("missing");
            }
            link
        })
        .collect::<Vec<_>>();
    let claimed = rows
        .iter()
        .map(|item| item["container_id"].as_str().unwrap_or_default())
        .collect::<std::collections::HashSet<_>>();
    let unmanaged = containers
        .into_iter()
        .filter(|item| {
            item["id"].as_str().is_some_and(|id| {
                !claimed
                    .iter()
                    .any(|link| id.starts_with(link) || link.starts_with(id))
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(
        json!({"links":rows,"unmanaged_containers":unmanaged,"ownership":"metadata_link_only","can_control":false}),
    ))
}

pub(super) async fn link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<LinkInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !input.confirmed
        || !valid_container_id(&input.container_id)
        || !valid_service_name(&input.service_name)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "تأیید اتصال، شناسهٔ Docker معتبر و نام سرویس لازم است.",
        ));
    }
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
        owned_app(&db, &app_id)?;
    }
    let actual = inventory().await?;
    let found = actual
        .iter()
        .filter(|item| {
            item["id"]
                .as_str()
                .is_some_and(|id| id.starts_with(&input.container_id))
        })
        .collect::<Vec<_>>();
    if found.len() != 1 {
        return Err(error(
            StatusCode::NOT_FOUND,
            "شناسهٔ کانتینر در فهرست فعلی Docker یکتا یا موجود نیست.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    owned_app(&db, &app_id)?;
    db.execute("INSERT INTO app_container_links(application_id,container_id,service_name,created_by,created_at) VALUES(?1,?2,?3,?4,?5)",params![app_id,found[0]["id"].as_str().unwrap_or_default(),input.service_name,actor,now()]).map_err(|problem|if problem.sqlite_error_code()==Some(rusqlite::ErrorCode::ConstraintViolation){error(StatusCode::CONFLICT,"این کانتینر یا نام سرویس قبلاً به اپی متصل شده است.")}else{db_error(problem)})?;
    audit(
        &db,
        &actor,
        "app.container.link",
        &format!("app={app_id}; container={}", input.container_id),
    );
    Ok(Json(
        json!({"linked":true,"ownership":"metadata_link_only","container":found[0]}),
    ))
}

pub(super) async fn unlink(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((app_id, container_id)): Path<(String, String)>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    owned_app(&db, &app_id)?;
    let changed = db
        .execute(
            "DELETE FROM app_container_links WHERE application_id=?1 AND container_id=?2",
            params![app_id, container_id],
        )
        .map_err(db_error)?;
    if changed == 0 {
        return Err(error(StatusCode::NOT_FOUND, "اتصال کانتینر پیدا نشد."));
    }
    audit(
        &db,
        &actor,
        "app.container.unlink",
        &format!("app={app_id}; container={container_id}"),
    );
    Ok(Json(json!({"unlinked":true,"docker_untouched":true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn container_link_inputs_are_bounded_and_hex_only() {
        assert!(valid_container_id("0123456789ab"));
        assert!(!valid_container_id("../etc/passwd"));
        assert!(!valid_container_id("123"));
        assert!(valid_service_name("web-api_1"));
        assert!(!valid_service_name("../web"));
    }
}
