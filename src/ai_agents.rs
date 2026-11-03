use super::*;

#[derive(RustEmbed)]
#[folder = "docs/"]
struct AgentDocs;

#[derive(RustEmbed)]
#[folder = "src/"]
struct AgentRustSource;

#[derive(RustEmbed)]
#[folder = "frontend/src/"]
struct AgentFrontendSource;

const AGENT_SCOPES: &[&str] = &[
    "apps:read",
    "deployments:read",
    "docs:read",
    "repository:read",
    "deployments:propose",
    "terminal:root:propose",
];
const AGENT_REQUESTS_PER_MINUTE: i64 = 30;
const MAX_PENDING_PROPOSALS: i64 = 20;
const MAX_AGENT_LIFETIME_DAYS: i64 = 30;
const MAX_ROOT_COMMAND_BYTES: usize = 1200;
const ROOT_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const MAX_ROOT_COMMAND_OUTPUT_BYTES: usize = 16 * 1024;
static ROOT_COMMAND_OUTPUTS: OnceLock<Mutex<HashMap<String, (i64, String, i64)>>> = OnceLock::new();
static ROOT_COMMANDS: OnceLock<Mutex<HashMap<String, (i64, String, i64)>>> = OnceLock::new();

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateAgentInput {
    label: String,
    expires_days: i64,
    scopes: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UpdateAgentScopesInput {
    scopes: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct ApproveProposalInput {
    password: Option<String>,
    confirmation: Option<String>,
}

#[derive(Clone)]
struct AgentContext {
    id: i64,
    user_id: i64,
    username: String,
    label: String,
    scopes: Vec<String>,
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS ai_agent_tokens (
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
         CREATE INDEX IF NOT EXISTS ai_agent_tokens_user ON ai_agent_tokens(user_id, created_at DESC);
         CREATE TABLE IF NOT EXISTS ai_agent_usage (
            token_id INTEGER PRIMARY KEY REFERENCES ai_agent_tokens(id) ON DELETE CASCADE,
            window_start INTEGER NOT NULL,
            request_count INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS ai_agent_proposals (
            id TEXT PRIMARY KEY,
            token_id INTEGER REFERENCES ai_agent_tokens(id) ON DELETE SET NULL,
            user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            action_type TEXT NOT NULL,
            application_id TEXT NOT NULL,
            action_json TEXT NOT NULL DEFAULT '{}',
            reason TEXT NOT NULL DEFAULT '',
            state TEXT NOT NULL CHECK(state IN ('pending','approving','approved','rejected','expired','failed')),
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            reviewed_at INTEGER,
            reviewed_by TEXT,
            result_json TEXT,
            result_error TEXT
         );
         CREATE INDEX IF NOT EXISTS ai_agent_proposals_state ON ai_agent_proposals(state, created_at DESC);
         CREATE INDEX IF NOT EXISTS ai_agent_proposals_user ON ai_agent_proposals(user_id, created_at DESC);",
    )?;
    let has_action_json = {
        let mut statement = db.prepare("PRAGMA table_info(ai_agent_proposals)")?;
        let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
        columns
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|name| name == "action_json")
    };
    if !has_action_json {
        db.execute(
            "ALTER TABLE ai_agent_proposals ADD COLUMN action_json TEXT NOT NULL DEFAULT '{}'",
            [],
        )?;
    }
    Ok(())
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
    let token_ids = {
        let mut statement = db.prepare("SELECT id FROM ai_agent_tokens WHERE user_id=?1")?;
        let rows = statement.query_map([user_id], |row| row.get::<_, i64>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let changed = db.execute(
        "UPDATE ai_agent_tokens SET revoked_at=?1 WHERE user_id=?2 AND revoked_at IS NULL",
        params![now(), user_id],
    )?;
    db.execute("UPDATE ai_agent_proposals SET state='rejected',action_json='{}',reviewed_at=?1,reviewed_by='system:owner-ineligible' WHERE user_id=?2 AND state='pending'", params![now(),user_id])?;
    if let Some(commands) = ROOT_COMMANDS.get() {
        let ids = token_ids.iter().copied().collect::<HashSet<_>>();
        commands
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, (token_id, _, _)| !ids.contains(token_id));
    }
    if let Some(outputs) = ROOT_COMMAND_OUTPUTS.get() {
        let ids = token_ids.into_iter().collect::<HashSet<_>>();
        outputs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, (token_id, _, _)| !ids.contains(token_id));
    }
    Ok(changed)
}

pub(super) async fn list(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> ApiResult {
    if !secure_transport(&headers, peer.ip()) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "مدیریت اتصال عامل فقط روی HTTPS مجاز است.",
        ));
    }
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let mut statement = db.prepare(
        "SELECT t.id,u.username,t.label,t.token_prefix,t.scopes_json,t.created_at,t.expires_at,t.revoked_at,t.last_used_at,
         COALESCE((SELECT request_count FROM ai_agent_usage usage WHERE usage.token_id=t.id AND usage.window_start=?1),0)
         FROM ai_agent_tokens t JOIN users u ON u.id=t.user_id ORDER BY t.created_at DESC LIMIT 100",
    ).map_err(db_error)?;
    let window = now() / 60 * 60;
    let rows = statement.query_map([window], |row| {
        let scopes: String = row.get(4)?;
        Ok(json!({
            "id":row.get::<_,i64>(0)?,"username":row.get::<_,String>(1)?,"label":row.get::<_,String>(2)?,
            "prefix":row.get::<_,String>(3)?,"scopes":serde_json::from_str::<Vec<String>>(&scopes).unwrap_or_default(),
            "created_at":row.get::<_,i64>(5)?,"expires_at":row.get::<_,i64>(6)?,"revoked_at":row.get::<_,Option<i64>>(7)?,
            "last_used_at":row.get::<_,Option<i64>>(8)?,"requests_this_minute":row.get::<_,i64>(9)?
        }))
    }).map_err(db_error)?;
    let tokens: Vec<Value> = rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
    Ok(Json(
        json!({"tokens":tokens,"max_lifetime_days":MAX_AGENT_LIFETIME_DAYS,"requests_per_minute":AGENT_REQUESTS_PER_MINUTE}),
    ))
}

pub(super) async fn create(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(input): Json<CreateAgentInput>,
) -> ApiResult {
    if !secure_transport(&headers, peer.ip()) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "AI agent credentials can only be issued over HTTPS",
        ));
    }
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let admin = require_admin(&db, &headers)?;
    let label = input.label.trim();
    if label.is_empty()
        || label.len() > 80
        || label.chars().any(char::is_control)
        || !(1..=MAX_AGENT_LIFETIME_DAYS).contains(&input.expires_days)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام اتصال باید ۱ تا ۸۰ نویسه و اعتبار آن ۱ تا ۳۰ روز باشد.",
        ));
    }
    if input.scopes.is_empty()
        || input
            .scopes
            .iter()
            .any(|scope| !AGENT_SCOPES.contains(&scope.as_str()))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "مجوز انتخاب‌شده پشتیبانی نمی‌شود.",
        ));
    }
    let scopes = normalized_scopes(&input.scopes);
    let active: i64 = db.query_row(
        "SELECT COUNT(*) FROM ai_agent_tokens WHERE user_id=?1 AND revoked_at IS NULL AND expires_at>?2",
        params![admin.id, now()], |row| row.get(0),
    ).map_err(db_error)?;
    if active >= 10 {
        return Err(error(
            StatusCode::CONFLICT,
            "برای هر مدیر حداکثر ۱۰ اتصال فعال عامل هوش مصنوعی مجاز است.",
        ));
    }
    let raw = format!("arg_agent_{}", token());
    let prefix = raw.chars().take(19).collect::<String>();
    let created_at = now();
    let expires_at = created_at.saturating_add(input.expires_days * 86_400);
    db.execute(
        "INSERT INTO ai_agent_tokens(user_id,label,token_hash,token_prefix,scopes_json,created_at,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![admin.id,label,session_hash(&raw),prefix,serde_json::to_string(&scopes).unwrap_or_else(|_| "[]".into()),created_at,expires_at],
    ).map_err(db_error)?;
    let id = db.last_insert_rowid();
    audit(
        &db,
        &admin.username,
        "ai_agent.token.create",
        &format!("token_id={id}; scopes={}", scopes.join(",")),
    );
    Ok(Json(
        json!({"id":id,"label":label,"token":raw,"prefix":prefix,"scopes":scopes,"created_at":created_at,"expires_at":expires_at,"notice":"توکن فقط یک‌بار نمایش داده می‌شود؛ آن را در Secret Manager نگه‌دار."}),
    ))
}

