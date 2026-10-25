use super::*;

const NGINX_AVAILABLE_DIR: &str = "/etc/nginx/sites-available";
const NGINX_ENABLED_DIR: &str = "/etc/nginx/sites-enabled";
static NGINX_OPERATION_LOCK: Mutex<()> = Mutex::new(());

#[derive(Deserialize)]
pub(super) struct NginxSiteContentInput {
    content: String,
    #[serde(default)]
    settings: Option<Value>,
}
#[derive(Deserialize)]
pub(super) struct NginxSiteEnabledInput {
    enabled: bool,
}
#[derive(Deserialize)]
pub(super) struct NginxActionInput {
    action: String,
}
#[derive(Deserialize)]
pub(super) struct NginxCreateSiteInput {
    name: String,
    content: String,
    #[serde(default)]
    settings: Option<Value>,
}
#[derive(Deserialize)]
pub(super) struct NginxSiteSyncInput {
    source: String,
}
#[derive(Deserialize)]
pub(super) struct NginxSslIssueInput {
    email: String,
    domains: Vec<String>,
    #[serde(default)]
    redirect: bool,
}

fn nginx_content_hash(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

fn store_nginx_snapshot(
    db: &Connection,
    name: &str,
    content: &str,
    settings: Option<&Value>,
) -> Result<(), rusqlite::Error> {
    db.execute("INSERT INTO nginx_site_config_state(name,content,settings_json,content_hash,updated_at) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(name) DO UPDATE SET content=excluded.content,settings_json=excluded.settings_json,content_hash=excluded.content_hash,updated_at=excluded.updated_at", params![name, content, settings.map(Value::to_string), nginx_content_hash(content), now()])?;
    Ok(())
}
#[derive(Deserialize)]
pub(super) struct NginxSiteRenameInput {
    new_name: String,
}
#[derive(Deserialize)]
pub(super) struct NginxSiteMetadataInput {
    description: String,
}
#[derive(Deserialize)]
pub(super) struct NginxSiteRuntimeInput {
    unit: String,
    cpu_quota_percent: Option<f64>,
    memory_max_bytes: Option<u64>,
}
#[derive(Serialize)]
struct NginxSiteAction {
    id: i64,
    site_name: String,
    title: String,
    icon: String,
    operation: String,
}
#[derive(Deserialize)]
pub(super) struct NginxSiteActionInput {
    title: String,
    icon: String,
    operation: String,
}
#[derive(Deserialize)]
pub(super) struct NginxSiteLogInput {
    kind: String,
}
#[derive(Deserialize)]
pub(super) struct NginxDomainCheckQuery {
    hosts: String,
}
#[derive(Deserialize)]
pub(super) struct NginxBrowseQuery {
    path: Option<String>,
}

fn nginx_manager_request(request: Value) -> Result<Value, (StatusCode, Json<Value>)> {
    let response = privileged_request(request)?;
    if response["ok"] == true {
        Ok(response["data"].clone())
    } else {
        Err(error(
            StatusCode::BAD_REQUEST,
            response["error"]
                .as_str()
                .unwrap_or("Nginx operation failed"),
        ))
    }
}

async fn nginx_manager_request_async(request: Value) -> Result<Value, (StatusCode, Json<Value>)> {
    let response = privileged_request_async(request).await?;
    if response["ok"] == true {
        Ok(response["data"].clone())
    } else {
        Err(error(
            StatusCode::BAD_REQUEST,
            response["error"]
                .as_str()
                .unwrap_or("Nginx operation failed"),
        ))
    }
}

pub(super) fn nginx_site_content(value: &Value) -> Result<&str, (StatusCode, Json<Value>)> {
    // Current privileged-manager responses wrap a site in `site`; older installed
    // helpers returned the site object directly. Accept both wire formats so an
    // upgrade cannot turn a readable config into an empty reconciliation preview.
    value["site"]["content"]
        .as_str()
        .or_else(|| value["content"].as_str())
        .ok_or_else(|| {
            error(
                StatusCode::BAD_GATEWAY,
                "مدیر Nginx محتوای فایل سایت را در قالب قابل‌خواندن برنگرداند.",
            )
        })
}

pub(super) async fn nginx_sites(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let mut result = nginx_manager_request_async(json!({"action":"nginx-list"})).await?;
    if let Some(sites) = result["sites"].as_array_mut() {
        for site in sites {
            if let Some(name) = site["name"].as_str().map(str::to_owned) {
                let (description, snapshot) = {
                    let db = state.lock().unwrap();
                    let description = db
                        .query_row(
                            "SELECT description FROM nginx_site_metadata WHERE name=?1",
                            [&name],
                            |row| row.get::<_, String>(0),
                        )
                        .unwrap_or_default();
                    let snapshot: Option<(String, String)> = db
                        .query_row(
                            "SELECT content_hash,content FROM nginx_site_config_state WHERE name=?1",
                            [&name],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .ok();
                    (description, snapshot)
                };
                site["description"] = json!(description);
                // The helper already hashes every config during listing. Read
                // the full file only for a new or changed site.
                let listed_hash = site["content_hash"].as_str();
                if snapshot
                    .as_ref()
                    .is_some_and(|(hash, _)| Some(hash.as_str()) == listed_hash)
                {
                    site["sync_mismatch"] = json!(false);
                    continue;
                }
                let current =
                    nginx_manager_request_async(json!({"action":"nginx-get","name":name})).await?;
                let content = nginx_site_content(&current)?;
                let file_hash = nginx_content_hash(content);
                if let Some((hash, db_content)) = snapshot {
                    let mismatch = hash != file_hash;
                    site["sync_mismatch"] = json!(mismatch);
                    if mismatch {
                        site["db_content"] = json!(db_content);
                        site["content"] = json!(content);
                    }
                } else {
                    let db = state.lock().unwrap();
                    store_nginx_snapshot(&db, &name, content, None).map_err(db_error)?;
                    site["sync_mismatch"] = json!(false);
                }
            }
        }
    }
    Ok(Json(result))
}

pub(super) async fn nginx_status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    nginx_manager_request_async(json!({"action":"nginx-status"}))
        .await
        .map(Json)
}

pub(super) async fn api_nginx_browse_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<NginxBrowseQuery>,
) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let result = nginx_manager_request(json!({"action":"nginx-browse", "path":query.path}))?;
    Ok(Json(result))
}

pub(super) async fn nginx_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<NginxActionInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    let allowed = [
        "install", "start", "stop", "restart", "reload", "enable", "disable", "test", "logs",
    ];
    if !allowed.contains(&input.action.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "عملیات Nginx معتبر نیست."));
    }
    let result = nginx_manager_request(json!({"action":format!("nginx-{}", input.action)}))?;
    let db = state.lock().unwrap();
    audit(&db, &actor, "nginx.service.action", &input.action);
    Ok(Json(result))
}

pub(super) async fn create_nginx_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<NginxCreateSiteInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&input.name) || input.content.len() > 512 * 1024 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام یا اندازهٔ فایل سایت معتبر نیست.",
        ));
    }
    let result = nginx_manager_request(
        json!({"action":"nginx-create", "name":input.name, "content":input.content}),
    )?;
    let db = state.lock().unwrap();
    db.execute(
        "INSERT OR IGNORE INTO nginx_site_metadata(name,description) VALUES (?1,'')",
        [&input.name],
    )
    .map_err(db_error)?;
    store_nginx_snapshot(&db, &input.name, &input.content, input.settings.as_ref())
        .map_err(db_error)?;
    audit(&db, &actor, "nginx.site.create", &input.name);
    Ok(Json(result))
}

pub(super) async fn delete_nginx_site(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&name) {
        return Err(error(StatusCode::BAD_REQUEST, "نام سایت معتبر نیست."));
    }
    let result = nginx_manager_request(json!({"action":"nginx-delete", "name":name}))?;
    let db = state.lock().unwrap();
    db.execute("DELETE FROM nginx_site_metadata WHERE name=?1", [&name])
        .map_err(db_error)?;
    db.execute("DELETE FROM nginx_site_config_state WHERE name=?1", [&name])
        .map_err(db_error)?;
    db.execute("DELETE FROM nginx_site_runtime WHERE site_name=?1", [&name])
        .map_err(db_error)?;
    db.execute("DELETE FROM nginx_site_actions WHERE site_name=?1", [&name])
        .map_err(db_error)?;
    audit(&db, &actor, "nginx.site.delete", &name);
    Ok(Json(result))
}

