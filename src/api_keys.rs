use super::*;

const SCOPES: &[&str] = &["apps:read", "deployments:read"];
const REQUESTS_PER_MINUTE: i64 = 60;

pub(super) async fn no_store(request: Request, next: Next) -> Response {
    let api_path = is_api_path(request.uri().path());
    let mut response = next.run(request).await;
    if api_path {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, private, max-age=0"),
        );
    }
    response
}

fn is_api_path(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateInput {
    label: String,
    expires_days: i64,
}

#[derive(Deserialize)]
pub(super) struct ListQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS public_api_keys (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            label TEXT NOT NULL,
            token_hash TEXT NOT NULL UNIQUE,
            token_prefix TEXT NOT NULL,
            scopes_json TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            revoked_at INTEGER,
            last_used_at INTEGER
         );
         CREATE INDEX IF NOT EXISTS public_api_keys_user ON public_api_keys(user_id, created_at DESC);
         CREATE TABLE IF NOT EXISTS public_api_key_usage (
            key_id INTEGER PRIMARY KEY REFERENCES public_api_keys(id) ON DELETE CASCADE,
            window_start INTEGER NOT NULL,
            request_count INTEGER NOT NULL DEFAULT 0
         );",
    )
}

pub(super) fn revoke_if_owner_ineligible(
    db: &Connection,
    user_id: i64,
    role: &str,
    enabled: bool,
) -> rusqlite::Result<usize> {
    if enabled && role == "admin" {
        return Ok(0);
    }
    db.execute(
        "UPDATE public_api_keys SET revoked_at=?1 WHERE user_id=?2 AND revoked_at IS NULL",
        params![now(), user_id],
    )
}

pub(super) async fn list(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let mut statement = db.prepare(
        "SELECT k.id,u.username,k.label,k.token_prefix,k.scopes_json,k.created_at,k.expires_at,k.revoked_at,k.last_used_at,
         COALESCE((SELECT request_count FROM public_api_key_usage usage WHERE usage.key_id=k.id AND usage.window_start=?1),0)
         FROM public_api_keys k JOIN users u ON u.id=k.user_id ORDER BY k.created_at DESC LIMIT 200",
    ).map_err(db_error)?;
    let current_window = now() / 60 * 60;
    let rows = statement
        .query_map([current_window], |row| {
            let scopes: String = row.get(4)?;
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "username": row.get::<_, String>(1)?,
                "label": row.get::<_, String>(2)?,
                "prefix": row.get::<_, String>(3)?,
                "scopes": serde_json::from_str::<Vec<String>>(&scopes).unwrap_or_default(),
                "created_at": row.get::<_, i64>(5)?,
                "expires_at": row.get::<_, i64>(6)?,
                "revoked_at": row.get::<_, Option<i64>>(7)?,
                "last_used_at": row.get::<_, Option<i64>>(8)?,
                "requests_this_minute": row.get::<_, i64>(9)?
            }))
        })
        .map_err(db_error)?;
    let keys: Vec<Value> = rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
    Ok(Json(json!({"keys":keys,"max_returned":200})))
}

pub(super) async fn create(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(input): Json<CreateInput>,
) -> ApiResult {
    if !secure_transport(&headers, peer.ip()) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "API keys can only be issued over HTTPS",
        ));
    }
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let user = require_admin(&db, &headers)?;
    let label = input.label.trim();
    if label.is_empty() || label.len() > 80 || input.expires_days < 1 || input.expires_days > 365 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Label must be 1–80 characters and expiry must be 1–365 days",
        ));
    }
    let active: i64 = db.query_row(
        "SELECT COUNT(*) FROM public_api_keys WHERE user_id=?1 AND revoked_at IS NULL AND expires_at>?2",
        params![user.id, now()], |row| row.get(0),
    ).map_err(db_error)?;
    if active >= 10 {
        return Err(error(
            StatusCode::CONFLICT,
            "At most 10 active API keys are allowed per administrator",
        ));
    }
    let raw = format!("arg_live_{}", token());
    let hash = session_hash(&raw);
    let prefix = raw.chars().take(18).collect::<String>();
    let created = now();
    let expires = created.saturating_add(input.expires_days.saturating_mul(86_400));
    let scopes: Vec<&str> = SCOPES.to_vec();
    db.execute(
        "INSERT INTO public_api_keys(user_id,label,token_hash,token_prefix,scopes_json,created_at,expires_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![user.id, label, hash, prefix, serde_json::to_string(&scopes).unwrap_or_else(|_| "[]".into()), created, expires],
    ).map_err(db_error)?;
    let id = db.last_insert_rowid();
    audit(
        &db,
        &user.username,
        "api_key.create",
        &format!("Created read-only API key id={id}, scopes=apps:read,deployments:read"),
    );
    Ok(Json(json!({
        "id": id,
        "label": label,
        "token": raw,
        "prefix": prefix,
        "scopes": scopes,
        "created_at": created,
        "expires_at": expires,
        "notice": "The raw token is shown once. Store it in a secret manager; it cannot be retrieved later."
    })))
}