pub(super) async fn update_scopes(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(input): Json<UpdateAgentScopesInput>,
) -> ApiResult {
    if !secure_transport(&headers, peer.ip()) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "تغییر مجوز عامل فقط روی HTTPS مجاز است.",
        ));
    }
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let admin = require_admin(&db, &headers)?;
    if input.scopes.is_empty()
        || input
            .scopes
            .iter()
            .any(|scope| !AGENT_SCOPES.contains(&scope.as_str()))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "مجوز انتخاب‌شده پشتیبانی نمی‌شود.",
        ));
    }
    let scopes = normalized_scopes(&input.scopes);
    let prior: Option<String> = db.query_row(
        "SELECT scopes_json FROM ai_agent_tokens WHERE id=?1 AND user_id=?2 AND revoked_at IS NULL AND expires_at>?3",
        params![id, admin.id, now()],
        |row| row.get(0),
    ).optional().map_err(db_error)?;
    let Some(prior) = prior else {
        return Err(error(StatusCode::NOT_FOUND, "توکن فعال پیدا نشد."));
    };
    db.execute("UPDATE ai_agent_tokens SET scopes_json=?1 WHERE id=?2 AND user_id=?3 AND revoked_at IS NULL", params![serde_json::to_string(&scopes).unwrap_or_else(|_| "[]".into()),id,admin.id]).map_err(db_error)?;
    if !scopes.iter().any(|scope| scope == "terminal:root:propose") {
        db.execute("UPDATE ai_agent_proposals SET state='rejected',action_json='{}',reviewed_at=?1,reviewed_by='system:root-scope-removed' WHERE token_id=?2 AND action_type='root_command' AND state='pending'", params![now(),id]).map_err(db_error)?;
        if let Some(outputs) = ROOT_COMMAND_OUTPUTS.get() {
            outputs
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .retain(|_, (token_id, _, _)| *token_id != id);
        }
        if let Some(commands) = ROOT_COMMANDS.get() {
            commands
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .retain(|_, (token_id, _, _)| *token_id != id);
        }
    }
    let prior_scopes: Vec<String> = serde_json::from_str(&prior).unwrap_or_default();
    audit(
        &db,
        &admin.username,
        "ai_agent.token.scopes_update",
        &format!(
            "token_id={id}; previous={}; current={}",
            prior_scopes.join(","),
            scopes.join(",")
        ),
    );
    Ok(Json(json!({"ok":true,"id":id,"scopes":scopes})))
}

fn normalized_scopes(selected: &[String]) -> Vec<String> {
    AGENT_SCOPES
        .iter()
        .filter(|scope| selected.iter().any(|value| value == **scope))
        .map(|scope| (*scope).to_owned())
        .collect()
}

pub(super) async fn revoke(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    if !secure_transport(&headers, peer.ip()) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "لغو اتصال عامل فقط روی HTTPS مجاز است.",
        ));
    }
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let admin = require_admin(&db, &headers)?;
    let changed = db
        .execute(
            "UPDATE ai_agent_tokens SET revoked_at=?1 WHERE id=?2 AND revoked_at IS NULL",
            params![now(), id],
        )
        .map_err(db_error)?;
    if changed == 0 {
        return Err(error(StatusCode::NOT_FOUND, "اتصال فعال عامل پیدا نشد."));
    }
    db.execute("UPDATE ai_agent_proposals SET state='rejected',action_json='{}',reviewed_at=?1,reviewed_by='system:token-revoked' WHERE token_id=?2 AND state='pending'", params![now(),id]).map_err(db_error)?;
    if let Some(outputs) = ROOT_COMMAND_OUTPUTS.get() {
        outputs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, (token_id, _, _)| *token_id != id);
    }
    if let Some(commands) = ROOT_COMMANDS.get() {
        commands
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, (token_id, _, _)| *token_id != id);
    }
    audit(
        &db,
        &admin.username,
        "ai_agent.token.revoke",
        &format!("token_id={id}"),
    );
    Ok(Json(json!({"ok":true,"revoked_id":id})))
}

fn authorize(
    db: &Connection,
    headers: &HeaderMap,
    required_scope: Option<&str>,
) -> Result<AgentContext, (StatusCode, Json<Value>)> {
    let raw = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| value.starts_with("arg_agent_") && value.len() == 74)
        .ok_or_else(|| {
            error(
                StatusCode::UNAUTHORIZED,
                "توکن معتبر عامل هوش مصنوعی لازم است.",
            )
        })?;
    let record = db.query_row(
        "SELECT t.id,t.user_id,u.username,t.label,t.scopes_json,t.expires_at FROM ai_agent_tokens t JOIN users u ON u.id=t.user_id AND u.role='admin' AND u.enabled=1 WHERE t.token_hash=?1 AND t.revoked_at IS NULL",
        [session_hash(raw)],
        |row| Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,i64>(5)?)),
    ).optional().map_err(db_error)?.ok_or_else(|| error(StatusCode::UNAUTHORIZED, "توکن عامل نامعتبر یا لغوشده است."))?;
    let (id, user_id, username, label, scopes_json, expires_at) = record;
    if expires_at <= now() {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "اعتبار توکن عامل پایان یافته است.",
        ));
    }
    let scopes = serde_json::from_str::<Vec<String>>(&scopes_json).unwrap_or_default();
    if required_scope.is_some_and(|required| !scopes.iter().any(|scope| scope == required)) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "این توکن مجوز لازم برای این قابلیت را ندارد.",
        ));
    }
    let window = now() / 60 * 60;
    db.execute("INSERT INTO ai_agent_usage(token_id,window_start,request_count) VALUES(?1,?2,1) ON CONFLICT(token_id) DO UPDATE SET request_count=CASE WHEN window_start<?2 THEN 1 ELSE request_count+1 END,window_start=?2", params![id,window]).map_err(db_error)?;
    let count: i64 = db
        .query_row(
            "SELECT request_count FROM ai_agent_usage WHERE token_id=?1",
            [id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if count > AGENT_REQUESTS_PER_MINUTE {
        return Err(error(
            StatusCode::TOO_MANY_REQUESTS,
            "محدودیت درخواست عامل (۳۰ درخواست در دقیقه) رد شد.",
        ));
    }
    db.execute(
        "UPDATE ai_agent_tokens SET last_used_at=?1 WHERE id=?2",
        params![now(), id],
    )
    .map_err(db_error)?;
    Ok(AgentContext {
        id,
        user_id,
        username,
        label,
        scopes,
    })
}

pub(super) async fn mcp(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Response {
    if !secure_transport(&headers, peer.ip()) {
        return error(
            StatusCode::FORBIDDEN,
            "اتصال MCP فقط از طریق HTTPS مجاز است.",
        )
        .into_response();
    }
    let id = request.get("id").cloned();
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || request.get("method").and_then(Value::as_str).is_none()
    {
        return jsonrpc_error(id, -32600, "درخواست JSON-RPC معتبر نیست.");
    }
    let method = request["method"].as_str().unwrap_or_default();
    if let Some(version) = headers
        .get("mcp-protocol-version")
        .and_then(|value| value.to_str().ok())
    {
        if version != "2025-03-26" {
            return error(
                StatusCode::BAD_REQUEST,
                "نسخهٔ MCP در این endpoint پشتیبانی نمی‌شود.",
            )
            .into_response();
        }
    }
    let is_notification = !request.get("id").is_some();
    let mut db = state.lock().unwrap();
    let context = match authorize(&db, &headers, None) {
        Ok(context) => context,
        Err((status, body)) => return (status, body).into_response(),
    };
    if db.execute("UPDATE ai_agent_proposals SET state='expired',action_json=CASE WHEN action_type='root_command' THEN '{}' ELSE action_json END WHERE state='pending' AND expires_at<=?1", [now()]).is_err() {
        return error(StatusCode::INTERNAL_SERVER_ERROR, "درخواست‌های منقضی پاک‌سازی نشدند.").into_response();
    }
    if let Some(outputs) = ROOT_COMMAND_OUTPUTS.get() {
        outputs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, (_, _, expires)| *expires > now());
    }
    if let Some(commands) = ROOT_COMMANDS.get() {
        commands
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, (_, _, expires)| *expires > now());
    }
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion":"2025-03-26",
            "capabilities":{"tools":{"listChanged":false},"resources":{"subscribe":false,"listChanged":false}},
            "serverInfo":{"name":"arg-control-center","version":env!("CARGO_PKG_VERSION")},
            "instructions":"ARG MCP is scoped to the token's explicit permissions. Deployment and root-command operations are only submitted as proposals and require explicit approval in the ARG panel. Root commands additionally require fresh administrator password verification and a typed confirmation; no persistent interactive root shell is provided. Secrets and unrestricted API forwarding are not available."
        })),
        "notifications/initialized" => {
            return if is_notification {
                mcp_accepted()
            } else {
                jsonrpc_error(id, -32600, "اعلان MCP شناسهٔ پاسخ ندارد.")
            };
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": available_tools(&context.scopes)})),
        "resources/list" => Ok(json!({"resources": available_resources(&context.scopes)})),
        "resources/read" => read_resource(
            &context.scopes,
            params
                .get("uri")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        "tools/call" => Ok(match call_tool(&mut db, &context, &params) {
            Ok(result) => result,
            Err(message) => json!({"content":[{"type":"text","text":message}],"isError":true}),
        }),
        _ => return jsonrpc_error(id, -32601, "روش MCP پشتیبانی نمی‌شود."),
    };
    if is_notification {
        return mcp_accepted();
    }
    match result {
        Ok(result) => jsonrpc_result(id, result),
        Err(message) => jsonrpc_error(id, -32000, &message),
    }
}