pub(super) async fn rename_nginx_site(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(input): Json<NginxSiteRenameInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&name) || !valid_nginx_site_name(&input.new_name) {
        return Err(error(StatusCode::BAD_REQUEST, "نام فایل سایت معتبر نیست."));
    }
    let mut result = nginx_manager_request(
        json!({"action":"nginx-rename", "name":name, "new_name":input.new_name}),
    )?;
    let db = state.lock().unwrap();
    let old_description = db
        .query_row(
            "SELECT description FROM nginx_site_metadata WHERE name=?1",
            [&name],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_default();
    db.execute("DELETE FROM nginx_site_metadata WHERE name=?1", [&name])
        .map_err(db_error)?;
    db.execute(
        "INSERT OR REPLACE INTO nginx_site_metadata(name,description) VALUES (?1,?2)",
        params![input.new_name, old_description],
    )
    .map_err(db_error)?;
    db.execute(
        "UPDATE nginx_site_config_state SET name=?2 WHERE name=?1",
        params![name, input.new_name],
    )
    .map_err(db_error)?;
    db.execute(
        "UPDATE nginx_site_runtime SET site_name=?2 WHERE site_name=?1",
        params![name, input.new_name],
    )
    .map_err(db_error)?;
    db.execute(
        "UPDATE nginx_site_actions SET site_name=?2 WHERE site_name=?1",
        params![name, input.new_name],
    )
    .map_err(db_error)?;
    result["description"] = json!(old_description);
    audit(
        &db,
        &actor,
        "nginx.site.rename",
        &format!("{name} -> {}", input.new_name),
    );
    Ok(Json(result))
}

pub(super) async fn update_nginx_site_metadata(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(input): Json<NginxSiteMetadataInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_nginx_site_name(&name) || input.description.chars().count() > 500 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام سایت یا طول توضیحات معتبر نیست.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    nginx_manager_request(json!({"action":"nginx-get", "name":name}))?;
    db.execute("INSERT INTO nginx_site_metadata(name,description) VALUES (?1,?2) ON CONFLICT(name) DO UPDATE SET description=excluded.description", params![name, input.description.trim()]).map_err(db_error)?;
    audit(&db, &actor, "nginx.site.metadata", &format!("site={name}"));
    Ok(Json(
        json!({"name":name,"description":input.description.trim(),"message":"توضیحات سایت ذخیره شد."}),
    ))
}

fn read_site_action(db: &Connection, site: &str, id: i64) -> Result<NginxSiteAction, rusqlite::Error> {
    db.query_row("SELECT id,site_name,title,icon,operation FROM nginx_site_actions WHERE site_name=?1 AND id=?2", params![site,id], |row| Ok(NginxSiteAction { id:row.get(0)?,site_name:row.get(1)?,title:row.get(2)?,icon:row.get(3)?,operation:row.get(4)? }))
}

fn validate_site_action(input: &NginxSiteActionInput) -> Result<(), (StatusCode, Json<Value>)> {
    if input.title.trim().is_empty() || input.title.chars().count() > 40 {
        return Err(error(StatusCode::BAD_REQUEST, "عنوان دکمه باید بین 1 تا 40 نویسه باشد."));
    }
    if !["Play", "RotateCw", "ShieldCheck", "FolderOpen", "ScrollText", "Globe", "Zap", "Terminal", "Power", "Activity"].contains(&input.icon.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "آیکون انتخاب‌شده پشتیبانی نمی‌شود."));
    }
    if !["nginx-test", "nginx-reload", "nginx-restart", "site-enable", "site-disable", "site-access-log", "site-error-log", "site-files", "app-start", "app-restart", "app-reload"].contains(&input.operation.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "اکشن انتخاب‌شده پشتیبانی نمی‌شود."));
    }
    Ok(())
}

pub(super) async fn list_site_actions(State(state): State<AppState>, Path(name): Path<String>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap(); require_admin(&db, &headers)?;
    if !valid_nginx_site_name(&name) { return Err(error(StatusCode::BAD_REQUEST, "نام سایت معتبر نیست.")); }
    let mut stmt = db.prepare("SELECT id,site_name,title,icon,operation FROM nginx_site_actions WHERE site_name=?1 ORDER BY id").map_err(db_error)?;
    let rows = stmt.query_map([&name], |row| Ok(NginxSiteAction { id:row.get(0)?,site_name:row.get(1)?,title:row.get(2)?,icon:row.get(3)?,operation:row.get(4)? })).map_err(db_error)?;
    Ok(Json(json!({"actions":rows.collect::<Result<Vec<_>,_>>().map_err(db_error)?})))
}

pub(super) async fn create_site_action(State(state): State<AppState>, Path(name): Path<String>, headers: HeaderMap, Json(input): Json<NginxSiteActionInput>) -> ApiResult {
    check_origin(&headers)?; validate_site_action(&input)?;
    let actor = { let db=state.lock().unwrap(); require_admin(&db,&headers)?.username };
    if !valid_nginx_site_name(&name) { return Err(error(StatusCode::BAD_REQUEST, "نام سایت معتبر نیست.")); }
    nginx_manager_request_async(json!({"action":"nginx-get","name":name})).await?;
    let db=state.lock().unwrap();
    db.execute("INSERT INTO nginx_site_actions(site_name,title,icon,operation,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?5)", params![name,input.title.trim(),input.icon,input.operation,now()]).map_err(db_error)?;
    let action=read_site_action(&db,&name,db.last_insert_rowid()).map_err(db_error)?;
    audit(&db,&actor,"nginx.site.action.create",&format!("site={name} action={} operation={}",action.id,action.operation));
    Ok(Json(json!({"action":action})))
}

pub(super) async fn update_site_action(State(state): State<AppState>, Path((name,id)): Path<(String,i64)>, headers: HeaderMap, Json(input): Json<NginxSiteActionInput>) -> ApiResult {
    check_origin(&headers)?; validate_site_action(&input)?;
    let actor={let db=state.lock().unwrap();require_admin(&db,&headers)?.username};
    if !valid_nginx_site_name(&name) { return Err(error(StatusCode::BAD_REQUEST,"نام سایت معتبر نیست.")); }
    let db=state.lock().unwrap();
    read_site_action(&db,&name,id).map_err(|_|error(StatusCode::NOT_FOUND,"دکمهٔ سایت پیدا نشد."))?;
    db.execute("UPDATE nginx_site_actions SET title=?1,icon=?2,operation=?3,updated_at=?4 WHERE site_name=?5 AND id=?6",params![input.title.trim(),input.icon,input.operation,now(),name,id]).map_err(db_error)?;
    let action=read_site_action(&db,&name,id).map_err(db_error)?;
    audit(&db,&actor,"nginx.site.action.update",&format!("site={name} action={id} operation={}",action.operation));
    Ok(Json(json!({"action":action})))
}

pub(super) async fn delete_site_action(State(state): State<AppState>, Path((name,id)): Path<(String,i64)>, headers: HeaderMap) -> ApiResult {
    check_origin(&headers)?;
    let db=state.lock().unwrap();let actor=require_admin(&db,&headers)?.username;
    if !valid_nginx_site_name(&name) { return Err(error(StatusCode::BAD_REQUEST,"نام سایت معتبر نیست.")); }
    let action=read_site_action(&db,&name,id).map_err(|_|error(StatusCode::NOT_FOUND,"دکمهٔ سایت پیدا نشد."))?;
    db.execute("DELETE FROM nginx_site_actions WHERE site_name=?1 AND id=?2",params![name,id]).map_err(db_error)?;
    audit(&db,&actor,"nginx.site.action.delete",&format!("site={name} action={id}"));
    Ok(Json(json!({"ok":true,"deleted":action.title})))
}