pub(super) async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let user = require_admin(&db, &headers)?;
    let changed = db
        .execute(
            "UPDATE public_api_keys SET revoked_at=?1 WHERE id=?2 AND revoked_at IS NULL",
            params![now(), id],
        )
        .map_err(db_error)?;
    if changed == 0 {
        return Err(error(
            StatusCode::NOT_FOUND,
            "API key not found or already revoked",
        ));
    }
    audit(
        &db,
        &user.username,
        "api_key.revoke",
        &format!("Revoked API key id={id}"),
    );
    Ok(Json(json!({"ok":true,"revoked_id":id})))
}

fn authorize(
    db: &Connection,
    headers: &HeaderMap,
    required_scope: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| value.starts_with("arg_live_") && value.len() == 73)
        .ok_or_else(|| {
            error(
                StatusCode::UNAUTHORIZED,
                "A valid ARG API bearer token is required",
            )
        })?;
    let hash = session_hash(bearer);
    let record: Option<(i64, String, i64)> = db.query_row(
        "SELECT k.id,k.scopes_json,k.expires_at FROM public_api_keys k
         JOIN users u ON u.id=k.user_id AND u.role='admin' AND u.enabled=1
         WHERE k.token_hash=?1 AND k.revoked_at IS NULL",
        [&hash], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).optional().map_err(db_error)?;
    let Some((id, scopes_json, expires_at)) = record else {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "API key is invalid or revoked",
        ));
    };
    if expires_at <= now() {
        return Err(error(StatusCode::UNAUTHORIZED, "API key has expired"));
    }
    let scopes: Vec<String> = serde_json::from_str(&scopes_json).unwrap_or_default();
    if !scopes.iter().any(|scope| scope == required_scope) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "API key does not include the required scope",
        ));
    }
    let window = now() / 60 * 60;
    db.execute(
        "INSERT INTO public_api_key_usage(key_id,window_start,request_count) VALUES (?1,?2,1)
         ON CONFLICT(key_id) DO UPDATE SET request_count=CASE WHEN window_start<?2 THEN 1 ELSE request_count+1 END,window_start=?2",
        params![id, window],
    ).map_err(db_error)?;
    let count: i64 = db
        .query_row(
            "SELECT request_count FROM public_api_key_usage WHERE key_id=?1",
            [id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if count > REQUESTS_PER_MINUTE {
        return Err(error(
            StatusCode::TOO_MANY_REQUESTS,
            "API key rate limit exceeded (60 requests per minute)",
        ));
    }
    db.execute(
        "UPDATE public_api_keys SET last_used_at=?1 WHERE id=?2",
        params![now(), id],
    )
    .map_err(db_error)?;
    Ok(())
}

fn pagination(query: &ListQuery) -> (i64, i64) {
    (
        query.limit.unwrap_or(50).clamp(1, 100),
        query.offset.unwrap_or(0).clamp(0, 1_000_000),
    )
}

pub(super) async fn apps_read(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> ApiResult {
    if !secure_transport(&headers, peer.ip()) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "Public API keys may only be used over HTTPS",
        ));
    }
    let db = state.lock().unwrap();
    authorize(&db, &headers, "apps:read")?;
    let (limit, offset) = pagination(&query);
    let mut statement = db.prepare(
        "SELECT id,name,framework,runtime_version,domain,desired_state,observed_state,lifecycle_state,updated_at
         FROM applications WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT ?1 OFFSET ?2",
    ).map_err(db_error)?;
    let rows = statement.query_map(params![limit, offset], |row| Ok(json!({
        "id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"framework":row.get::<_,String>(2)?,
        "runtime_version":row.get::<_,String>(3)?,"domain":row.get::<_,Option<String>>(4)?,
        "desired_state":row.get::<_,String>(5)?,"observed_state":row.get::<_,String>(6)?,
        "lifecycle_state":row.get::<_,String>(7)?,"updated_at":row.get::<_,i64>(8)?
    }))).map_err(db_error)?;
    let items: Vec<Value> = rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
    Ok(Json(
        json!({"data":items,"pagination":{"limit":limit,"offset":offset,"count":items.len()}}),
    ))
}