fn jsonrpc_result(id: Option<Value>, result: Value) -> Response {
    let mut response = Json(json!({"jsonrpc":"2.0","id":id,"result":result})).into_response();
    response.headers_mut().insert(
        "mcp-protocol-version",
        HeaderValue::from_static("2025-03-26"),
    );
    response
}

fn mcp_accepted() -> Response {
    let mut response = StatusCode::ACCEPTED.into_response();
    response.headers_mut().insert(
        "mcp-protocol-version",
        HeaderValue::from_static("2025-03-26"),
    );
    response
}

fn jsonrpc_error(id: Option<Value>, code: i64, message: &str) -> Response {
    let mut response =
        Json(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}))
            .into_response();
    response.headers_mut().insert(
        "mcp-protocol-version",
        HeaderValue::from_static("2025-03-26"),
    );
    response
}

fn available_tools(scopes: &[String]) -> Vec<Value> {
    let mut tools = Vec::new();
    if scopes.iter().any(|scope| scope == "apps:read") {
        tools.push(json!({"name":"list_applications","description":"فهرست خلاصهٔ اپلیکیشن‌های فعال روی همین VPS؛ بدون ENV، رمز یا فایل محرمانه.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}));
    }
    if scopes.iter().any(|scope| scope == "deployments:read") {
        tools.push(json!({"name":"list_recent_deployments","description":"آخرین Jobهای استقرار این VPS، فقط با وضعیت و زمان‌ها؛ بدون سورس، فرمان یا log.","inputSchema":{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":50}},"additionalProperties":false}}));
    }
    if scopes
        .iter()
        .any(|scope| scope == "apps:read" || scope == "deployments:read")
    {
        tools.push(json!({"name":"get_panel_status","description":"خلاصهٔ غیرمحرمانهٔ نسخه و شمار اپلیکیشن‌ها و Jobهای فعال همین VPS.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}));
    }
    if scopes.iter().any(|scope| scope == "deployments:propose") {
        tools.push(json!({"name":"propose_application_deployment","description":"درخواست استقرار با منبع و فرمان پیش‌فرضی که مدیر قبلاً در ARG ذخیره کرده است. این فراخوانی هیچ استقراری اجرا نمی‌کند؛ مدیر باید آن را در پنل بررسی و تأیید کند.","inputSchema":{"type":"object","properties":{"application_id":{"type":"string","minLength":1,"maxLength":64},"reason":{"type":"string","maxLength":500}},"required":["application_id"],"additionalProperties":false}}));
    }
    if scopes.iter().any(|scope| scope == "terminal:root:propose") {
        tools.push(json!({"name":"propose_root_command","description":"پیشنهاد یک فرمان یک‌خطی برای اجرای موقت در ترمینال root. تا وقتی مدیر در ARG فرمان دقیق را بررسی، رمز خود را دوباره وارد و صریحاً تأیید نکند، هیچ فرمانی اجرا نمی‌شود.","inputSchema":{"type":"object","properties":{"command":{"type":"string","minLength":1,"maxLength":1200},"reason":{"type":"string","maxLength":500}},"required":["command"],"additionalProperties":false}}));
        tools.push(json!({"name":"get_root_command_result","description":"وضعیت درخواست فرمان root را بخوان؛ خروجی فقط یک‌بار و فقط برای همان توکن ارائه می‌شود و روی دیسک ذخیره نمی‌شود.","inputSchema":{"type":"object","properties":{"proposal_id":{"type":"string","minLength":1,"maxLength":64}},"required":["proposal_id"],"additionalProperties":false}}));
    }
    tools
}

fn available_resources(scopes: &[String]) -> Vec<Value> {
    let mut resources = Vec::new();
    if scopes.iter().any(|scope| scope == "repository:read") {
        resources.push(json!({"uri":"arg://repository","name":"ARG public source repository","description":format!("مخزن عمومی ARG؛ source همراه نسخهٔ پنل {} نیز در اینجا موجود است.",env!("CARGO_PKG_VERSION")),"mimeType":"text/markdown"}));
        for path in AgentRustSource::iter().filter(|path| public_source_path(path)) {
            resources.push(json!({"uri":format!("arg://source/rust/{path}"),"name":format!("src/{path}"),"mimeType":"text/x-rust"}));
        }
        for path in AgentFrontendSource::iter().filter(|path| public_source_path(path)) {
            resources.push(json!({"uri":format!("arg://source/frontend/{path}"),"name":format!("frontend/src/{path}"),"mimeType":"text/plain"}));
        }
    }
    if scopes.iter().any(|scope| scope == "docs:read") {
        for path in AgentDocs::iter().filter(|path| {
            (path.starts_with("fa/") || path.starts_with("en/")) && path.ends_with(".md")
        }) {
            resources.push(
                json!({"uri":format!("arg://docs/{path}"),"name":path,"mimeType":"text/markdown"}),
            );
        }
    }
    resources
}

fn public_source_path(path: &str) -> bool {
    !path.starts_with('.')
        && !path.contains("..")
        && !path.contains('\\')
        && ["rs", "ts", "tsx", "css"]
            .iter()
            .any(|extension| path.ends_with(&format!(".{extension}")))
}

fn read_resource(scopes: &[String], uri: &str) -> Result<Value, String> {
    if uri == "arg://repository" {
        if !scopes.iter().any(|scope| scope == "repository:read") {
            return Err("توکن مجوز خواندن مخزن را ندارد.".into());
        }
        let contents = format!("# ARG public repository\n\nOfficial source: https://github.com/themrcortex/arg\n\nThe ARG Rust and frontend source files bundled with panel version {} are available through the `arg://source/` resources. This endpoint does not fetch arbitrary URLs.\n", env!("CARGO_PKG_VERSION"));
        return Ok(json!({"contents":[{"uri":uri,"mimeType":"text/markdown","text":contents}]}));
    }
    if let Some(path) = uri.strip_prefix("arg://source/rust/") {
        if !scopes.iter().any(|scope| scope == "repository:read") {
            return Err("توکن مجوز خواندن مخزن را ندارد.".into());
        }
        if !public_source_path(path) {
            return Err("مسیر فایل منبع مجاز نیست.".into());
        }
        let source = AgentRustSource::get(path)
            .ok_or_else(|| "فایل منبع در snapshot این نسخه پیدا نشد.".to_owned())?;
        let text = std::str::from_utf8(source.data.as_ref())
            .map_err(|_| "فایل منبع UTF-8 معتبر نیست.".to_owned())?;
        return Ok(json!({"contents":[{"uri":uri,"mimeType":"text/x-rust","text":text}]}));
    }
    if let Some(path) = uri.strip_prefix("arg://source/frontend/") {
        if !scopes.iter().any(|scope| scope == "repository:read") {
            return Err("توکن مجوز خواندن مخزن را ندارد.".into());
        }
        if !public_source_path(path) {
            return Err("مسیر فایل منبع مجاز نیست.".into());
        }
        let source = AgentFrontendSource::get(path)
            .ok_or_else(|| "فایل منبع در snapshot این نسخه پیدا نشد.".to_owned())?;
        let text = std::str::from_utf8(source.data.as_ref())
            .map_err(|_| "فایل منبع UTF-8 معتبر نیست.".to_owned())?;
        return Ok(json!({"contents":[{"uri":uri,"mimeType":"text/plain","text":text}]}));
    }
    let Some(path) = uri.strip_prefix("arg://docs/") else {
        return Err("منبع مستندات مجاز نیست.".into());
    };
    if !scopes.iter().any(|scope| scope == "docs:read") {
        return Err("توکن مجوز خواندن مستندات را ندارد.".into());
    }
    if !(path.starts_with("fa/") || path.starts_with("en/"))
        || !path.ends_with(".md")
        || path.contains("..")
        || path.contains('\\')
    {
        return Err("مسیر مستندات مجاز نیست.".into());
    }
    let document = AgentDocs::get(path).ok_or_else(|| "مستند در بستهٔ ARG پیدا نشد.".to_owned())?;
    let text = std::str::from_utf8(document.data.as_ref())
        .map_err(|_| "مستند UTF-8 معتبر نیست.".to_owned())?;
    Ok(json!({"contents":[{"uri":uri,"mimeType":"text/markdown","text":text}]}))
}

fn call_tool(db: &mut Connection, context: &AgentContext, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "نام ابزار لازم است.".to_owned())?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match name {
        "list_applications" => {
            ensure_scope(context, "apps:read")?;
            let mut statement = db.prepare("SELECT id,name,framework,runtime_version,domain,lifecycle_state,observed_state,updated_at FROM applications WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT 200").map_err(|_| "فهرست اپلیکیشن‌ها خوانده نشد.".to_owned())?;
            let rows = statement.query_map([], |row| Ok(json!({"id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"framework":row.get::<_,String>(2)?,"runtime_version":row.get::<_,String>(3)?,"domain":row.get::<_,Option<String>>(4)?,"lifecycle_state":row.get::<_,String>(5)?,"observed_state":row.get::<_,String>(6)?,"updated_at":row.get::<_,i64>(7)?}))).map_err(|_| "فهرست اپلیکیشن‌ها خوانده نشد.".to_owned())?;
            let data = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "فهرست اپلیکیشن‌ها خوانده نشد.".to_owned())?;
            Ok(text_tool_result(
                json!({"applications":data,"count":data.len()}),
            ))
        }
        "list_recent_deployments" => {
            ensure_scope(context, "deployments:read")?;
            let limit = arguments
                .get("limit")
                .and_then(Value::as_i64)
                .unwrap_or(20)
                .clamp(1, 50);
            let mut statement = db.prepare("SELECT j.id,j.application_id,a.name,j.state,j.created_at,j.started_at,j.finished_at FROM deployment_jobs j JOIN applications a ON a.id=j.application_id ORDER BY j.created_at DESC LIMIT ?1").map_err(|_| "فهرست استقرارها خوانده نشد.".to_owned())?;
            let rows = statement.query_map([limit], |row| Ok(json!({"id":row.get::<_,String>(0)?,"application_id":row.get::<_,String>(1)?,"application_name":row.get::<_,String>(2)?,"state":row.get::<_,String>(3)?,"created_at":row.get::<_,i64>(4)?,"started_at":row.get::<_,Option<i64>>(5)?,"finished_at":row.get::<_,Option<i64>>(6)?}))).map_err(|_| "فهرست استقرارها خوانده نشد.".to_owned())?;
            let data = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "فهرست استقرارها خوانده نشد.".to_owned())?;
            Ok(text_tool_result(
                json!({"deployments":data,"count":data.len()}),
            ))
        }
        "get_panel_status" => {
            if !context
                .scopes
                .iter()
                .any(|scope| scope == "apps:read" || scope == "deployments:read")
            {
                return Err("توکن مجوز خواندن وضعیت را ندارد.".into());
            }
            let apps: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM applications WHERE deleted_at IS NULL",
                    [],
                    |row| row.get(0),
                )
                .map_err(|_| "وضعیت اپلیکیشن‌ها خوانده نشد.".to_owned())?;
            let active_jobs: i64 = db.query_row("SELECT COUNT(*) FROM deployment_jobs WHERE state IN ('queued','validating','running','verifying')", [], |row| row.get(0)).map_err(|_| "وضعیت استقرارها خوانده نشد.".to_owned())?;
            Ok(text_tool_result(
                json!({"version":env!("CARGO_PKG_VERSION"),"applications":apps,"active_deployments":active_jobs,"workspace":"local-vps"}),
            ))
        }
        "propose_application_deployment" => {
            ensure_scope(context, "deployments:propose")?;
            let app_id = arguments
                .get("application_id")
                .and_then(Value::as_str)
                .filter(|id| {
                    id.len() <= 64
                        && id
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || b"-".contains(&byte))
                })
                .ok_or_else(|| "شناسهٔ اپلیکیشن معتبر نیست.".to_owned())?;
            let reason = arguments
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if reason.len() > 500 || reason.chars().any(char::is_control) {
                return Err("توضیح درخواست حداکثر ۵۰۰ نویسه و بدون نویسهٔ کنترلی باشد.".into());
            }
            let owner =
                federation::local_server_uuid(db).map_err(|_| "هویت VPS خوانده نشد.".to_owned())?;
            let application: Option<String> = db.query_row("SELECT name FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL", params![app_id,owner], |row| row.get(0)).optional().map_err(|_| "اپلیکیشن بررسی نشد.".to_owned())?;
            let Some(app_name) = application else {
                return Err("اپلیکیشن فعال در همین VPS پیدا نشد.".into());
            };
            let snapshot = deploy::agent_default_deployment_snapshot(db, app_id, &owner)?;
            let pending: i64 = db.query_row("SELECT COUNT(*) FROM ai_agent_proposals WHERE token_id=?1 AND state IN ('pending','approving') AND expires_at>?2", params![context.id,now()], |row| row.get(0)).map_err(|_| "درخواست‌های معلق شمرده نشدند.".to_owned())?;
            if pending >= MAX_PENDING_PROPOSALS {
                return Err("این اتصال به سقف ۲۰ درخواست معلق رسیده است.".into());
            }
            let proposal_id = apps::random_uuid();
            let created_at = now();
            let action_json = serde_json::to_string(&snapshot)
                .map_err(|_| "Snapshot تنظیمات استقرار ذخیره نشد.".to_owned())?;
            db.execute("INSERT INTO ai_agent_proposals(id,token_id,user_id,action_type,application_id,action_json,reason,state,created_at,expires_at) VALUES(?1,?2,?3,'deploy_application',?4,?5,?6,'pending',?7,?8)", params![proposal_id,context.id,context.user_id,app_id,action_json,reason,created_at,created_at+1800]).map_err(|_| "درخواست استقرار ذخیره نشد.".to_owned())?;
            audit(
                db,
                &format!("ai-agent:{}", context.label),
                "ai_agent.deployment.propose",
                &format!(
                    "proposal_id={proposal_id}; app_id={app_id}; requester={}",
                    context.username
                ),
            );
            Ok(text_tool_result(
                json!({"proposal_id":proposal_id,"application_id":app_id,"application_name":app_name,"action":"deploy_application","deployment":snapshot.get("source_summary"),"pre_deploy_command_count":snapshot.get("pre_deploy_command_count"),"state":"pending_approval","expires_at":created_at+1800,"message":"استقرار اجرا نشده است. مدیر باید درخواست را در ARG بررسی و تأیید کند."}),
            ))
        }
        "propose_root_command" => {
            ensure_scope(context, "terminal:root:propose")?;
            let command = arguments
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| "فرمان لازم است.".to_owned())?;
            if !valid_root_command(command) {
                return Err("فرمان باید یک خط، حداکثر ۱۲۰۰ بایت و بدون نویسهٔ کنترلی باشد.".into());
            }
            let reason = arguments
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if reason.len() > 500 || reason.chars().any(char::is_control) {
                return Err("توضیح درخواست حداکثر ۵۰۰ نویسه و بدون نویسهٔ کنترلی باشد.".into());
            }
            let pending: i64 = db.query_row("SELECT COUNT(*) FROM ai_agent_proposals WHERE token_id=?1 AND state IN ('pending','approving') AND expires_at>?2", params![context.id,now()], |row| row.get(0)).map_err(|_| "درخواست‌های معلق شمرده نشدند.".to_owned())?;
            if pending >= MAX_PENDING_PROPOSALS {
                return Err("این اتصال به سقف ۲۰ درخواست معلق رسیده است.".into());
            }
            let id = apps::random_uuid();
            let created = now();
            db.execute("INSERT INTO ai_agent_proposals(id,token_id,user_id,action_type,application_id,action_json,reason,state,created_at,expires_at) VALUES(?1,?2,?3,'root_command','','{}',?4,'pending',?5,?6)", params![id,context.id,context.user_id,reason,created,created+300]).map_err(|_| "درخواست فرمان ذخیره نشد.".to_owned())?;
            ROOT_COMMANDS
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(id.clone(), (context.id, command.to_owned(), created + 300));
            audit(
                db,
                &format!("ai-agent:{}", context.label),
                "ai_agent.root_command.propose",
                &format!("proposal_id={id}; requester={}", context.username),
            );
            Ok(text_tool_result(
                json!({"proposal_id":id,"action":"root_command","state":"pending_approval","expires_at":created+300,"message":"فرمان هنوز اجرا نشده است. مدیر باید متن دقیق فرمان را در ARG بررسی کند، رمز را دوباره وارد کند و تأیید صریح بدهد."}),
            ))
        }
        "get_root_command_result" => {
            ensure_scope(context, "terminal:root:propose")?;
            let proposal_id = arguments
                .get("proposal_id")
                .and_then(Value::as_str)
                .filter(|id| apps::valid_uuid(id))
                .ok_or_else(|| "شناسهٔ درخواست معتبر نیست.".to_owned())?;
            let state: Option<String> = db.query_row("SELECT state FROM ai_agent_proposals WHERE id=?1 AND token_id=?2 AND action_type='root_command'", params![proposal_id,context.id], |row| row.get(0)).optional().map_err(|_| "وضعیت درخواست خوانده نشد.".to_owned())?;
            let Some(state) = state else {
                return Err("درخواست برای این توکن پیدا نشد.".into());
            };
            if state != "approved" {
                return Ok(text_tool_result(
                    json!({"proposal_id":proposal_id,"state":state,"output":""}),
                ));
            }
            let outputs = ROOT_COMMAND_OUTPUTS.get_or_init(|| Mutex::new(HashMap::new()));
            let result = outputs
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(proposal_id);
            match result {
                Some((token_id, output, exit_code)) if token_id == context.id => {
                    Ok(text_tool_result(
                        json!({"proposal_id":proposal_id,"state":"completed","exit_code":exit_code,"output":output,"notice":"این خروجی فقط یک‌بار قابل دریافت بود و از حافظهٔ موقت حذف شد."}),
                    ))
                }
                _ => Ok(text_tool_result(
                    json!({"proposal_id":proposal_id,"state":"completed","output":"خروجی موقت در دسترس نیست؛ احتمالاً قبلاً خوانده شده یا پنل راه‌اندازی مجدد شده است."}),
                )),
            }
        }
        _ => Err("ابزار در این اتصال موجود نیست.".into()),
    }
}

fn valid_root_command(command: &str) -> bool {
    !command.trim().is_empty()
        && command.len() <= MAX_ROOT_COMMAND_BYTES
        && !command.chars().any(|character| character.is_control())
}

fn verify_admin_reauthentication(
    db: &Connection,
    admin_id: i64,
    password: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    let throttle_key = format!("root-terminal:{admin_id}");
    let attempts: Option<(i64, i64)> = db
        .query_row(
            "SELECT count,window_start FROM auth_failures WHERE username=?1",
            [&throttle_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(db_error)?;
    if attempts
        .is_some_and(|(count, window_start)| count >= 8 && now().saturating_sub(window_start) < 600)
    {
        return Err(error(
            StatusCode::TOO_MANY_REQUESTS,
            "تعداد تلاش رمز مدیر زیاد است؛ کمی بعد دوباره امتحان کن.",
        ));
    }
    let password_hash: String = db
        .query_row(
            "SELECT password_hash FROM users WHERE id=?1 AND enabled=1 AND role='admin'",
            [admin_id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    let parsed = PasswordHash::new(&password_hash).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "اعتبارسنجی رمز مدیر ممکن نیست.",
        )
    })?;
    if Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_err()
    {
        let at = now();
        db.execute(
            "INSERT INTO auth_failures(username,count,window_start) VALUES(?1,1,?2) ON CONFLICT(username) DO UPDATE SET count=CASE WHEN ?2-window_start>=600 THEN 1 ELSE count+1 END,window_start=CASE WHEN ?2-window_start>=600 THEN ?2 ELSE window_start END",
            params![throttle_key, at],
        )
        .map_err(db_error)?;
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "رمز مدیر نادرست است؛ فرمان root اجرا نشد.",
        ));
    }
    db.execute(
        "DELETE FROM auth_failures WHERE username=?1",
        [&throttle_key],
    )
    .map_err(db_error)?;
    Ok(())
}

fn execute_root_command(command: &str) -> Result<(String, i64), String> {
    if !valid_root_command(command) {
        return Err("فرمان root معتبر نیست.".into());
    }
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 28,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|_| "ساخت محیط موقت ترمینال root ناموفق بود.".to_owned())?;
    let master = pair.master;
    let slave = pair.slave;
    let tty = master
        .tty_name()
        .ok_or_else(|| "نشانی ترمینال موقت در دسترس نیست.".to_owned())?
        .to_string_lossy()
        .into_owned();
    let response = privileged_request(
        json!({"action":"terminal-root-start","tty":tty,"max_lifetime_seconds":30}),
    )
    .map_err(|_| "سرویس عملیات ممتاز در دسترس نیست.".to_owned())?;
    if response.get("ok") != Some(&Value::Bool(true)) {
        return Err(response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("ساخت ترمینال root رد شد.")
            .to_owned());
    }
    let pid = response
        .get("pid")
        .and_then(Value::as_u64)
        .filter(|pid| *pid > 1 && *pid <= i32::MAX as u64)
        .ok_or_else(|| "شناسهٔ فرایند root معتبر نیست.".to_owned())? as u32;
    let result = (|| {
        let mut writer = master
            .take_writer()
            .map_err(|_| "نوشتن در ترمینال root ممکن نیست.".to_owned())?;
        let reader = master
            .try_clone_reader()
            .map_err(|_| "خواندن ترمینال root ممکن نیست.".to_owned())?;
        let marker = format!("ARG_ROOT_DONE_{}", apps::random_uuid().replace('-', ""));
        let script =
            format!("{command}\n__arg_exit=$?; printf '\\n{marker}:%s\\n' \"$__arg_exit\"\n");
        writer
            .write_all(script.as_bytes())
            .map_err(|_| "فرمان به ترمینال root ارسال نشد.".to_owned())?;
        drop(writer);
        drop(slave);
        let (sender, receiver) = std::sync::mpsc::sync_channel::<Vec<u8>>(8);
        std::thread::spawn(move || {
            let mut reader = reader;
            let mut buffer = [0_u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(size) if sender.send(buffer[..size].to_vec()).is_err() => break,
                    Ok(_) => {}
                }
            }
        });
        let deadline = Instant::now() + ROOT_COMMAND_TIMEOUT;
        let mut captured = Vec::new();
        let marker_bytes = marker.as_bytes();
        let status = loop {
            if Instant::now() >= deadline {
                return Err("اجرای فرمان root از مهلت ۲۰ ثانیه عبور کرد و متوقف شد.".to_owned());
            }
            match receiver.recv_timeout(std::time::Duration::from_millis(150)) {
                Ok(chunk) => {
                    captured.extend_from_slice(&chunk);
                    if captured.len() > MAX_ROOT_COMMAND_OUTPUT_BYTES + 512 {
                        let remove = captured.len() - MAX_ROOT_COMMAND_OUTPUT_BYTES - 512;
                        captured.drain(..remove);
                    }
                    if let Some(start) = captured
                        .windows(marker_bytes.len())
                        .position(|window| window == marker_bytes)
                    {
                        let tail_start = start + marker_bytes.len();
                        let tail = &captured[tail_start..];
                        if let Some(colon) = tail.iter().position(|byte| *byte == b':') {
                            let status_bytes = &tail[colon + 1..];
                            if let Some(end) = status_bytes
                                .iter()
                                .position(|byte| *byte == b'\n' || *byte == b'\r')
                            {
                                break std::str::from_utf8(&status_bytes[..end])
                                    .ok()
                                    .and_then(|value| value.trim().parse::<i64>().ok())
                                    .unwrap_or(255);
                            }
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("ترمینال root پیش از پایان فرمان قطع شد.".to_owned())
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
        };
        let output_end = captured
            .windows(marker_bytes.len())
            .position(|window| window == marker_bytes)
            .unwrap_or(captured.len());
        let output_start = captured
            .len()
            .saturating_sub(MAX_ROOT_COMMAND_OUTPUT_BYTES)
            .min(output_end);
        let output = String::from_utf8_lossy(&captured[output_start..output_end])
            .chars()
            .filter(|character| {
                !character.is_control()
                    || *character == '\n'
                    || *character == '\t'
                    || *character == '\r'
            })
            .collect::<String>();
        Ok((output, status))
    })();
    let _ = privileged_request(json!({"action":"terminal-root-stop","pid":pid}));
    result
}

fn ensure_scope(context: &AgentContext, required: &str) -> Result<(), String> {
    context
        .scopes
        .iter()
        .any(|scope| scope == required)
        .then_some(())
        .ok_or_else(|| "توکن مجوز لازم برای این ابزار را ندارد.".into())
}

fn text_tool_result(value: Value) -> Value {
    json!({"content":[{"type":"text","text":serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into())}],"isError":false})
}

pub(super) async fn list_proposals(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> ApiResult {
    if !secure_transport(&headers, peer.ip()) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "مشاهدهٔ درخواست‌های عامل فقط روی HTTPS مجاز است.",
        ));
    }
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let stale_cutoff = now() - 60;
    let mut stale_query = db
        .prepare("SELECT id,action_type FROM ai_agent_proposals WHERE state='approving' AND reviewed_at<=?1")
        .map_err(db_error)?;
    let stale_ids = stale_query
        .query_map([stale_cutoff], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    drop(stale_query);
    for (proposal_id, action_type) in stale_ids {
        if action_type == "root_command" {
            db.execute("UPDATE ai_agent_proposals SET state='failed',action_json='{}',result_error='اجرای فرمان root کامل نشد یا نتیجه پس از راه‌اندازی مجدد قابل تأیید نیست؛ برای جلوگیری از اجرای تکراری، پیشنهاد خودکار تکرار نشد.' WHERE id=?1 AND state='approving'", [&proposal_id]).map_err(db_error)?;
            audit(
                &db,
                "system",
                "ai_agent.root_command.recovery_required",
                &format!("proposal_id={proposal_id}; result=not_retried"),
            );
            continue;
        }
        if let Some(job) = deploy::agent_approved_job(&db, &proposal_id).map_err(db_error)? {
            db.execute("UPDATE ai_agent_proposals SET state='approved',result_json=?1 WHERE id=?2 AND state='approving'", params![serde_json::to_string(&job).unwrap_or_else(|_| "{}".into()),proposal_id]).map_err(db_error)?;
            audit(
                &db,
                "system",
                "ai_agent.approval.recovered",
                &format!("proposal_id={proposal_id}; result=job_found"),
            );
        } else {
            db.execute("UPDATE ai_agent_proposals SET state='pending',reviewed_at=NULL,reviewed_by=NULL WHERE id=?1 AND state='approving'", [&proposal_id]).map_err(db_error)?;
            audit(
                &db,
                "system",
                "ai_agent.approval.recovered",
                &format!("proposal_id={proposal_id}; result=approval_reopened"),
            );
        }
    }
    db.execute(
        "UPDATE ai_agent_proposals SET state='expired',action_json=CASE WHEN action_type='root_command' THEN '{}' ELSE action_json END WHERE state='pending' AND expires_at<=?1",
        [now()],
    )
    .map_err(db_error)?;
    if let Some(commands) = ROOT_COMMANDS.get() {
        commands
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, (_, _, expires)| *expires > now());
    }
    let pending_root = {
        let mut statement = db.prepare("SELECT id,token_id FROM ai_agent_proposals WHERE action_type='root_command' AND state='pending'").map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(db_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?
    };
    for (proposal_id, token_id) in pending_root {
        let available = ROOT_COMMANDS.get().is_some_and(|commands| {
            commands
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get(&proposal_id)
                .is_some_and(|(stored_token, _, expires)| {
                    *stored_token == token_id && *expires > now()
                })
        });
        if !available {
            db.execute("UPDATE ai_agent_proposals SET state='failed',action_json='{}',result_error='درخواست root پس از راه‌اندازی مجدد پنل از بین رفت؛ فرمان ذخیره نشده و دوباره اجرا نمی‌شود.' WHERE id=?1 AND state='pending'", [&proposal_id]).map_err(db_error)?;
            audit(
                &db,
                "system",
                "ai_agent.root_command.lost",
                &format!("proposal_id={proposal_id}; result=not_retried"),
            );
        }
    }
    let mut statement = db.prepare("SELECT p.id,p.action_type,p.application_id,COALESCE(a.name,'(حذف‌شده)'),p.reason,p.state,p.created_at,p.expires_at,p.reviewed_at,p.reviewed_by,p.result_json,p.result_error,t.label,u.username,p.action_json,p.token_id FROM ai_agent_proposals p LEFT JOIN applications a ON a.id=p.application_id LEFT JOIN ai_agent_tokens t ON t.id=p.token_id JOIN users u ON u.id=p.user_id ORDER BY p.created_at DESC LIMIT 100").map_err(db_error)?;
    let rows = statement.query_map([], |row| {
        let action_json: String = row.get(14)?;
        let action_type: String = row.get(1)?;
        let state: String = row.get(5)?;
        let action = serde_json::from_str::<Value>(&action_json).unwrap_or_else(|_| json!({}));
        let deployment = serde_json::from_value::<Value>(action.clone()).map(|snapshot| json!({
            "source":snapshot.get("source_summary").cloned().unwrap_or(Value::Null),
            "pre_deploy_command_count":snapshot.get("pre_deploy_command_count").cloned().unwrap_or(json!(0)),
            "healthcheck_file":snapshot.get("healthcheck_file").cloned().unwrap_or(Value::Null),
            "retention_count":snapshot.get("retention_count").cloned().unwrap_or(json!(5))
        })).unwrap_or_else(|_| json!({"error":"Snapshot معتبر نیست"}));
        let proposal_id: String = row.get(0)?;
        let token_id: i64 = row.get(15)?;
        let root_command = if action_type == "root_command" && (state == "pending" || state == "approving") {
            ROOT_COMMANDS.get().and_then(|commands| commands.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&proposal_id).filter(|(stored_token, _, expires)| *stored_token == token_id && *expires > now()).map(|(_, command, _)| command.clone()))
        } else { None };
        let application_name = if action_type == "root_command" { "ترمینال روت".to_owned() } else { row.get::<_,String>(3)? };
        Ok(json!({"id":row.get::<_,String>(0)?,"action_type":action_type,"application_id":row.get::<_,String>(2)?,"application_name":application_name,"reason":row.get::<_,String>(4)?,"state":state,"created_at":row.get::<_,i64>(6)?,"expires_at":row.get::<_,i64>(7)?,"reviewed_at":row.get::<_,Option<i64>>(8)?,"reviewed_by":row.get::<_,Option<String>>(9)?,"result":row.get::<_,Option<String>>(10)?,"error":row.get::<_,Option<String>>(11)?,"agent_label":row.get::<_,Option<String>>(12)?,"requested_by":row.get::<_,String>(13)?,"deployment":deployment,"root_command":root_command}))
    }).map_err(db_error)?;
    let proposals: Vec<Value> = rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
    Ok(Json(json!({"proposals":proposals})))
}

pub(super) async fn approve_proposal(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ApproveProposalInput>,
) -> ApiResult {
    if !secure_transport(&headers, peer.ip()) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "تأیید درخواست عامل فقط روی HTTPS مجاز است.",
        ));
    }
    check_origin(&headers)?;
    let admin = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    if !apps::valid_uuid(&id) {
        return Err(error(StatusCode::BAD_REQUEST, "شناسهٔ درخواست معتبر نیست."));
    }
    let action_type = {
        let db = state.lock().unwrap();
        db.query_row(
            "SELECT action_type FROM ai_agent_proposals WHERE id=?1 AND state='pending'",
            [&id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(db_error)?
    };
    if action_type.as_deref() == Some("root_command") {
        return approve_root_command(state, admin, id, input).await;
    }
    let (app_id, snapshot, token_id) = {
        let db = state.lock().unwrap();
        let proposal: Option<(String, String, i64, String, i64)> = db.query_row("SELECT p.action_type,p.application_id,p.expires_at,p.action_json,p.token_id FROM ai_agent_proposals p JOIN ai_agent_tokens t ON t.id=p.token_id JOIN users u ON u.id=p.user_id WHERE p.id=?1 AND p.state='pending' AND t.revoked_at IS NULL AND t.expires_at>?2 AND u.role='admin' AND u.enabled=1", params![id,now()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional().map_err(db_error)?;
        let Some((action, app_id, expires_at, action_json, token_id)) = proposal else {
            return Err(error(
                StatusCode::CONFLICT,
                "درخواست پیدا نشد یا قبلاً بررسی شده است.",
            ));
        };
        if expires_at <= now() {
            db.execute(
                "UPDATE ai_agent_proposals SET state='expired',action_json=CASE WHEN action_type='root_command' THEN '{}' ELSE action_json END WHERE id=?1 AND state='pending'",
                [&id],
            )
            .map_err(db_error)?;
            return Err(error(StatusCode::GONE, "مهلت درخواست پایان یافته است."));
        }
        if action != "deploy_application" {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "نوع عملیات در این نسخه قابل اجرا نیست.",
            ));
        }
        let snapshot = serde_json::from_str::<Value>(&action_json).map_err(|_| error(StatusCode::CONFLICT, "Snapshot درخواست عامل خراب است؛ برای جلوگیری از اجرای تنظیمات نامشخص، تأیید متوقف شد."))?;
        let changed = db.execute("UPDATE ai_agent_proposals SET state='approving',reviewed_at=?1,reviewed_by=?2 WHERE id=?3 AND state='pending'", params![now(),admin.username,id]).map_err(db_error)?;
        if changed != 1 {
            return Err(error(
                StatusCode::CONFLICT,
                "درخواست هم‌زمان توسط کاربر دیگری بررسی شد.",
            ));
        }
        (app_id, snapshot, token_id)
    };
    let _ = token_id;
    let result = deploy::queue_agent_approved_snapshot(
        &state,
        &headers,
        &app_id,
        &admin.username,
        &id,
        &snapshot,
    )
    .await;
    match result {
        Ok(job) => {
            let db = state.lock().unwrap();
            db.execute("UPDATE ai_agent_proposals SET state='approved',result_json=?1 WHERE id=?2 AND state='approving'", params![serde_json::to_string(&job).unwrap_or_else(|_| "{}".into()),id]).map_err(db_error)?;
            audit(
                &db,
                &admin.username,
                "ai_agent.deployment.approve",
                &format!(
                    "proposal_id={id}; app_id={app_id}; job_id={}",
                    job.get("id").and_then(Value::as_str).unwrap_or("unknown")
                ),
            );
            Ok(Json(json!({"ok":true,"proposal_id":id,"job":job})))
        }
        Err((status, body)) => {
            let message = body
                .0
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("خطای استقرار");
            let db = state.lock().unwrap();
            db.execute("UPDATE ai_agent_proposals SET state='failed',result_error=?1 WHERE id=?2 AND state='approving'", params![message,id]).map_err(db_error)?;
            audit(
                &db,
                &admin.username,
                "ai_agent.deployment.approve_failed",
                &format!("proposal_id={id}; app_id={app_id}; status={status}"),
            );
            Err((status, body))
        }
    }
}

async fn approve_root_command(
    state: AppState,
    admin: User,
    id: String,
    input: ApproveProposalInput,
) -> ApiResult {
    let (token_id, command) = {
        let db = state.lock().unwrap();
        let proposal: Option<(i64, i64)> = db.query_row("SELECT p.token_id,p.expires_at FROM ai_agent_proposals p JOIN ai_agent_tokens t ON t.id=p.token_id JOIN users u ON u.id=p.user_id WHERE p.id=?1 AND p.action_type='root_command' AND p.state='pending' AND t.revoked_at IS NULL AND t.expires_at>?2 AND u.role='admin' AND u.enabled=1", params![id,now()], |row| Ok((row.get(0)?,row.get(1)?))).optional().map_err(db_error)?;
        let Some((token_id, expires_at)) = proposal else {
            return Err(error(
                StatusCode::CONFLICT,
                "درخواست پیدا نشد یا قبلاً بررسی شده است.",
            ));
        };
        if expires_at <= now() {
            db.execute("UPDATE ai_agent_proposals SET state='expired',action_json='{}' WHERE id=?1 AND state='pending'", [&id]).map_err(db_error)?;
            if let Some(commands) = ROOT_COMMANDS.get() {
                commands
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .remove(&id);
            }
            return Err(error(StatusCode::GONE, "مهلت درخواست پایان یافته است."));
        }
        let command = ROOT_COMMANDS.get().and_then(|commands| {
            commands
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get(&id)
                .filter(|(stored_token, _, expires)| *stored_token == token_id && *expires > now())
                .map(|(_, command, _)| command.clone())
        });
        let Some(command) = command.filter(|value| valid_root_command(value)) else {
            db.execute("UPDATE ai_agent_proposals SET state='failed',action_json='{}',result_error='متن موقت فرمان در دسترس نیست؛ فرمان اجرا نشد.' WHERE id=?1 AND state='pending'", [&id]).map_err(db_error)?;
            return Err(error(
                StatusCode::CONFLICT,
                "متن موقت فرمان در دسترس نیست؛ برای امنیت هیچ فرمانی اجرا نشد.",
            ));
        };
        if input.confirmation.as_deref() != Some("RUN AS ROOT") {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "برای تأیید فرمان، عبارت RUN AS ROOT را وارد کن.",
            ));
        }
        let password = input
            .password
            .as_deref()
            .filter(|value| !value.is_empty() && value.len() <= 1024)
            .ok_or_else(|| {
                error(
                    StatusCode::UNAUTHORIZED,
                    "برای اجرای فرمان root رمز مدیر لازم است.",
                )
            })?;
        verify_admin_reauthentication(&db, admin.id, password)?;
        let changed = db.execute("UPDATE ai_agent_proposals SET state='approving',reviewed_at=?1,reviewed_by=?2 WHERE id=?3 AND state='pending'", params![now(),admin.username,id]).map_err(db_error)?;
        if changed != 1 {
            return Err(error(
                StatusCode::CONFLICT,
                "درخواست هم‌زمان توسط کاربر دیگری بررسی شد.",
            ));
        }
        if let Some(commands) = ROOT_COMMANDS.get() {
            commands
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&id);
        }
        (token_id, command)
    };

    let result = tokio::task::spawn_blocking(move || execute_root_command(&command))
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "اجرای فرمان root متوقف شد.",
            )
        })?;
    match result {
        Ok((output, exit_code)) => {
            ROOT_COMMAND_OUTPUTS
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(id.clone(), (token_id, output, now() + 300));
            let db = state.lock().unwrap();
            db.execute("UPDATE ai_agent_proposals SET state='approved',action_json='{}',result_json=?1 WHERE id=?2 AND state='approving'", params![json!({"exit_code":exit_code}).to_string(),id]).map_err(db_error)?;
            audit(
                &db,
                &admin.username,
                "ai_agent.root_command.approve",
                &format!("proposal_id={id}; token_id={token_id}; exit_code={exit_code}"),
            );
            Ok(Json(
                json!({"ok":true,"proposal_id":id,"state":"approved","exit_code":exit_code,"output_available_to_agent":true,"notice":"خروجی فقط یک‌بار از MCP قابل دریافت است و روی دیسک ثبت نشده است."}),
            ))
        }
        Err(message) => {
            let db = state.lock().unwrap();
            db.execute("UPDATE ai_agent_proposals SET state='failed',action_json='{}',result_error=?1 WHERE id=?2 AND state='approving'", params![message,id]).map_err(db_error)?;
            audit(
                &db,
                &admin.username,
                "ai_agent.root_command.failed",
                &format!("proposal_id={id}; token_id={token_id}"),
            );
            Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "اجرای فرمان root کامل نشد؛ جزئیات حساس فرمان در گزارش ثبت نشده است.",
            ))
        }
    }
}