pub(super) async fn run_site_action(State(state): State<AppState>, Path((name,id)): Path<(String,i64)>, headers: HeaderMap) -> ApiResult {
    check_origin(&headers)?;
    let (actor,action,app_unit)={
        let db=state.lock().unwrap();let actor=require_admin(&db,&headers)?.username;
        if !valid_nginx_site_name(&name) { return Err(error(StatusCode::BAD_REQUEST,"نام سایت معتبر نیست.")); }
        let action=read_site_action(&db,&name,id).map_err(|_|error(StatusCode::NOT_FOUND,"دکمهٔ سایت پیدا نشد."))?;
        let unit=db.query_row("SELECT unit FROM nginx_site_runtime WHERE site_name=?1",[&name],|row|row.get::<_,String>(0)).optional().map_err(db_error)?;
        (actor,action,unit)
    };
    let result=match action.operation.as_str() {
        "nginx-test"|"nginx-reload"|"nginx-restart" => {
            let op=action.operation.trim_start_matches("nginx-");
            let result=nginx_manager_request_async(json!({"action":format!("nginx-{op}")})).await?;
            json!({"message":format!("Nginx {op} completed"),"output":result})
        },
        "site-enable"|"site-disable" => {
            let enabled=action.operation=="site-enable";
            let result=nginx_manager_request_async(json!({"action":"nginx-toggle","name":name,"enabled":enabled})).await?;
            json!({"message":if enabled{"سایت فعال شد."}else{"سایت غیرفعال شد."},"output":result})
        },
        "site-access-log"|"site-error-log" => {
            let kind=if action.operation=="site-access-log"{"access"}else{"error"};
            nginx_manager_request_async(json!({"action":"nginx-site-logs","name":name,"kind":kind})).await?
        },
        "site-files" => {
            let (path,_)=nginx_site_paths(&name).map_err(|message|error(StatusCode::BAD_REQUEST,&message))?;
            let config=fs::read_to_string(path).map_err(|_|error(StatusCode::BAD_REQUEST,"فایل کانفیگ سایت خوانده نشد."))?;
            let root=config.lines().find_map(|line|{let mut parts=line.trim().trim_end_matches(';').split_whitespace();(parts.next()?=="root").then(||parts.next().unwrap_or("").to_owned())}).unwrap_or_default();
            json!({"message":"مسیر فایل‌های سایت آماده است.","path":root})
        },
        "app-start"|"app-restart"|"app-reload" => {
            let unit=app_unit.ok_or_else(||error(StatusCode::BAD_REQUEST,"برای این سایت هنوز سرویس اختصاصی برنامه در تنظیمات منابع ثبت نشده است."))?;
            if !super::valid_service_unit(&unit)||super::protected_service_mutation(&unit,action.operation.trim_start_matches("app-")) {return Err(error(StatusCode::BAD_REQUEST,"سرویس متصل‌شده برای این عملیات معتبر نیست."));}
            let operation=action.operation.trim_start_matches("app-");
            let response=privileged_request_async(json!({"action":"service-action","operation":operation,"unit":unit})).await?;
            if response["ok"]!=true {return Err(error(StatusCode::BAD_GATEWAY,response["error"].as_str().unwrap_or("عملیات سرویس برنامه انجام نشد.")));}
            json!({"message":format!("سرویس {unit} {operation} شد."),"output":response["stdout"]})
        },
        _=>return Err(error(StatusCode::BAD_REQUEST,"اکشن پشتیبانی نمی‌شود.")),
    };
    let db=state.lock().unwrap();
    audit(&db,&actor,"nginx.site.action.run",&format!("site={name} action={id} operation={}",action.operation));
    Ok(Json(result))
}

pub(super) async fn api_nginx_site_logs(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(input): Json<NginxSiteLogInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&name) || !["access", "error"].contains(&input.kind.as_str()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نوع لاگ یا نام سایت معتبر نیست.",
        ));
    }
    let result =
        nginx_manager_request(json!({"action":"nginx-site-logs", "name":name, "kind":input.kind}))?;
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor,
        "nginx.site.logs",
        &format!("site={name} kind={}", input.kind),
    );
    Ok(Json(result))
}

pub(super) async fn api_nginx_site_traffic(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    if !valid_nginx_site_name(&name) {
        return Err(error(StatusCode::BAD_REQUEST, "نام سایت معتبر نیست."));
    }
    Ok(Json(
        nginx_manager_request_async(json!({"action":"nginx-site-traffic", "name":name})).await?,
    ))
}

pub(super) async fn api_nginx_site_runtime(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    method: axum::http::Method,
    body: Option<Json<NginxSiteRuntimeInput>>,
) -> ApiResult {
    if method == axum::http::Method::PUT { check_origin(&headers)?; }
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&name) {
        return Err(error(StatusCode::BAD_REQUEST, "نام سایت معتبر نیست."));
    }
    nginx_manager_request_async(json!({"action":"nginx-get", "name":name})).await?;
    if method == axum::http::Method::PUT {
        let input = body.ok_or_else(|| error(StatusCode::BAD_REQUEST, "تنظیمات منابع ارسال نشده است."))?.0;
        let unit = input.unit.trim();
        if !super::valid_service_unit(unit) {
            return Err(error(StatusCode::BAD_REQUEST, "نام واحد systemd معتبر نیست."));
        }
        if super::shared_web_service(unit) {
            return Err(error(StatusCode::BAD_REQUEST, "سرویس وب یا PHP-FPM بین چند سایت مشترک است و محدودیتش همه را تحت‌تأثیر می‌گذارد. برای این سایت یک سرویس برنامهٔ مستقل معرفی کن."));
        }
        if input.cpu_quota_percent.is_some_and(|value| !value.is_finite() || !(1.0..=10000.0).contains(&value))
            || input.memory_max_bytes.is_some_and(|value| !(16 * 1024 * 1024..=1_099_511_627_776).contains(&value))
        {
            return Err(error(StatusCode::BAD_REQUEST, "واحد systemd یا سقف CPU/RAM معتبر نیست. CPU باید 1 تا 10000 درصد و RAM حداقل 16 MiB باشد."));
        }
        let result = privileged_request_async(json!({
            "action":"service-limits", "unit":unit,
            "cpu_quota_percent":input.cpu_quota_percent,
            "memory_max_bytes":input.memory_max_bytes
        })).await?;
        if result["ok"] != true {
            return Err(error(StatusCode::BAD_REQUEST, result["error"].as_str().unwrap_or("اعمال محدودیت منابع سرویس ناموفق بود.")));
        }
        let db = state.lock().unwrap();
        db.execute("INSERT INTO nginx_site_runtime(site_name,unit,cpu_quota_percent,memory_max_bytes,last_cpu_usage_ns,last_sampled_at,updated_at) VALUES (?1,?2,?3,?4,NULL,NULL,?5) ON CONFLICT(site_name) DO UPDATE SET unit=excluded.unit,cpu_quota_percent=excluded.cpu_quota_percent,memory_max_bytes=excluded.memory_max_bytes,last_cpu_usage_ns=NULL,last_sampled_at=NULL,updated_at=excluded.updated_at", params![name, unit, input.cpu_quota_percent, input.memory_max_bytes.map(|value| value as i64), now()]).map_err(db_error)?;
        audit(&db, &actor, "nginx.site.runtime.limits", &format!("site={name} unit={unit}"));
    }
    let (unit, cpu_limit, memory_limit, previous_cpu, previous_at): (String, Option<f64>, Option<i64>, Option<i64>, Option<i64>) = {
        let db = state.lock().unwrap();
        db.query_row("SELECT unit,cpu_quota_percent,memory_max_bytes,last_cpu_usage_ns,last_sampled_at FROM nginx_site_runtime WHERE site_name=?1", [&name], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)))
            .optional().map_err(db_error)?
            .map(|row| row)
            .unwrap_or_else(|| (String::new(),None,None,None,None))
    };
    if unit.is_empty() {
        return Ok(Json(json!({"configured":false,"site":name})));
    }
    let result = privileged_request_async(json!({"action":"service-resources","unit":unit})).await?;
    if result["ok"] != true {
        return Err(error(StatusCode::BAD_GATEWAY, result["error"].as_str().unwrap_or("خواندن مصرف سرویس ناموفق بود.")));
    }
    let data = result["data"].clone();
    let cpu_now = data["cpu_usage_ns"].as_u64();
    let sampled_at = now();
    let cpu_percent = match (previous_cpu.and_then(|value| u64::try_from(value).ok()), previous_at) {
        (Some(old_cpu), Some(old_at)) if sampled_at > old_at => cpu_now.filter(|current| *current >= old_cpu).map(|current| (current - old_cpu) as f64 / ((sampled_at - old_at) as f64 * 10_000_000.0)),
        _ => None,
    };
    {
        let db = state.lock().unwrap();
        db.execute("UPDATE nginx_site_runtime SET last_cpu_usage_ns=?1,last_sampled_at=?2 WHERE site_name=?3", params![cpu_now.map(|value| value as i64), sampled_at, name]).map_err(db_error)?;
    }
    Ok(Json(json!({"configured":true,"unit":unit,"cpu_quota_percent":cpu_limit,"memory_max_bytes":memory_limit,"cpu_percent":cpu_percent,"memory_current_bytes":data["memory_current_bytes"],"sampled_at":sampled_at,"note":"Resource values apply to the associated systemd unit. A shared PHP-FPM unit affects every site using that pool."})))
}

pub(super) async fn api_nginx_site_repository(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    if !valid_nginx_site_name(&name) {
        return Err(error(StatusCode::BAD_REQUEST, "نام سایت معتبر نیست."));
    }
    Ok(Json(
        nginx_manager_request_async(json!({"action":"nginx-site-repository", "name":name})).await?,
    ))
}

pub(super) async fn check_nginx_domains(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<NginxDomainCheckQuery>,
) -> ApiResult {
    check_origin(&headers)?;
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    if query.hosts.len() > 2048 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "فهرست دامنه‌ها بیش از حد طولانی است.",
        ));
    }
    let hosts: Vec<String> = query
        .hosts
        .split(|ch: char| ch == ',' || ch.is_whitespace())
        .filter(|host| !host.is_empty())
        .map(str::to_owned)
        .collect();
    if hosts.is_empty() || hosts.len() > 20 || hosts.iter().any(|host| !valid_domain_name(host)) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "دامنه نامعتبر است یا تعداد دامنه‌ها از 20 بیشتر شده است.",
        ));
    }
    let mut results = Vec::with_capacity(hosts.len());
    for host in hosts {
        let query_host = host.strip_prefix("*.").unwrap_or(&host);
        match tokio::net::lookup_host((query_host, 80)).await {
            Ok(addresses) => {
                let mut values: Vec<String> =
                    addresses.map(|address| address.ip().to_string()).collect();
                values.sort();
                values.dedup();
                results.push(json!({"host":host,"resolved":!values.is_empty(),"addresses":values}));
            }
            Err(err) => results
                .push(json!({"host":host,"resolved":false,"addresses":[],"error":err.to_string()})),
        }
    }
    Ok(Json(json!({"domains":results})))
}