pub(super) async fn deployments_read(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> ApiResult {
    if !secure_transport(&headers, peer.ip()) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "Public API keys may only be used over HTTPS",
        ));
    }
    let db = state.lock().unwrap();
    authorize(&db, &headers, "deployments:read")?;
    let (limit, offset) = pagination(&query);
    let mut statement = db.prepare(
        "SELECT j.id,j.application_id,a.name,j.state,j.source_commit,j.created_at,j.started_at,j.finished_at
         FROM deployment_jobs j JOIN applications a ON a.id=j.application_id
         ORDER BY j.created_at DESC LIMIT ?1 OFFSET ?2",
    ).map_err(db_error)?;
    let rows = statement.query_map(params![limit, offset], |row| Ok(json!({
        "id":row.get::<_,String>(0)?,"application_id":row.get::<_,String>(1)?,"application_name":row.get::<_,String>(2)?,
        "state":row.get::<_,String>(3)?,"source_commit":row.get::<_,Option<String>>(4)?,"created_at":row.get::<_,i64>(5)?,
        "started_at":row.get::<_,Option<i64>>(6)?,"finished_at":row.get::<_,Option<i64>>(7)?
    }))).map_err(db_error)?;
    let items: Vec<Value> = rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
    Ok(Json(
        json!({"data":items,"pagination":{"limit":limit,"offset":offset,"count":items.len()}}),
    ))
}