pub(super) async fn reject_proposal(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult {
    if !secure_transport(&headers, peer.ip()) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "بررسی درخواست عامل فقط روی HTTPS مجاز است.",
        ));
    }
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let admin = require_admin(&db, &headers)?;
    let changed = db.execute("UPDATE ai_agent_proposals SET state='rejected',action_json=CASE WHEN action_type='root_command' THEN '{}' ELSE action_json END,reviewed_at=?1,reviewed_by=?2 WHERE id=?3 AND state='pending'", params![now(),admin.username,id]).map_err(db_error)?;
    if changed != 1 {
        return Err(error(
            StatusCode::CONFLICT,
            "درخواست پیدا نشد یا قبلاً بررسی شده است.",
        ));
    }
    if let Some(commands) = ROOT_COMMANDS.get() {
        commands
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&id);
    }
    audit(
        &db,
        &admin.username,
        "ai_agent.proposal.reject",
        &format!("proposal_id={id}"),
    );
    Ok(Json(json!({"ok":true,"proposal_id":id,"state":"rejected"})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{header, HeaderValue};

    fn setup() -> (Connection, String, HeaderMap) {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE users(id INTEGER PRIMARY KEY,username TEXT,role TEXT,enabled INTEGER); INSERT INTO users VALUES(1,'admin','admin',1);").unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        let raw = format!("arg_agent_{}", token());
        db.execute("INSERT INTO ai_agent_tokens(user_id,label,token_hash,token_prefix,scopes_json,created_at,expires_at) VALUES(1,'test',?1,'arg_agent_test','[\"apps:read\"]',?2,?3)", params![session_hash(&raw),now(),now()+3600]).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {raw}")).unwrap(),
        );
        (db, raw, headers)
    }

    #[test]
    fn agent_token_is_scoped_expiring_rate_limited_and_revocable() {
        let (db, _raw, headers) = setup();
        assert!(authorize(&db, &headers, Some("apps:read")).is_ok());
        assert!(matches!(
            authorize(&db, &headers, Some("deployments:propose")),
            Err((StatusCode::FORBIDDEN, _))
        ));
        for _ in 1..AGENT_REQUESTS_PER_MINUTE {
            assert!(authorize(&db, &headers, None).is_ok());
        }
        assert!(matches!(
            authorize(&db, &headers, None),
            Err((StatusCode::TOO_MANY_REQUESTS, _))
        ));
        db.execute("UPDATE ai_agent_tokens SET revoked_at=?1", [now()])
            .unwrap();
        assert!(matches!(
            authorize(&db, &headers, None),
            Err((StatusCode::UNAUTHORIZED, _))
        ));
    }

    #[test]
    fn demoting_token_owner_revokes_agent_access() {
        let (db, _raw, headers) = setup();
        assert!(authorize(&db, &headers, None).is_ok());
        assert_eq!(
            revoke_if_owner_ineligible(&db, 1, "viewer", true).unwrap(),
            1
        );
        assert!(matches!(
            authorize(&db, &headers, None),
            Err((StatusCode::UNAUTHORIZED, _))
        ));
    }

    #[test]
    fn losing_admin_access_revokes_agent_and_invalidates_pending_proposals() {
        let (db, _raw, _headers) = setup();
        let token_id: i64 = db
            .query_row("SELECT id FROM ai_agent_tokens", [], |row| row.get(0))
            .unwrap();
        db.execute("INSERT INTO ai_agent_proposals(id,token_id,user_id,action_type,application_id,reason,state,created_at,expires_at) VALUES('proposal',?1,1,'deploy_application','app','test','pending',?2,?3)", params![token_id,now(),now()+1800]).unwrap();
        assert_eq!(
            revoke_if_owner_ineligible(&db, 1, "viewer", true).unwrap(),
            1
        );
        let state: String = db
            .query_row(
                "SELECT state FROM ai_agent_proposals WHERE id='proposal'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(state, "rejected");
    }

    #[test]
    fn deployment_proposal_snapshot_pins_safe_source_settings() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE applications(id TEXT PRIMARY KEY,owner_server_uuid TEXT,name TEXT,framework TEXT,wordpress_version TEXT,deleted_at INTEGER); CREATE TABLE application_deployment_settings(application_id TEXT,retention_count INTEGER,default_healthcheck_file TEXT,default_source_json TEXT,default_pre_deploy_commands_json TEXT); INSERT INTO applications VALUES('app-1','server','Test App','node','',NULL);").unwrap();
        db.execute(
            "INSERT INTO application_deployment_settings VALUES('app-1',7,'healthz.json',?1,'[]')",
            [r#"{"kind":"git","repository":"https://github.com/acme/site","reference":"main"}"#],
        )
        .unwrap();
        let snapshot = deploy::agent_default_deployment_snapshot(&db, "app-1", "server").unwrap();
        assert_eq!(
            snapshot["source_summary"]["repository"],
            "https://github.com/acme/site"
        );
        assert_eq!(snapshot["source_summary"]["reference"], "main");
        assert_eq!(snapshot["retention_count"], 7);
        assert_eq!(snapshot["pre_deploy_command_count"], 0);
        db.execute("UPDATE application_deployment_settings SET default_source_json=?1 WHERE application_id='app-1'", [r#"{"kind":"git","repository":"https://user:password@github.com/acme/site","reference":"main"}"#]).unwrap();
        assert!(deploy::agent_default_deployment_snapshot(&db, "app-1", "server").is_err());
    }

    #[test]
    fn resource_reader_allows_only_bundled_public_docs_and_repository_pointer() {
        let docs_scope = vec!["docs:read".to_owned()];
        let source_scope = vec!["repository:read".to_owned()];
        assert!(read_resource(&source_scope, "arg://repository").is_ok());
        assert!(read_resource(&source_scope, "arg://source/rust/main.rs").is_ok());
        assert!(read_resource(&source_scope, "arg://source/rust/../../Cargo.toml").is_err());
        assert!(read_resource(&docs_scope, "arg://repository").is_err());
        assert!(read_resource(&docs_scope, "arg://docs/fa/security.md").is_ok());
        assert!(read_resource(&docs_scope, "arg://docs/../Cargo.toml").is_err());
        assert!(read_resource(&docs_scope, "https://169.254.169.254/latest/meta-data/").is_err());
    }

    #[test]
    fn no_credential_or_shell_tools_are_published() {
        let scopes = vec![
            "apps:read".into(),
            "deployments:read".into(),
            "docs:read".into(),
            "repository:read".into(),
            "deployments:propose".into(),
        ];
        let tools = available_tools(&scopes);
        assert!(!tools.iter().any(|tool| tool["name"] == "run_command"
            || tool["name"] == "read_secret"
            || tool["name"] == "proxy_api"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "propose_application_deployment"));
    }

    #[test]
    fn root_terminal_tools_require_an_explicit_capability() {
        let tools = available_tools(&["apps:read".into()]);
        assert!(!tools
            .iter()
            .any(|tool| tool["name"] == "propose_root_command"));
        let root_tools = available_tools(&["terminal:root:propose".into()]);
        assert!(root_tools
            .iter()
            .any(|tool| tool["name"] == "propose_root_command"));
        assert!(root_tools
            .iter()
            .any(|tool| tool["name"] == "get_root_command_result"));
        assert!(!root_tools
            .iter()
            .any(|tool| tool["name"] == "root_terminal_open"));
    }

    #[test]
    fn root_command_input_is_single_line_bounded_and_control_free() {
        assert!(valid_root_command("systemctl status nginx"));
        assert!(!valid_root_command("  \n "));
        assert!(!valid_root_command("id\nwhoami"));
        assert!(!valid_root_command(&"x".repeat(MAX_ROOT_COMMAND_BYTES + 1)));
    }

    #[test]
    fn root_command_approval_requires_fresh_admin_password_and_throttles_failures() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE users(id INTEGER PRIMARY KEY,password_hash TEXT,role TEXT,enabled INTEGER); CREATE TABLE auth_failures(username TEXT PRIMARY KEY,count INTEGER NOT NULL,window_start INTEGER NOT NULL);").unwrap();
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(b"correct horse battery staple", &salt)
            .unwrap()
            .to_string();
        db.execute("INSERT INTO users VALUES(1,?1,'admin',1)", [&hash])
            .unwrap();
        assert!(matches!(
            verify_admin_reauthentication(&db, 1, "wrong"),
            Err((StatusCode::UNAUTHORIZED, _))
        ));
        let failed_attempts: i64 = db
            .query_row(
                "SELECT count FROM auth_failures WHERE username='root-terminal:1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(failed_attempts, 1);
        assert!(verify_admin_reauthentication(&db, 1, "correct horse battery staple").is_ok());
        let remaining: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM auth_failures WHERE username='root-terminal:1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0);
        db.execute_batch("DROP TABLE auth_failures;").unwrap();
        assert!(matches!(
            verify_admin_reauthentication(&db, 1, "correct horse battery staple"),
            Err((StatusCode::INTERNAL_SERVER_ERROR, _))
        ));
    }

    #[test]
    fn root_command_reauthentication_fails_closed_when_throttled() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE users(id INTEGER PRIMARY KEY,password_hash TEXT,role TEXT,enabled INTEGER); CREATE TABLE auth_failures(username TEXT PRIMARY KEY,count INTEGER NOT NULL,window_start INTEGER NOT NULL);").unwrap();
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(b"correct horse battery staple", &salt)
            .unwrap()
            .to_string();
        db.execute("INSERT INTO users VALUES(1,?1,'admin',1)", [&hash])
            .unwrap();
        db.execute(
            "INSERT INTO auth_failures VALUES('root-terminal:1',8,?1)",
            [now()],
        )
        .unwrap();
        assert!(matches!(
            verify_admin_reauthentication(&db, 1, "correct horse battery staple"),
            Err((StatusCode::TOO_MANY_REQUESTS, _))
        ));
    }

    #[test]
    fn scopes_are_allowlisted_deduplicated_and_stably_ordered() {
        assert_eq!(
            normalized_scopes(&vec![
                "terminal:root:propose".into(),
                "apps:read".into(),
                "apps:read".into()
            ]),
            vec!["apps:read", "terminal:root:propose"]
        );
    }

    #[test]
    fn mcp_responses_advertise_the_supported_protocol_version() {
        let response = jsonrpc_result(Some(json!(1)), json!({"ok":true}));
        assert_eq!(response.headers()["mcp-protocol-version"], "2025-03-26");
        let notification_response = mcp_accepted();
        assert_eq!(notification_response.status(), StatusCode::ACCEPTED);
        assert_eq!(
            notification_response.headers()["mcp-protocol-version"],
            "2025-03-26"
        );
    }
}