pub(super) async fn get_nginx_site(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    if !valid_nginx_site_name(&name) {
        return Err(error(StatusCode::BAD_REQUEST, "نام سایت معتبر نیست."));
    }
    let mut result = nginx_manager_request(json!({"action":"nginx-get","name":name}))?;
    let db = state.lock().unwrap();
    let content = nginx_site_content(&result)?.to_owned();
    let snapshot: Option<(String, Option<String>, String)> = db
        .query_row(
            "SELECT content,settings_json,content_hash FROM nginx_site_config_state WHERE name=?1",
            [&name],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok();
    if let Some((db_content, settings, db_hash)) = snapshot {
        let mismatch = db_hash != nginx_content_hash(&content);
        result["site"]["sync_mismatch"] = json!(mismatch);
        result["site"]["db_content"] = json!(db_content);
        result["site"]["settings"] = settings
            .and_then(|value| serde_json::from_str::<Value>(&value).ok())
            .unwrap_or(Value::Null);
    } else {
        store_nginx_snapshot(&db, &name, &content, None).map_err(db_error)?;
        result["site"]["sync_mismatch"] = json!(false);
        result["site"]["db_content"] = json!(content);
        result["site"]["settings"] = Value::Null;
    }
    Ok(Json(result))
}

pub(super) async fn save_nginx_site(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(input): Json<NginxSiteContentInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&name) {
        return Err(error(StatusCode::BAD_REQUEST, "نام سایت معتبر نیست."));
    }
    if input.content.len() > 512 * 1024 {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "تنظیمات سایت نمی‌تواند بیشتر از 512 KB باشد.",
        ));
    }
    let result =
        nginx_manager_request(json!({"action":"nginx-save","name":name,"content":input.content}))?;
    let db = state.lock().unwrap();
    store_nginx_snapshot(&db, &name, &input.content, input.settings.as_ref()).map_err(db_error)?;
    audit(&db, &actor, "nginx.site.edit", &format!("site={name}"));
    Ok(Json(result))
}

pub(super) async fn sync_nginx_site(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(input): Json<NginxSiteSyncInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&name) || !["database", "file"].contains(&input.source.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "منبع همگام‌سازی معتبر نیست."));
    }
    let db = state.lock().unwrap();
    if input.source == "database" {
        let (content, settings): (String, Option<String>) = db
            .query_row(
                "SELECT content,settings_json FROM nginx_site_config_state WHERE name=?1",
                [&name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| {
                error(
                    StatusCode::CONFLICT,
                    "نسخه‌ای در دیتابیس برای این سایت ذخیره نشده است.",
                )
            })?;
        nginx_manager_request(json!({"action":"nginx-save","name":name,"content":content}))?;
        let parsed = settings.and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
        store_nginx_snapshot(&db, &name, &content, parsed.as_ref()).map_err(db_error)?;
    } else {
        nginx_manager_request(json!({"action":"nginx-test"}))?;
        let file = nginx_manager_request(json!({"action":"nginx-get","name":name}))?;
        let content = nginx_site_content(&file)?;
        // Keep the visual editor's DB metadata while accepting the on-disk file.
        let settings: Option<Option<String>> = db
            .query_row(
                "SELECT settings_json FROM nginx_site_config_state WHERE name=?1",
                [&name],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?;
        let parsed = settings
            .flatten()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
        store_nginx_snapshot(&db, &name, content, parsed.as_ref()).map_err(db_error)?;
    }
    audit(
        &db,
        &actor,
        "nginx.site.sync",
        &format!("site={name} source={}", input.source),
    );
    Ok(Json(
        json!({"ok":true,"source":input.source,"message":"همگام‌سازی انجام شد. کانفیگ Nginx نیز اعتبارسنجی شد."}),
    ))
}

pub(super) async fn issue_nginx_site_ssl(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(input): Json<NginxSslIssueInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&name)
        || input.domains.is_empty()
        || input.domains.len() > 20
        || input
            .domains
            .iter()
            .any(|domain| !valid_domain_name(domain) || domain.starts_with("*."))
        || input.email.len() > 254
        || !input
            .email
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && valid_domain_name(domain))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "دامنه یا ایمیل معتبر نیست؛ wildcard با HTTP-01 پشتیبانی نمی‌شود.",
        ));
    }
    let mut domains = input.domains.clone();
    domains.sort();
    domains.dedup();
    if domains.len() != input.domains.len() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "دامنه تکراری در فهرست وجود دارد.",
        ));
    }
    let result = nginx_manager_request(
        json!({"action":"nginx-ssl-issue","name":name,"email":input.email,"domains":domains,"redirect":input.redirect}),
    )?;
    if result["ok"] == false {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("گرفتن گواهی SSL ناموفق بود."),
        ));
    }
    let data = result.get("data").cloned().unwrap_or(result);
    let current = nginx_manager_request(json!({"action":"nginx-get","name":name}))?;
    let content = nginx_site_content(&current)?;
    let db = state.lock().unwrap();
    let settings: Option<Option<String>> = db
        .query_row(
            "SELECT settings_json FROM nginx_site_config_state WHERE name=?1",
            [&name],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?;
    let parsed = settings
        .flatten()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
    store_nginx_snapshot(&db, &name, content, parsed.as_ref()).map_err(db_error)?;
    audit(
        &db,
        &actor,
        "nginx.site.ssl",
        &format!("site={name} domains={}", domains.join(",")),
    );
    Ok(Json(data))
}

pub(super) async fn toggle_nginx_site(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(input): Json<NginxSiteEnabledInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if !valid_nginx_site_name(&name) {
        return Err(error(StatusCode::BAD_REQUEST, "نام سایت معتبر نیست."));
    }
    let result = nginx_manager_request(
        json!({"action":"nginx-toggle","name":name,"enabled":input.enabled}),
    )?;
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor,
        "nginx.site.toggle",
        &format!("site={name} enabled={}", input.enabled),
    );
    Ok(Json(result))
}

fn valid_nginx_site_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_domain_name(host: &str) -> bool {
    let host = host.strip_prefix("*.").unwrap_or(host);
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn nginx_site_paths(name: &str) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    if !valid_nginx_site_name(name) {
        return Err("نام سایت معتبر نیست.".into());
    }
    let available_dir = std::path::Path::new(NGINX_AVAILABLE_DIR);
    let enabled_dir = std::path::Path::new(NGINX_ENABLED_DIR);
    if !available_dir.is_dir() || !enabled_dir.is_dir() {
        return Err("Nginx نصب نیست یا پوشه‌های sites-available و sites-enabled پیدا نشدند.".into());
    }
    Ok((available_dir.join(name), enabled_dir.join(name)))
}

fn nginx_site_enabled(name: &str) -> Result<bool, String> {
    let (available, enabled) = nginx_site_paths(name)?;
    let available_meta = fs::symlink_metadata(&available).map_err(|_| "فایل سایت پیدا نشد.")?;
    if !available_meta.file_type().is_file() {
        return Err("فایل سایت باید یک فایل معمولی در sites-available باشد.".into());
    }
    match fs::symlink_metadata(&enabled) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
        Ok(meta) if !meta.file_type().is_symlink() => {
            Err("مسیر sites-enabled اشغال شده و symlink معتبر نیست.".into())
        }
        Ok(_) => {
            let target = fs::canonicalize(&enabled)
                .map_err(|_| "symlink سایت شکسته یا مقصد آن نامعتبر است.")?;
            let expected = fs::canonicalize(&available).map_err(|error| error.to_string())?;
            if target == expected {
                Ok(true)
            } else {
                Err("symlink موجود به فایل sites-available همین سایت اشاره نمی‌کند.".into())
            }
        }
    }
}

fn nginx_config_test() -> Result<(), String> {
    let output = std::process::Command::new("nginx")
        .arg("-t")
        .output()
        .map_err(|error| format!("اجرای nginx -t ناموفق بود: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if detail.is_empty() {
            "اعتبارسنجی تنظیمات Nginx ناموفق بود.".into()
        } else {
            detail
        })
    }
}