pub(super) async fn openapi() -> Json<Value> {
    Json(json!({
        "openapi":"3.1.0","info":{"title":"ARG Public Read API","version":"v1","description":"Read-only, rate-limited API. Tokens are scoped, expiring and shown once."},
        "servers":[{"url":"/api/v1"}],"security":[{"BearerAuth":[]}],
        "components":{"securitySchemes":{"BearerAuth":{"type":"http","scheme":"bearer"}}},
        "paths":{
            "/apps":{"get":{"summary":"List applications","parameters":[{"name":"limit","in":"query","schema":{"type":"integer","minimum":1,"maximum":100}},{"name":"offset","in":"query","schema":{"type":"integer","minimum":0}}],"responses":{"200":{"description":"Application summaries"},"401":{"description":"Invalid, revoked or expired key"},"429":{"description":"Rate limit exceeded"}}}},
            "/deployments":{"get":{"summary":"List deployment summaries","parameters":[{"name":"limit","in":"query","schema":{"type":"integer","minimum":1,"maximum":100}},{"name":"offset","in":"query","schema":{"type":"integer","minimum":0}}],"responses":{"200":{"description":"Deployment summaries without source configuration or credentials"},"401":{"description":"Invalid, revoked or expired key"},"429":{"description":"Rate limit exceeded"}}}}
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        ListQuery, authorize, is_api_path, migrate, pagination, revoke_if_owner_ineligible,
    };
    use crate::{session_hash, token};
    use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
    use rusqlite::Connection;

    #[test]
    fn public_api_pagination_is_bounded() {
        assert_eq!(
            pagination(&ListQuery {
                limit: Some(5000),
                offset: Some(-2)
            }),
            (100, 0)
        );
        assert_eq!(
            pagination(&ListQuery {
                limit: None,
                offset: None
            }),
            (50, 0)
        );
        assert_eq!(
            pagination(&ListQuery {
                limit: Some(0),
                offset: Some(2_000_000)
            }),
            (1, 1_000_000)
        );
    }

    #[test]
    fn cache_policy_covers_api_routes_without_changing_static_assets() {
        assert!(is_api_path("/api/state"));
        assert!(is_api_path("/api/apps/123"));
        assert!(is_api_path("/api"));
        assert!(!is_api_path("/"));
        assert!(!is_api_path("/assets/app.js"));
    }

    #[test]
    fn api_keys_are_hashed_scoped_expiring_revocable_and_rate_limited() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE users(id INTEGER PRIMARY KEY,username TEXT,role TEXT,enabled INTEGER); INSERT INTO users VALUES(1,'admin','admin',1);").unwrap();
        migrate(&db).unwrap();
        let raw = format!("arg_live_{}", token());
        let hash = session_hash(&raw);
        let expires = crate::now() + 3600;
        db.execute("INSERT INTO public_api_keys(user_id,label,token_hash,token_prefix,scopes_json,created_at,expires_at) VALUES(1,'test',?1,'arg_live_test','[\"apps:read\"]',?2,?3)",rusqlite::params![hash,crate::now(),expires]).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {raw}")).unwrap(),
        );
        assert!(authorize(&db, &headers, "apps:read").is_ok());
        assert!(matches!(
            authorize(&db, &headers, "deployments:read"),
            Err((StatusCode::FORBIDDEN, _))
        ));
        for _ in 1..60 {
            assert!(authorize(&db, &headers, "apps:read").is_ok());
        }
        assert!(matches!(
            authorize(&db, &headers, "apps:read"),
            Err((StatusCode::TOO_MANY_REQUESTS, _))
        ));
        db.execute(
            "UPDATE public_api_keys SET expires_at=?1",
            [crate::now() - 1],
        )
        .unwrap();
        assert!(matches!(
            authorize(&db, &headers, "apps:read"),
            Err((StatusCode::UNAUTHORIZED, _))
        ));
        db.execute(
            "UPDATE public_api_keys SET expires_at=?1,revoked_at=?2",
            rusqlite::params![crate::now() + 3600, crate::now()],
        )
        .unwrap();
        assert!(matches!(
            authorize(&db, &headers, "apps:read"),
            Err((StatusCode::UNAUTHORIZED, _))
        ));
        let persisted: String = db
            .query_row("SELECT token_hash FROM public_api_keys", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_ne!(persisted, raw);
    }

    #[test]
    fn api_keys_are_revoked_when_owner_loses_administrator_access() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE users(id INTEGER PRIMARY KEY,username TEXT,role TEXT,enabled INTEGER);
             INSERT INTO users VALUES(1,'admin','admin',1);",
        )
        .unwrap();
        migrate(&db).unwrap();
        let raw = format!("arg_live_{}", token());
        db.execute(
            "INSERT INTO public_api_keys(user_id,label,token_hash,token_prefix,scopes_json,created_at,expires_at)
             VALUES(1,'test',?1,'arg_live_test','[\"apps:read\"]',?2,?3)",
            rusqlite::params![session_hash(&raw), crate::now(), crate::now() + 3600],
        )
        .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {raw}")).unwrap(),
        );
        assert!(authorize(&db, &headers, "apps:read").is_ok());

        db.execute("UPDATE users SET role='viewer' WHERE id=1", [])
            .unwrap();
        assert_eq!(revoke_if_owner_ineligible(&db, 1, "viewer", true).unwrap(), 1);
        assert!(matches!(
            authorize(&db, &headers, "apps:read"),
            Err((StatusCode::UNAUTHORIZED, _))
        ));

        db.execute("UPDATE users SET role='admin',enabled=1 WHERE id=1", [])
            .unwrap();
        assert!(matches!(
            authorize(&db, &headers, "apps:read"),
            Err((StatusCode::UNAUTHORIZED, _))
        ));
    }
}
