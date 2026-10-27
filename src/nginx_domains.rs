use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkInput {
    site_name: String,
    confirmed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UnlinkInput {
    confirmed: bool,
}

fn valid_site(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 180
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS app_nginx_sites(site_name TEXT PRIMARY KEY,application_id TEXT NOT NULL UNIQUE REFERENCES applications(id) ON DELETE CASCADE,created_by TEXT NOT NULL,created_at INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS app_nginx_sites_app ON app_nginx_sites(application_id);")
}

pub(super) async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let app: Option<(String, Option<String>)> = db
        .query_row(
            "SELECT name,domain FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
            params![app_id, owner],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(db_error)?;
    let Some((name, domain)) = app else {
        return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن فعال پیدا نشد."));
    };
    let site: Option<(String, String, i64)> = db
        .query_row(
            "SELECT site_name,created_by,created_at FROM app_nginx_sites WHERE application_id=?1",
            [&app_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(db_error)?;
    Ok(Json(
        json!({"application_id":app_id,"application_name":name,"domain":domain,"site":site.map(|(site_name,created_by,created_at)|json!({"name":site_name,"created_by":created_by,"created_at":created_at})),"config_mutated":false}),
    ))
}

pub(super) async fn link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<LinkInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !input.confirmed || !valid_site(&input.site_name) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "تأیید اتصال و نام معتبر سایت Nginx لازم است.",
        ));
    }
    let (actor, owner, domain) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let domain: Option<String> = db
            .query_row(
                "SELECT domain FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
                params![app_id, owner],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "اپلیکیشن فعال پیدا نشد."))?;
        let Some(domain) = domain.filter(|value| !value.trim().is_empty()) else {
            return Err(error(
                StatusCode::CONFLICT,
                "ابتدا دامنهٔ اپلیکیشن را ثبت کن تا مالکیت سایت Nginx قابل بررسی باشد.",
            ));
        };
        (
            actor,
            owner,
            domain.trim().trim_end_matches('.').to_ascii_lowercase(),
        )
    };
    let response =
        nginx::nginx_manager_request_async(json!({"action":"nginx-get","name":input.site_name}))
            .await?;
    let content = nginx::nginx_site_content(&response)?;
    let names = nginx::nginx_server_names(content)
        .into_iter()
        .map(|value| value.trim_end_matches('.').to_ascii_lowercase())
        .collect::<Vec<_>>();
    if !names.iter().any(|name| name == &domain) {
        return Err(error(
            StatusCode::CONFLICT,
            "دامنهٔ ثبت‌شده برای اپلیکیشن در server_name این سایت Nginx پیدا نشد؛ پیکربندی تغییر نکرد.",
        ));
    }
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let current_owner = federation::local_server_uuid(&db).map_err(db_error)?;
    if current_owner != owner {
        return Err(error(
            StatusCode::CONFLICT,
            "هویت VPS هنگام ثبت لینک تغییر کرد.",
        ));
    }
    let linked = db.execute(
        "INSERT INTO app_nginx_sites(site_name,application_id,created_by,created_at) VALUES(?1,?2,?3,?4) ON CONFLICT(application_id) DO UPDATE SET site_name=excluded.site_name,created_by=excluded.created_by,created_at=excluded.created_at",
        params![input.site_name, app_id, actor, now()],
    );
    if let Err(problem) = linked {
        return Err(
            if problem.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                error(
                    StatusCode::CONFLICT,
                    "این سایت Nginx قبلاً به اپلیکیشن دیگری متصل شده است.",
                )
            } else {
                db_error(problem)
            },
        );
    }
    audit(
        &db,
        &actor,
        "app.nginx.link",
        &format!("app={app_id}; site={}", input.site_name),
    );
    Ok(Json(
        json!({"linked":true,"application_id":app_id,"site_name":input.site_name,"domain":domain,"config_mutated":false}),
    ))
}

pub(super) async fn unlink(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<UnlinkInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !input.confirmed {
        return Err(error(StatusCode::BAD_REQUEST, "تأیید قطع لینک لازم است."));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let changed = db.execute(
        "DELETE FROM app_nginx_sites WHERE application_id=?1 AND EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2)",
        params![app_id, owner],
    ).map_err(db_error)?;
    if changed == 0 {
        return Err(error(
            StatusCode::NOT_FOUND,
            "لینک Nginx این اپلیکیشن پیدا نشد.",
        ));
    }
    audit(&db, &actor, "app.nginx.unlink", &format!("app={app_id}"));
    Ok(Json(json!({"unlinked":true,"nginx_untouched":true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn site_names_are_path_safe() {
        assert!(valid_site("example.com"));
        assert!(!valid_site("../sites-enabled/x"));
        assert!(!valid_site(""));
    }
}