fn nginx_reload_if_active() -> Result<(), String> {
    let active = std::process::Command::new("systemctl")
        .args(["is-active", "--quiet", "nginx"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !active {
        return Ok(());
    }
    let output = std::process::Command::new("systemctl")
        .args(["reload", "nginx"])
        .output()
        .map_err(|error| format!("بارگذاری مجدد Nginx ناموفق بود: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if detail.is_empty() {
            "بارگذاری مجدد Nginx ناموفق بود.".into()
        } else {
            detail
        })
    }
}

fn nginx_list_sites() -> Result<Value, String> {
    let available_dir = std::path::Path::new(NGINX_AVAILABLE_DIR);
    if !available_dir.is_dir() {
        return Err("Nginx نصب نیست یا sites-available پیدا نشد.".into());
    }
    let mut sites = Vec::new();
    for entry in fs::read_dir(available_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !valid_nginx_site_name(&name) {
            continue;
        }
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        if metadata.len() > 512 * 1024 {
            continue;
        }
        let enabled = nginx_site_enabled(&name)?;
        let content = fs::read_to_string(&entry.path()).map_err(|error| error.to_string())?;
        let content_hash = hex::encode(Sha256::digest(content.as_bytes()));
        sites.push(json!({"name":name,"enabled":enabled,"bytes":metadata.len(),"content_hash":content_hash}));
    }
    sites.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    Ok(json!(sites))
}

fn nginx_get_site(name: &str) -> Result<Value, String> {
    let (available, _) = nginx_site_paths(name)?;
    let metadata = fs::symlink_metadata(&available).map_err(|_| "فایل سایت پیدا نشد.")?;
    if !metadata.file_type().is_file() {
        return Err("فایل سایت باید یک فایل معمولی باشد.".into());
    }
    if metadata.len() > 512 * 1024 {
        return Err("حجم تنظیمات سایت از حد مجاز بیشتر است.".into());
    }
    let content = fs::read_to_string(&available).map_err(|error| error.to_string())?;
    Ok(
        json!({"name":name,"enabled":nginx_site_enabled(name)?,"content":content,"bytes":metadata.len()}),
    )
}

fn nginx_atomic_write(path: &std::path::Path, content: &[u8], mode: u32) -> Result<(), String> {
    let parent = path.parent().ok_or("مسیر تنظیمات نامعتبر است.")?;
    let file_name = path
        .file_name()
        .and_then(|part| part.to_str())
        .ok_or("نام فایل نامعتبر است.")?;
    let temp = parent.join(format!(".arg-{file_name}-{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| error.to_string())?;
        file.write_all(content).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::set_permissions(&temp, fs::Permissions::from_mode(mode & 0o777))
            .map_err(|error| error.to_string())?;
        fs::rename(&temp, path).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn nginx_issue_ssl(
    name: &str,
    email: &str,
    domains: &[String],
    redirect: bool,
) -> Result<Value, String> {
    if !valid_nginx_site_name(name)
        || domains.is_empty()
        || domains.len() > 20
        || domains
            .iter()
            .any(|domain| !valid_domain_name(domain) || domain.starts_with("*."))
    {
        return Err("دامنه معتبر نیست؛ HTTP-01 برای wildcard مناسب نیست.".into());
    }
    let (site_path, _) = nginx_site_paths(name)?;
    if !nginx_site_enabled(name)? {
        return Err("ابتدا سایت را روشن کن؛ صدور گواهی برای سایت غیرفعال انجام نمی‌شود.".into());
    }
    if !nginx_service_status()["active"].as_bool().unwrap_or(false) {
        return Err("سرویس Nginx باید روشن باشد تا Certbot بتواند challenge را پاسخ دهد.".into());
    }
    let original_metadata = fs::metadata(&site_path).map_err(|error| error.to_string())?;
    let original_content = fs::read(&site_path).map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(&original_content);
    if name == "arg" {
        return Err(
            "این سایت برای پنل ARG رزرو است؛ ابزار SSL سایت‌ها نباید گواهی پنل را تغییر دهد.".into(),
        );
    }
    let mut configured_domains = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("server_name ") {
            if let Some(value) = trimmed
                .strip_prefix("server_name ")
                .and_then(|value| value.split(';').next())
            {
                configured_domains.extend(
                    value
                        .split_whitespace()
                        .map(|value| value.to_ascii_lowercase()),
                );
            }
        }
    }
    for domain in domains {
        if !configured_domains
            .iter()
            .any(|configured| configured == &domain.to_ascii_lowercase())
        {
            return Err(format!("دامنهٔ {domain} در server_name همین سایت نیست. ابتدا دامنه‌های سایت را ذخیره و دوباره بررسی کن."));
        }
    }
    let panel_domain = fs::read_to_string("/etc/arg/https-domain")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !panel_domain.is_empty()
        && domains
            .iter()
            .any(|domain| domain.eq_ignore_ascii_case(&panel_domain))
    {
        return Err(
            "دامنهٔ پنل ARG را با ابزار SSL سایت‌ها مدیریت نکن؛ گواهی پنل بخش جداگانه‌ای دارد.".into(),
        );
    }
    let version = std::process::Command::new("certbot")
        .arg("--version")
        .output()
        .map_err(|_| "Certbot نصب نیست. ابتدا بستهٔ python3-certbot-nginx را نصب کن.".to_owned())?;
    if !version.status.success() {
        return Err(
            "Certbot نصب نیست یا اجرا نمی‌شود. بستهٔ python3-certbot-nginx را نصب کن.".into(),
        );
    }
    let backup_dir = std::path::Path::new("/var/backups/arg/nginx-ssl");
    fs::create_dir_all(backup_dir)
        .map_err(|error| format!("ساخت پوشهٔ پشتیبان ناموفق بود: {error}"))?;
    fs::set_permissions(backup_dir, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    let backup = backup_dir.join(format!("{name}-{}.conf", now()));
    fs::write(&backup, &original_content)
        .map_err(|error| format!("تهیهٔ پشتیبان ناموفق بود: {error}"))?;
    fs::set_permissions(&backup, fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;

    // Keep site certificates in their own lineage. Certbot otherwise tries to
    // expand an unrelated certificate (for example the panel certificate) and
    // prompts even when --non-interactive is set.
    let certificate_name = format!(
        "arg-site-{}",
        &hex::encode(Sha256::digest(name.as_bytes()))[..16]
    );
    let has_site_lineage = std::path::Path::new("/etc/letsencrypt/renewal")
        .join(format!("{certificate_name}.conf"))
        .exists();
    let mut command = std::process::Command::new("timeout");
    command.args([
        "600s",
        "certbot",
        "--nginx",
        "--non-interactive",
        "--agree-tos",
        "--cert-name",
        &certificate_name,
        "--email",
        email,
    ]);
    command.arg(if has_site_lineage {
        "--expand"
    } else {
        "--duplicate"
    });
    for domain in domains {
        command.arg("-d").arg(domain);
    }
    command.arg(if redirect {
        "--redirect"
    } else {
        "--no-redirect"
    });
    let output = command
        .output()
        .map_err(|error| format!("اجرای Certbot ناموفق بود: {error}"))?;
    let mut detail = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        let _ = nginx_atomic_write(
            &site_path,
            &original_content,
            original_metadata.permissions().mode(),
        );
        let _ = nginx_reload_if_active();
        detail.truncate(6000);
        return Err(format!("صدور یا نصب گواهی کامل نشد؛ فایل سایت از پشتیبان بازگردانده شد. پورت 80، DNS و دسترسی Certbot را بررسی کن.\n{detail}"));
    }
    if let Err(error) = nginx_config_test() {
        let _ = nginx_atomic_write(
            &site_path,
            &original_content,
            original_metadata.permissions().mode(),
        );
        let _ = nginx_reload_if_active();
        return Err(format!(
            "پس از Certbot آزمون nginx -t شکست خورد؛ کانفیگ سایت بازگردانده شد: {error}"
        ));
    }
    if let Err(error) = nginx_reload_if_active() {
        let _ = nginx_atomic_write(
            &site_path,
            &original_content,
            original_metadata.permissions().mode(),
        );
        let _ = nginx_reload_if_active();
        return Err(format!(
            "راه‌اندازی مجدد Nginx ناموفق شد؛ کانفیگ سایت بازگردانده شد: {error}"
        ));
    }
    let certificate = format!("/etc/letsencrypt/live/{certificate_name}/fullchain.pem");
    detail.truncate(3000);
    Ok(
        json!({"message":"گواهی صادر و در تنظیمات سایت فعال شد. Certbot تمدید خودکار را مدیریت می‌کند.","domains":domains,"certificate":certificate,"backup":backup,"output":detail.trim()}),
    )
}

fn nginx_save_site(name: &str, content: &str) -> Result<Value, String> {
    if content.len() > 512 * 1024 || content.contains('\0') {
        return Err("تنظیمات خالی، نامعتبر یا بزرگ‌تر از 512 KB است.".into());
    }
    let (available, _) = nginx_site_paths(name)?;
    let metadata = fs::symlink_metadata(&available).map_err(|_| "فایل سایت پیدا نشد.")?;
    if !metadata.file_type().is_file() {
        return Err("فقط فایل معمولی داخل sites-available قابل ویرایش است.".into());
    }
    let old_content = fs::read(&available).map_err(|error| error.to_string())?;
    let enabled = nginx_site_enabled(name)?;
    nginx_atomic_write(
        &available,
        content.as_bytes(),
        metadata.permissions().mode(),
    )?;
    if let Err(error) = nginx_config_test() {
        let _ = nginx_atomic_write(&available, &old_content, metadata.permissions().mode());
        return Err(format!(
            "تنظیمات ذخیره نشد و نسخهٔ قبلی بازگردانده شد: {error}"
        ));
    }
    if enabled {
        if let Err(error) = nginx_reload_if_active() {
            let _ = nginx_atomic_write(&available, &old_content, metadata.permissions().mode());
            let _ = nginx_reload_if_active();
            return Err(format!(
                "بارگذاری تنظیمات ناموفق بود؛ نسخهٔ قبلی بازگردانده شد: {error}"
            ));
        }
    }
    Ok(
        json!({"name":name,"enabled":enabled,"message":if enabled { "تنظیمات ذخیره و بررسی شد." } else { "تنظیمات ذخیره و بررسی شد؛ سایت غیرفعال است." }}),
    )
}

fn nginx_set_site_enabled(name: &str, enabled: bool) -> Result<Value, String> {
    let (available, link) = nginx_site_paths(name)?;
    let metadata = fs::symlink_metadata(&available).map_err(|_| "فایل سایت پیدا نشد.")?;
    if !metadata.file_type().is_file() {
        return Err("فقط فایل معمولی داخل sites-available قابل فعال‌سازی است.".into());
    }
    let was_enabled = nginx_site_enabled(name)?;
    if was_enabled == enabled {
        return Ok(json!({"name":name,"enabled":enabled,"message":"وضعیت سایت تغییری نکرد."}));
    }
    if enabled {
        std::os::unix::fs::symlink(format!("../sites-available/{name}"), &link)
            .map_err(|error| format!("ساخت symlink خودکار ناموفق بود: {error}"))?;
    } else {
        fs::remove_file(&link).map_err(|error| format!("حذف symlink ناموفق بود: {error}"))?;
    }
    let apply = nginx_config_test().and_then(|_| nginx_reload_if_active());
    if let Err(error) = apply {
        if enabled {
            let _ = fs::remove_file(&link);
        } else {
            let _ = std::os::unix::fs::symlink(format!("../sites-available/{name}"), &link);
        }
        return Err(format!(
            "تغییر اعمال نشد و وضعیت قبلی بازگردانده شد: {error}"
        ));
    }
    Ok(
        json!({"name":name,"enabled":enabled,"message":if enabled { "سایت فعال شد و symlink ساخته شد." } else { "سایت غیرفعال شد و symlink حذف شد." }}),
    )
}

fn nginx_command(program: &str, args: &[&str]) -> Result<std::process::Output, String> {
    std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("اجرای {program} ناموفق بود: {error}"))
}

fn nginx_service_status() -> Value {
    let installed = std::path::Path::new("/usr/sbin/nginx").exists()
        || std::path::Path::new("/usr/bin/nginx").exists();
    let active = nginx_command("systemctl", &["is-active", "nginx"])
        .ok()
        .is_some_and(|out| {
            out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "active"
        });
    let enabled = nginx_command("systemctl", &["is-enabled", "nginx"])
        .ok()
        .is_some_and(|out| {
            out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "enabled"
        });
    let version = if installed {
        nginx_command("nginx", &["-v"])
            .ok()
            .map(|out| {
                let text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
                text.trim().to_owned()
            })
            .unwrap_or_default()
    } else {
        String::new()
    };
    let config_test = if installed {
        nginx_command("nginx", &["-t"])
            .ok()
            .map(|out| {
                let text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
                json!({"ok":out.status.success(),"output":text.trim()})
            })
            .unwrap_or_else(|| json!({"ok":false,"output":"اجرای nginx -t ناموفق بود."}))
    } else {
        json!({"ok":false,"output":"Nginx نصب نیست."})
    };
    json!({"installed":installed,"active":active,"enabled":enabled,"version":version,"config_test":config_test})
}

fn nginx_manage(action: &str) -> Result<Value, String> {
    match action {
        "nginx-status" => Ok(nginx_service_status()),
        "nginx-install" => {
            if std::path::Path::new("/usr/sbin/nginx").exists() {
                return Ok(json!({"message":"Nginx از قبل نصب است."}));
            }
            let (program, args): (&str, &[&str]) =
                if std::path::Path::new("/usr/bin/apt-get").exists() {
                    ("apt-get", &["install", "-y", "nginx"])
                } else if std::path::Path::new("/usr/bin/dnf").exists() {
                    ("dnf", &["install", "-y", "nginx"])
                } else if std::path::Path::new("/usr/bin/yum").exists() {
                    ("yum", &["install", "-y", "nginx"])
                } else {
                    return Err("مدیر بستهٔ پشتیبانی‌شده پیدا نشد (apt، dnf یا yum).".into());
                };
            let output = nginx_command(program, args)?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
            }
            let enable = nginx_command("systemctl", &["enable", "--now", "nginx"])?;
            if !enable.status.success() {
                return Err(format!(
                    "نصب انجام شد، اما راه‌اندازی سرویس ناموفق بود: {}",
                    String::from_utf8_lossy(&enable.stderr).trim()
                ));
            }
            Ok(json!({"message":"Nginx نصب و سرویس آن راه‌اندازی شد."}))
        }
        "nginx-start" | "nginx-stop" | "nginx-restart" | "nginx-reload" | "nginx-enable"
        | "nginx-disable" => {
            let verb = action.trim_start_matches("nginx-");
            if !nginx_service_status()["installed"]
                .as_bool()
                .unwrap_or(false)
            {
                return Err("Nginx نصب نیست.".into());
            }
            if matches!(verb, "reload" | "start" | "restart") {
                nginx_config_test()?;
            }
            let args: &[&str] = match verb {
                "enable" => &["enable", "nginx"],
                "disable" => &["disable", "nginx"],
                _ => &[verb, "nginx"],
            };
            let output = nginx_command("systemctl", args)?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
            }
            Ok(json!({"message":format!("دستور {verb} برای Nginx با موفقیت اجرا شد.")}))
        }
        "nginx-test" => {
            let output = nginx_command("nginx", &["-t"])?;
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if output.status.success() {
                Ok(json!({"ok":true,"output":text.trim(),"message":"تنظیمات Nginx معتبر است."}))
            } else {
                Err(text.trim().to_owned())
            }
        }
        "nginx-logs" => {
            let output = nginx_command(
                "journalctl",
                &["-u", "nginx", "-n", "100", "--no-pager", "-o", "short-iso"],
            )?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
            }
            Ok(json!({"logs":String::from_utf8_lossy(&output.stdout)}))
        }
        _ => Err("عملیات سرویس Nginx معتبر نیست.".into()),
    }
}

fn nginx_create_site(name: &str, content: &str) -> Result<Value, String> {
    if !valid_nginx_site_name(name)
        || content.is_empty()
        || content.len() > 512 * 1024
        || content.contains('\0')
    {
        return Err("نام یا محتوای فایل سایت معتبر نیست.".into());
    }
    let (available, _) = nginx_site_paths(name)?;
    if available.exists() {
        return Err("فایلی با این نام از قبل وجود دارد.".into());
    }
    nginx_atomic_write(&available, content.as_bytes(), 0o644)?;
    if let Err(error) = nginx_config_test() {
        let _ = fs::remove_file(&available);
        return Err(format!("پیکربندی معتبر نیست؛ فایل ساخته نشد: {error}"));
    }
    Ok(
        json!({"site":{"name":name,"enabled":false,"bytes":content.len()},"message":"فایل سایت ساخته و اعتبارسنجی شد؛ برای اعمال، آن را فعال کن."}),
    )
}

fn nginx_delete_site(name: &str) -> Result<Value, String> {
    if name == "default" {
        return Err("حذف سایت پیش‌فرض سیستم از پنل مجاز نیست.".into());
    }
    let (available, enabled) = nginx_site_paths(name)?;
    let metadata = fs::symlink_metadata(&available).map_err(|_| "فایل سایت پیدا نشد.")?;
    if !metadata.file_type().is_file() {
        return Err("فقط فایل معمولی قابل حذف است.".into());
    }
    let content = fs::read(&available).map_err(|error| error.to_string())?;
    let mode = metadata.permissions().mode();
    let was_enabled = nginx_site_enabled(name)?;
    if was_enabled {
        fs::remove_file(&enabled).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::remove_file(&available) {
        if was_enabled {
            let _ = std::os::unix::fs::symlink(format!("../sites-available/{name}"), &enabled);
        }
        return Err(error.to_string());
    }
    if let Err(error) = nginx_config_test().and_then(|_| nginx_reload_if_active()) {
        let _ = nginx_atomic_write(&available, &content, mode);
        if was_enabled {
            let _ = std::os::unix::fs::symlink(format!("../sites-available/{name}"), &enabled);
        }
        let _ = nginx_reload_if_active();
        return Err(format!("حذف اعمال نشد و سایت بازگردانده شد: {error}"));
    }
    Ok(json!({"message":"فایل سایت حذف شد."}))
}

fn nginx_rename_site(name: &str, new_name: &str) -> Result<Value, String> {
    if name == "default" || new_name == "default" {
        return Err("تغییر نام سایت پیش‌فرض سیستم مجاز نیست.".into());
    }
    if !valid_nginx_site_name(name) || !valid_nginx_site_name(new_name) {
        return Err("نام فایل سایت معتبر نیست.".into());
    }
    let (old_available, old_enabled) = nginx_site_paths(name)?;
    let (new_available, new_enabled) = nginx_site_paths(new_name)?;
    let metadata = fs::symlink_metadata(&old_available).map_err(|_| "فایل سایت پیدا نشد.")?;
    if !metadata.file_type().is_file() {
        return Err("فقط فایل معمولی قابل تغییر نام است.".into());
    }
    if name == new_name {
        return Ok(json!({"name":name,"message":"نام سایت تغییری نکرد."}));
    }
    if new_available.exists() || fs::symlink_metadata(&new_enabled).is_ok() {
        return Err("یک فایل یا symlink با نام مقصد از قبل وجود دارد.".into());
    }
    let was_enabled = nginx_site_enabled(name)?;
    fs::rename(&old_available, &new_available)
        .map_err(|error| format!("تغییر نام فایل ناموفق بود: {error}"))?;
    if was_enabled {
        if let Err(error) = fs::remove_file(&old_enabled).and_then(|_| {
            std::os::unix::fs::symlink(format!("../sites-available/{new_name}"), &new_enabled)
        }) {
            let _ = fs::rename(&new_available, &old_available);
            if fs::symlink_metadata(&old_enabled).is_err() {
                let _ =
                    std::os::unix::fs::symlink(format!("../sites-available/{name}"), &old_enabled);
            }
            return Err(format!("تغییر نام symlink ناموفق بود: {error}"));
        }
    }
    if let Err(error) = nginx_config_test().and_then(|_| nginx_reload_if_active()) {
        if was_enabled {
            let _ = fs::remove_file(&new_enabled);
            let _ = std::os::unix::fs::symlink(format!("../sites-available/{name}"), &old_enabled);
        }
        let _ = fs::rename(&new_available, &old_available);
        let _ = nginx_reload_if_active();
        return Err(format!(
            "تغییر نام اعمال نشد و نام قبلی بازگردانده شد: {error}"
        ));
    }
    Ok(json!({"name":new_name,"enabled":was_enabled,"message":"نام فایل سایت تغییر کرد."}))
}

fn nginx_site_repository(name: &str) -> Result<Value, String> {
    let (available, _) = nginx_site_paths(name)?;
    let config = fs::read_to_string(available).map_err(|_| "فایل سایت پیدا نشد.")?;
    let root = config
        .lines()
        .filter_map(|line| {
            let mut words = line.trim().trim_end_matches(';').split_whitespace();
            (words.next()? == "root")
                .then(|| words.next().unwrap_or("").trim_matches('"').to_owned())
        })
        .find(|path| path.starts_with('/'))
        .ok_or("برای این سایت root تعریف نشده است.")?;
    let root = fs::canonicalize(root).map_err(|_| "ریشهٔ سایت روی دیسک پیدا نشد.")?;
    let repo = if root.join(".git").exists() {
        root.clone()
    } else if root.file_name().is_some_and(|name| name == "public")
        && root
            .parent()
            .is_some_and(|parent| parent.join(".git").exists())
    {
        root.parent().unwrap().to_path_buf()
    } else {
        return Ok(
            json!({"available":false,"message":"در ریشهٔ سایت یا پوشهٔ والد public مخزن Git پیدا نشد."}),
        );
    };
    if !repo.starts_with("/var/www/") && !repo.starts_with("/srv/") {
        return Err("برای بررسی مخزن، مسیر سایت باید زیر /var/www یا /srv باشد.".into());
    }
    let git = |args: &[&str]| -> Result<String, String> {
        let output = std::process::Command::new("timeout")
            .arg("8s")
            .arg("git")
            .args([
                "-c",
                &format!("safe.directory={}", repo.display()),
                "-c",
                "protocol.file.allow=never",
                "-c",
                "protocol.ext.allow=never",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .arg("-C")
            .arg(&repo)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    let commit = git(&["rev-parse", "HEAD"])?;
    let changes = git(&["status", "--porcelain", "--untracked-files=no"])?;
    let origin = git(&["remote", "get-url", "origin"]).unwrap_or_default();
    // The remote is only queried for GitHub URLs. No credentials are passed.
    let remote_commit = if origin.starts_with("https://github.com/")
        && origin["https://github.com/".len()..]
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "/._-".contains(ch))
    {
        git(&["ls-remote", &origin, &format!("refs/heads/{branch}")])
            .ok()
            .and_then(|line| line.split_whitespace().next().map(str::to_owned))
    } else {
        None
    };
    let displayed_origin = if origin.contains('@') {
        "Private remote URL hidden"
    } else {
        &origin
    };
    Ok(
        json!({"available":true,"path":repo,"branch":branch,"commit":commit,"changes":changes.lines().count(),"origin":displayed_origin,"remote_commit":remote_commit,"differs_from_remote":remote_commit.as_ref().is_some_and(|remote| remote != &commit)}),
    )
}

fn nginx_site_traffic(name: &str) -> Result<Value, String> {
    let (available, _) = nginx_site_paths(name)?;
    let config = fs::read_to_string(available).map_err(|_| "فایل سایت پیدا نشد.")?;
    let configured = config
        .lines()
        .filter_map(|line| {
            let mut words = line.trim().trim_end_matches(';').split_whitespace();
            (words.next()? == "access_log")
                .then(|| words.next().unwrap_or("").trim_matches('"').to_owned())
        })
        .find(|path| !path.is_empty() && path != "off");
    let Some(path) = configured else {
        return Ok(
            json!({"available":false,"message":"برای آمار مستقل این سایت، مسیر اختصاصی access_log را در تنظیمات Nginx تعیین کن."}),
        );
    };
    if !path.starts_with("/var/log/nginx/") || path.contains('$') {
        return Err("مسیر access_log باید فایل ثابتی در /var/log/nginx باشد.".into());
    }
    let allowed = fs::canonicalize("/var/log/nginx").map_err(|error| error.to_string())?;
    let resolved = fs::canonicalize(&path).map_err(|error| error.to_string())?;
    if !resolved.starts_with(&allowed) || !resolved.is_file() {
        return Err("فایل access_log معتبر نیست.".into());
    }
    let output = std::process::Command::new("tail")
        .args(["-n", "5000", "--", resolved.to_string_lossy().as_ref()])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err("خواندن access_log ناموفق بود.".into());
    }
    let mut requests = 0_u64;
    let mut bytes = 0_u64;
    let mut statuses = [0_u64; 5];
    let mut hourly: Vec<(String, u64, u64)> = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        // Combined log format: request is quoted; status and body bytes follow.
        if let Some(after_request) = line.split('"').nth(2) {
            let mut fields = after_request.split_whitespace();
            if let (Some(status), Some(size)) = (
                fields.next().and_then(|v| v.parse::<usize>().ok()),
                fields.next(),
            ) {
                if (100..600).contains(&status) {
                    requests += 1;
                    statuses[status / 100 - 1] += 1;
                    bytes = bytes.saturating_add(size.parse::<u64>().unwrap_or(0));
                    if let Some(timestamp) = line.split('[').nth(1) {
                        let label = timestamp.split(':').take(2).collect::<Vec<_>>().join(":");
                        if !label.is_empty() {
                            if let Some(row) = hourly.iter_mut().find(|row| row.0 == label) {
                                row.1 += 1;
                                row.2 = row.2.saturating_add(size.parse::<u64>().unwrap_or(0));
                            } else {
                                hourly.push((label, 1, size.parse::<u64>().unwrap_or(0)));
                            }
                        }
                    }
                }
            }
        }
    }
    let hourly = hourly.into_iter().rev().take(24).collect::<Vec<_>>().into_iter().rev().map(|(hour, requests, body_bytes)| json!({"hour":hour,"requests":requests,"body_bytes":body_bytes})).collect::<Vec<_>>();
    Ok(
        json!({"available":true,"path":path,"sample_requests":requests,"sample_body_bytes":bytes,"status_groups":statuses,"sample_limit":5000,"hourly":hourly}),
    )
}

fn nginx_site_logs(name: &str, kind: &str) -> Result<Value, String> {
    if !valid_nginx_site_name(name) || !["access", "error"].contains(&kind) {
        return Err("نام سایت یا نوع لاگ معتبر نیست.".into());
    }
    let (available, _) = nginx_site_paths(name)?;
    let config = fs::read_to_string(&available).map_err(|_| "فایل سایت پیدا نشد.")?;
    let directive = if kind == "access" {
        "access_log"
    } else {
        "error_log"
    };
    let configured = config
        .lines()
        .filter_map(|line| {
            let mut parts = line.trim().trim_end_matches(';').split_whitespace();
            (parts.next()? == directive)
                .then(|| parts.next().unwrap_or("").trim_matches('"').to_owned())
        })
        .find(|value| !value.is_empty());
    let path = configured.unwrap_or_else(|| {
        format!(
            "/var/log/nginx/{}.log",
            if kind == "access" { "access" } else { "error" }
        )
    });
    if path == "off" {
        return Ok(
            json!({"name":name,"kind":kind,"path":"off","logs":"این سایت ثبت لاگ را غیرفعال کرده است."}),
        );
    }
    if !path.starts_with('/') || path.contains('$') {
        return Err("مسیر لاگ متغیر یا نسبی است و از پنل قابل خواندن نیست.".into());
    }
    let allowed_root =
        fs::canonicalize("/var/log/nginx").map_err(|_| "پوشهٔ لاگ استاندارد Nginx پیدا نشد.")?;
    let requested = std::path::PathBuf::from(&path);
    let canonical =
        fs::canonicalize(&requested).map_err(|_| format!("فایل لاگ پیدا نشد: {path}"))?;
    if !canonical.starts_with(&allowed_root) || !canonical.is_file() {
        return Err(
            "برای محافظت از فایل‌های سیستم، فقط فایل‌های داخل /var/log/nginx قابل مشاهده‌اند.".into(),
        );
    }
    let output = std::process::Command::new("tail")
        .args(["-n", "100", "--", canonical.to_string_lossy().as_ref()])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let start = output.stdout.len().saturating_sub(512 * 1024);
    let logs = String::from_utf8_lossy(&output.stdout[start..]).into_owned();
    Ok(json!({"name":name,"kind":kind,"path":path,"logs":logs}))
}

fn nginx_browse_files(path: Option<&str>) -> Result<Value, String> {
    const ROOTS: &[&str] = &["/etc/letsencrypt", "/etc/ssl", "/etc/nginx", "/var/www"];
    let roots: Vec<String> = ROOTS
        .iter()
        .filter(|root| std::path::Path::new(root).is_dir())
        .map(|root| (*root).to_owned())
        .collect();
    let Some(path) = path else {
        return Ok(json!({"path":"", "roots":roots, "entries":[]}));
    };
    if path.len() > 1024 {
        return Err("مسیر بیش از حد طولانی است.".into());
    }
    let requested = std::path::PathBuf::from(path);
    let directory =
        fs::canonicalize(&requested).map_err(|_| "مسیر پیدا نشد یا قابل دسترسی نیست.")?;
    let allowed = roots
        .iter()
        .filter_map(|root| fs::canonicalize(root).ok())
        .any(|root| directory.starts_with(root));
    if !allowed || !directory.is_dir() {
        return Err("دسترسی فقط به پوشه‌های مجاز SSL، Nginx و وب محدود است.".into());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(&directory)
        .map_err(|error| error.to_string())?
        .take(500)
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.len() > 255 {
            continue;
        }
        let displayed = requested.join(&name);
        let Ok(target) = fs::canonicalize(entry.path()) else {
            continue;
        };
        if !roots
            .iter()
            .filter_map(|root| fs::canonicalize(root).ok())
            .any(|root| target.starts_with(root))
        {
            continue;
        }
        let is_dir = target.is_dir();
        let ext = entry
            .path()
            .extension()
            .and_then(|part| part.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let selectable =
            !is_dir && matches!(ext.as_str(), "pem" | "crt" | "cer" | "key" | "p12" | "pfx");
        if is_dir || selectable {
            entries.push(json!({"name":name,"path":displayed.to_string_lossy(),"directory":is_dir,"selectable":selectable}));
        }
    }
    entries.sort_by(|a, b| {
        b["directory"]
            .as_bool()
            .cmp(&a["directory"].as_bool())
            .then_with(|| a["name"].as_str().cmp(&b["name"].as_str()))
    });
    Ok(json!({"path":requested.to_string_lossy(),"roots":roots,"entries":entries}))
}

fn execute_nginx_request(action: &str, request: &Value) -> Value {
    let result = match action {
        "nginx-status" | "nginx-install" | "nginx-start" | "nginx-stop" | "nginx-restart"
        | "nginx-reload" | "nginx-enable" | "nginx-disable" | "nginx-test" | "nginx-logs" => {
            nginx_manage(action)
        }
        "nginx-ssl-issue" => request["name"]
            .as_str()
            .ok_or_else(|| "نام سایت ارسال نشده است.".to_owned())
            .and_then(|name| {
                let email = request["email"]
                    .as_str()
                    .ok_or_else(|| "ایمیل ارسال نشده است.".to_owned())?;
                let domains = request["domains"]
                    .as_array()
                    .ok_or_else(|| "فهرست دامنه‌ها ارسال نشده است.".to_owned())?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| "دامنه نامعتبر است.".to_owned())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                nginx_issue_ssl(
                    name,
                    email,
                    &domains,
                    request["redirect"].as_bool().unwrap_or(false),
                )
            }),
        "nginx-create" => request["name"]
            .as_str()
            .ok_or_else(|| "نام سایت ارسال نشده است.".to_owned())
            .and_then(|name| {
                request["content"]
                    .as_str()
                    .ok_or_else(|| "محتوای سایت ارسال نشده است.".to_owned())
                    .and_then(|content| nginx_create_site(name, content))
            }),
        "nginx-delete" => request["name"]
            .as_str()
            .ok_or_else(|| "نام سایت ارسال نشده است.".to_owned())
            .and_then(nginx_delete_site)
            .map(|_| json!({"message":"فایل سایت حذف شد."})),
        "nginx-rename" => request["name"]
            .as_str()
            .ok_or_else(|| "نام فعلی ارسال نشده است.".to_owned())
            .and_then(|name| {
                request["new_name"]
                    .as_str()
                    .ok_or_else(|| "نام جدید ارسال نشده است.".to_owned())
                    .and_then(|new_name| nginx_rename_site(name, new_name))
            }),
        "nginx-site-logs" => request["name"]
            .as_str()
            .ok_or_else(|| "نام سایت ارسال نشده است.".to_owned())
            .and_then(|name| {
                request["kind"]
                    .as_str()
                    .ok_or_else(|| "نوع لاگ ارسال نشده است.".to_owned())
                    .and_then(|kind| nginx_site_logs(name, kind))
            }),
        "nginx-site-traffic" => request["name"]
            .as_str()
            .ok_or_else(|| "نام سایت ارسال نشده است.".to_owned())
            .and_then(nginx_site_traffic),
        "nginx-site-repository" => request["name"]
            .as_str()
            .ok_or_else(|| "نام سایت ارسال نشده است.".to_owned())
            .and_then(nginx_site_repository),
        "nginx-browse" => nginx_browse_files(request["path"].as_str()),
        "nginx-list" => nginx_list_sites().map(|sites| json!({"sites":sites})),
        "nginx-get" => request["name"]
            .as_str()
            .ok_or_else(|| "نام سایت ارسال نشده است.".to_owned())
            .and_then(nginx_get_site)
            .map(|site| json!({"site":site})),
        "nginx-save" => request["name"]
            .as_str()
            .ok_or_else(|| "نام سایت ارسال نشده است.".to_owned())
            .and_then(|name| {
                request["content"]
                    .as_str()
                    .ok_or_else(|| "متن تنظیمات ارسال نشده است.".to_owned())
                    .and_then(|content| nginx_save_site(name, content))
            })
            .map(|site| json!({"site":site})),
        "nginx-toggle" => request["name"]
            .as_str()
            .ok_or_else(|| "نام سایت ارسال نشده است.".to_owned())
            .and_then(|name| {
                request["enabled"]
                    .as_bool()
                    .ok_or_else(|| "وضعیت سایت ارسال نشده است.".to_owned())
                    .and_then(|enabled| nginx_set_site_enabled(name, enabled))
            })
            .map(|site| json!({"site":site})),
        _ => Err("درخواست مدیریت Nginx پشتیبانی نمی‌شود.".to_owned()),
    };
    match result {
        Ok(value) => json!({"ok":true,"data":value}),
        Err(error) => json!({"ok":false,"error":error}),
    }
}

pub(super) fn execute_privileged_request(action: &str, request: &Value) -> Value {
    let _guard = NGINX_OPERATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    execute_nginx_request(action, request)
}
