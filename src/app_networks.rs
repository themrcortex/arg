use super::*;
use futures_util::StreamExt;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NetworkInput {
    name: String,
    #[serde(default = "default_driver")]
    driver: String,
    #[serde(default = "default_visibility")]
    visibility: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MemberInput {
    application_id: String,
    #[serde(default)]
    container_name: String,
    #[serde(default)]
    alias: String,
    #[serde(default)]
    internal_port: Option<u16>,
}

fn default_driver() -> String {
    "docker-bridge".into()
}
fn default_visibility() -> String {
    "private".into()
}
fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 48
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS application_networks(id TEXT PRIMARY KEY,owner_server_uuid TEXT NOT NULL,name TEXT NOT NULL,driver TEXT NOT NULL,visibility TEXT NOT NULL DEFAULT 'private',docker_network TEXT UNIQUE,created_by TEXT NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,UNIQUE(owner_server_uuid,name COLLATE NOCASE)); CREATE TABLE IF NOT EXISTS application_network_members(network_id TEXT NOT NULL REFERENCES application_networks(id) ON DELETE CASCADE,application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,container_name TEXT NOT NULL DEFAULT '',alias TEXT NOT NULL,internal_port INTEGER,created_at INTEGER NOT NULL,PRIMARY KEY(network_id,application_id),UNIQUE(network_id,alias)); CREATE INDEX IF NOT EXISTS application_network_members_app ON application_network_members(application_id);")
}

pub(super) async fn list(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let mut stmt = db.prepare("SELECT n.id,n.name,n.driver,n.visibility,n.docker_network,n.created_at,(SELECT COUNT(*) FROM application_network_members m WHERE m.network_id=n.id) FROM application_networks n WHERE owner_server_uuid=?1 ORDER BY name COLLATE NOCASE").map_err(db_error)?;
    let networks = stmt.query_map([owner], |row| Ok(json!({"id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"driver":row.get::<_,String>(2)?,"visibility":row.get::<_,String>(3)?,"docker_network":row.get::<_,Option<String>>(4)?,"created_at":row.get::<_,i64>(5)?,"member_count":row.get::<_,i64>(6)?}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
    Ok(Json(
        json!({"networks":networks,"host_local_note":"Host-local endpoints use 127.0.0.1 and do not create process or network isolation."}),
    ))
}

pub(super) async fn mesh(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let (owner, applications, networks, members) = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let mut app_statement = db.prepare(
            "SELECT id,name,framework,source_type,domain,listen_port,desired_state,observed_state FROM applications WHERE owner_server_uuid=?1 AND deleted_at IS NULL ORDER BY name COLLATE NOCASE LIMIT 500",
        ).map_err(db_error)?;
        let applications = app_statement
            .query_map([&owner], |row| {
                let id: String = row.get(0)?;
                let framework: String = row.get(2)?;
                Ok(json!({
                    "id": id,
                    "name": row.get::<_,String>(1)?,
                    "framework": framework,
                    "source_type": row.get::<_,String>(3)?,
                    "domain": row.get::<_,Option<String>>(4)?,
                    "listen_port": row.get::<_,Option<i64>>(5)?,
                    "desired_state": row.get::<_,String>(6)?,
                    "observed_state": row.get::<_,String>(7)?,
                }))
            })
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        let mut network_statement = db.prepare(
            "SELECT id,name,driver,visibility,docker_network,created_at,(SELECT COUNT(*) FROM application_network_members m WHERE m.network_id=n.id) FROM application_networks n WHERE owner_server_uuid=?1 ORDER BY name COLLATE NOCASE LIMIT 128",
        ).map_err(db_error)?;
        let networks = network_statement.query_map([&owner], |row| {
            Ok(json!({"id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"driver":row.get::<_,String>(2)?,"visibility":row.get::<_,String>(3)?,"docker_network":row.get::<_,Option<String>>(4)?,"created_at":row.get::<_,i64>(5)?,"member_count":row.get::<_,i64>(6)?}))
        }).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
        let mut member_statement = db.prepare(
            "SELECT m.network_id,m.application_id,a.name,a.framework,a.source_type,m.container_name,m.alias,m.internal_port,m.created_at FROM application_network_members m JOIN application_networks n ON n.id=m.network_id JOIN applications a ON a.id=m.application_id WHERE n.owner_server_uuid=?1 AND a.deleted_at IS NULL ORDER BY n.name COLLATE NOCASE,a.name COLLATE NOCASE LIMIT 2000",
        ).map_err(db_error)?;
        let members = member_statement.query_map([&owner], |row| {
            Ok(json!({"network_id":row.get::<_,String>(0)?,"application_id":row.get::<_,String>(1)?,"application_name":row.get::<_,String>(2)?,"framework":row.get::<_,String>(3)?,"source_type":row.get::<_,String>(4)?,"container_name":row.get::<_,String>(5)?,"alias":row.get::<_,String>(6)?,"internal_port":row.get::<_,Option<i64>>(7)?,"created_at":row.get::<_,i64>(8)?}))
        }).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
        (owner, applications, networks, members)
    };

    // Reading a Laravel .env is filesystem I/O. Do it after releasing the
    // application registry lock so the dashboard cannot hold up app CRUD.
    let applications: Vec<Value> = applications
        .into_iter()
        .map(|mut application| {
            let app_id = application["id"].as_str().unwrap_or_default();
            application["database"] = if application["framework"] == "laravel" {
                laravel::database_connection_summary(app_id)
            } else {
                json!({"state":"unsupported","health":"not_checked"})
            };
            application
        })
        .collect();

    let docker_networks: Vec<_> = networks
        .iter()
        .filter(|network| network["driver"] == "docker-bridge")
        .cloned()
        .collect();
    let has_docker_networks = !docker_networks.is_empty();
    let observed_results = futures_util::stream::iter(docker_networks.into_iter())
        .map(|network| async move {
            let result = privileged_request_async(json!({
                "action":"app-network-operation",
                "operation":"inspect",
                "network":network["docker_network"],
                "network_id":network["id"],
            }))
            .await;
            (
                network["id"].as_str().unwrap_or_default().to_owned(),
                result,
            )
        })
        .buffer_unordered(8)
        .collect::<Vec<_>>()
        .await;
    let mut observed = HashMap::<String, Value>::new();
    for (network_id, result) in observed_results {
        let value = match result {
            Ok(value) if value["ok"] == true => value["data"].clone(),
            Ok(value) => {
                json!({"state":"unavailable","error":value["error"].as_str().unwrap_or("بررسی شبکهٔ Docker ناموفق بود.")} )
            }
            Err(_) => {
                json!({"state":"unavailable","error":"Helper برای خواندن وضعیت واقعی Docker در دسترس نیست."})
            }
        };
        observed.insert(network_id, value);
    }
    let networks: Vec<Value> = networks.into_iter().map(|mut network| {
        let id = network["id"].as_str().unwrap_or_default();
        if network["driver"] == "docker-bridge" {
            network["observed"] = observed.remove(id).unwrap_or_else(|| json!({"state":"unavailable"}));
        } else {
            network["observed"] = json!({"state":"metadata_only","note":"Loopback metadata does not create a network namespace or isolation."});
        }
        network
    }).collect();
    let members: Vec<Value> = members
        .into_iter()
        .map(|mut member| {
            let network_id = member["network_id"].as_str().unwrap_or_default();
            let network = networks.iter().find(|network| network["id"] == network_id);
            let driver = network
                .and_then(|item| item["driver"].as_str())
                .unwrap_or_default();
            let live = network.and_then(|item| item["observed"]["containers"].as_array());
            let app_id = member["application_id"].as_str().unwrap_or_default();
            let container_name = member["container_name"].as_str().unwrap_or_default();
            let state = if driver == "host-local" {
                "metadata_only"
            } else if network.is_some_and(|item| item["observed"]["state"] == "available") {
                if live.is_some_and(|items| {
                    items.iter().any(|container| {
                        container["application_id"] == app_id && container["name"] == container_name
                    })
                }) {
                    "connected"
                } else {
                    "not_connected"
                }
            } else {
                "unknown"
            };
            member["observed_state"] = json!(state);
            member
        })
        .collect();
    Ok(Json(json!({
        "server_uuid":owner,
        "applications":applications,
        "networks":networks,
        "members":members,
        "capabilities":{"docker_live_membership":has_docker_networks,"cross_node_overlay":false,"postgres_management":false,"laravel_database_metadata":true},
        "limits":{"applications":500,"networks":128,"members":2000},
        "notes":["Docker private bridge یک شبکهٔ داخلی واقعی روی همین VPS است؛ این endpoint بین Nodeها تونل نمی‌سازد.","عضویت Docker را از inspect واقعی daemon می‌خوانیم؛ host-local فقط metadata است.","اطلاعات دیتابیس فقط برای Laravel از فایل .env خوانده می‌شود؛ رمز و connection URL هرگز برگردانده نمی‌شوند و اتصال DB تست نمی‌شود."]
    })))
}

pub(super) async fn list_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let mut stmt=db.prepare("SELECT m.application_id,a.name,m.container_name,m.alias,m.internal_port,m.created_at FROM application_network_members m JOIN application_networks n ON n.id=m.network_id JOIN applications a ON a.id=m.application_id WHERE m.network_id=?1 AND n.owner_server_uuid=?2 ORDER BY a.name COLLATE NOCASE").map_err(db_error)?;
    let members=stmt.query_map(params![id,owner],|r|Ok(json!({"application_id":r.get::<_,String>(0)?,"application_name":r.get::<_,String>(1)?,"container_name":r.get::<_,String>(2)?,"alias":r.get::<_,String>(3)?,"internal_port":r.get::<_,Option<i64>>(4)?,"created_at":r.get::<_,i64>(5)?}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
    Ok(Json(json!({"members":members})))
}

pub(super) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<NetworkInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_slug(input.name.trim())
        || !["docker-bridge", "host-local"].contains(&input.driver.as_str())
        || !["private", "shared"].contains(&input.visibility.as_str())
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام یا تنظیم شبکه معتبر نیست.",
        ));
    }
    let (actor, owner, id) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let duplicate: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM application_networks WHERE owner_server_uuid=?1 AND name=?2 COLLATE NOCASE)",
                params![owner,input.name.trim()],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        if duplicate {
            return Err(error(
                StatusCode::CONFLICT,
                "نام شبکه در این سرور تکراری است.",
            ));
        }
        (actor, owner, apps::random_uuid())
    };
    let docker_name = if input.driver == "docker-bridge" {
        Some(format!(
            "arg-appnet-{}",
            id.replace('-', "").chars().take(12).collect::<String>()
        ))
    } else {
        None
    };
    if let Some(name) = docker_name.as_deref() {
        let result=privileged_request_async(json!({"action":"app-network-operation","operation":"create","network":name,"network_id":id})).await?;
        if result["ok"] != true {
            return Err(error(
                StatusCode::BAD_GATEWAY,
                result["error"]
                    .as_str()
                    .unwrap_or("ساخت شبکه Docker انجام نشد."),
            ));
        }
    }
    let inserted = {
        let db = state.lock().unwrap();
        if db.execute("INSERT INTO application_networks(id,owner_server_uuid,name,driver,visibility,docker_network,created_by,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8)",params![id,owner,input.name.trim(),input.driver,input.visibility,docker_name,actor,now()]).is_ok() {
            audit(
                &db,
                &actor,
                "app-network.create",
                &format!("id={id}; driver={}", input.driver),
            );
            true
        } else {
            false
        }
    };
    if !inserted {
        if let Some(name) = docker_name.as_deref() {
            let _=privileged_request_async(json!({"action":"app-network-operation","operation":"remove","network":name,"network_id":id})).await;
        }
        return Err(error(
            StatusCode::CONFLICT,
            "نام شبکه تکراری است یا ثبت آن انجام نشد.",
        ));
    }
    Ok(Json(
        json!({"id":id,"name":input.name.trim(),"driver":input.driver,"visibility":input.visibility,"docker_network":docker_name}),
    ))
}

pub(super) async fn attach(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<MemberInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_slug(&input.alias) || input.internal_port.is_some_and(|port| port == 0) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Alias یا پورت داخلی معتبر نیست.",
        ));
    }
    let (actor, driver, docker_name, app_info): (
        String,
        String,
        Option<String>,
        (String, String, Option<i64>),
    ) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let net=db.query_row("SELECT driver,docker_network FROM application_networks WHERE id=?1 AND owner_server_uuid=?2",params![id,owner],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?))).optional().map_err(db_error)?.ok_or_else(||error(StatusCode::NOT_FOUND,"شبکه پیدا نشد."))?;
        let app_info=db.query_row("SELECT source_type,framework,listen_port FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",params![input.application_id,owner],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<i64>>(2)?))).optional().map_err(db_error)?.ok_or_else(||error(StatusCode::NOT_FOUND,"اپلیکیشن پیدا نشد."))?;
        (actor, net.0, net.1, app_info)
    };
    let container = if driver == "docker-bridge" {
        if app_info.0 != "docker" || input.container_name.is_empty() {
            return Err(error(StatusCode::CONFLICT,"شبکه Docker فقط به کانتینری متصل می‌شود که متعلق‌بودنش به همین اپ با برچسب ARG تأیید شود."));
        }
        let result=privileged_request_async(json!({"action":"app-network-operation","operation":"connect","network":docker_name,"network_id":id,"container":input.container_name,"application_id":input.application_id,"alias":input.alias})).await?;
        if result["ok"] != true {
            return Err(error(
                StatusCode::BAD_GATEWAY,
                result["error"]
                    .as_str()
                    .unwrap_or("اتصال کانتینر به شبکه ناموفق بود."),
            ));
        }
        input.container_name.clone()
    } else {
        if !input.container_name.is_empty()
            || !apps::is_node_runtime_framework(&app_info.1)
            || input.internal_port.map(i64::from) != app_info.2
        {
            return Err(error(StatusCode::BAD_REQUEST,"برای Node/SSR باید پورت ثبت‌شدهٔ خود اپ را استفاده کنی؛ loopback جداسازی شبکه ایجاد نمی‌کند."));
        }
        String::new()
    };
    let inserted = {
        let db = state.lock().unwrap();
        if db.execute("INSERT INTO application_network_members(network_id,application_id,container_name,alias,internal_port,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![id,input.application_id,container,input.alias,input.internal_port.map(i64::from),now()]).is_ok() {
            audit(
                &db,
                &actor,
                "app-network.attach",
                &format!(
                    "network={id}; app={}; alias={}",
                    input.application_id, input.alias
                ),
            );
            true
        } else {
            false
        }
    };
    if !inserted {
        if driver == "docker-bridge" {
            let _=privileged_request_async(json!({"action":"app-network-operation","operation":"disconnect","network":docker_name,"network_id":id,"container":input.container_name,"application_id":input.application_id,"alias":input.alias})).await;
        }
        return Err(error(
            StatusCode::CONFLICT,
            "اپلیکیشن یا alias از قبل عضو این شبکه است.",
        ));
    }
    Ok(Json(
        json!({"attached":true,"alias":input.alias,"endpoint":if driver=="host-local" {input.internal_port.map(|port|format!("127.0.0.1:{port}"))} else {None},"isolation":driver=="docker-bridge"}),
    ))
}

pub(super) async fn detach(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, app_id)): Path<(String, String)>,
) -> ApiResult {
    check_origin(&headers)?;
    let (actor, driver, network, container): (String, String, Option<String>, String) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let (driver,network):(String,Option<String>)=db.query_row("SELECT driver,docker_network FROM application_networks WHERE id=?1 AND owner_server_uuid=?2",params![id,owner],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?.ok_or_else(||error(StatusCode::NOT_FOUND,"شبکه پیدا نشد."))?;
        let container=db.query_row("SELECT container_name FROM application_network_members WHERE network_id=?1 AND application_id=?2",params![id,app_id],|r|r.get::<_,String>(0)).optional().map_err(db_error)?.ok_or_else(||error(StatusCode::NOT_FOUND,"عضویت پیدا نشد."))?;
        (actor, driver, network, container)
    };
    if driver == "docker-bridge" {
        let result=privileged_request_async(json!({"action":"app-network-operation","operation":"disconnect","network":network,"network_id":id,"container":container,"application_id":app_id})).await?;
        if result["ok"] != true {
            return Err(error(
                StatusCode::BAD_GATEWAY,
                result["error"]
                    .as_str()
                    .unwrap_or("جداسازی کانتینر ناموفق بود."),
            ));
        }
    }
    let db = state.lock().unwrap();
    db.execute(
        "DELETE FROM application_network_members WHERE network_id=?1 AND application_id=?2",
        params![id, app_id],
    )
    .map_err(db_error)?;
    audit(
        &db,
        &actor,
        "app-network.detach",
        &format!("network={id}; app={app_id}"),
    );
    Ok(Json(json!({"detached":true})))
}

pub(super) async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult {
    check_origin(&headers)?;
    let confirmed = query.get("confirm").is_some_and(|v| v == "true");
    let (actor, owner, driver, docker_name, count) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let row=db.query_row("SELECT driver,docker_network,(SELECT COUNT(*) FROM application_network_members WHERE network_id=application_networks.id) FROM application_networks WHERE id=?1 AND owner_server_uuid=?2",params![id,owner],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,i64>(2)?))).optional().map_err(db_error)?.ok_or_else(||error(StatusCode::NOT_FOUND,"شبکه پیدا نشد."))?;
        (actor, owner, row.0, row.1, row.2)
    };
    if !confirmed {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "برای حذف شبکه تأیید صریح لازم است.",
        ));
    }
    if count > 0 {
        return Err(error(
            StatusCode::CONFLICT,
            "ابتدا همهٔ اعضای شبکه را جدا کن.",
        ));
    }
    if driver == "docker-bridge" {
        let result=privileged_request_async(json!({"action":"app-network-operation","operation":"remove","network":docker_name,"network_id":id})).await?;
        if result["ok"] != true {
            return Err(error(
                StatusCode::BAD_GATEWAY,
                result["error"]
                    .as_str()
                    .unwrap_or("حذف شبکه Docker ناموفق بود."),
            ));
        }
    }
    let db = state.lock().unwrap();
    db.execute(
        "DELETE FROM application_networks WHERE id=?1 AND owner_server_uuid=?2",
        params![id, owner],
    )
    .map_err(db_error)?;
    audit(&db, &actor, "app-network.remove", &format!("id={id}"));
    Ok(Json(json!({"removed":true})))
}

pub(super) fn execute_privileged_request(request: &Value) -> Value {
    let op = request["operation"].as_str().unwrap_or_default();
    let network = request["network"].as_str().unwrap_or_default();
    let id = request["network_id"].as_str().unwrap_or_default();
    if !apps::valid_uuid(id) {
        return json!({"ok":false,"error":"Network ID is invalid"});
    }
    let Some(id_short) = id.replace('-', "").get(..12).map(str::to_owned) else {
        return json!({"ok":false,"error":"Network ID is invalid"});
    };
    if network != format!("arg-appnet-{id_short}") {
        return json!({"ok":false,"error":"Network name is not ARG-managed"});
    }
    let mut cmd = std::process::Command::new("/usr/bin/docker");
    match op {
        "inspect" => {
            let mut inspect = std::process::Command::new("/usr/bin/docker");
            inspect.args(["network", "inspect", "--format", "{{json .}}", network]);
            let (success, output) = match command_result(&mut inspect) {
                Ok(result) => result,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            if !success {
                return json!({"ok":false,"error":"ARG Docker network is unavailable."});
            }
            return match docker_network_inspection_summary(&output, id) {
                Ok(data) => json!({"ok":true,"data":data}),
                Err(message) => json!({"ok":false,"error":message}),
            };
        }
        "create" => {
            cmd.args([
                "network",
                "create",
                "--driver",
                "bridge",
                "--internal",
                "--label",
                "com.arg.managed=true",
                "--label",
            ]);
            cmd.arg(format!("com.arg.network_id={id}"));
            cmd.arg(network);
        }
        "remove" => {
            let mut inspect = std::process::Command::new("/usr/bin/docker");
            inspect.args([
                "network",
                "inspect",
                "--format",
                "{{ index .Labels \"com.arg.network_id\" }}",
                network,
            ]);
            let owned = command_result(&mut inspect).is_ok_and(|(ok, out)| ok && out.trim() == id);
            if !owned {
                return json!({"ok":false,"error":"Network is not managed by ARG"});
            }
            cmd.args(["network", "rm", network]);
        }
        "connect" | "disconnect" => {
            let container = request["container"].as_str().unwrap_or_default();
            let app_id = request["application_id"].as_str().unwrap_or_default();
            let alias = request["alias"].as_str().unwrap_or_default();
            if !docker::valid_docker_object(container)
                || !apps::valid_uuid(app_id)
                || !docker::valid_docker_object(alias)
            {
                return json!({"ok":false,"error":"Container, app or alias is invalid"});
            }
            let mut network_inspect = std::process::Command::new("/usr/bin/docker");
            network_inspect.args([
                "network",
                "inspect",
                "--format",
                "{{ index .Labels \"com.arg.network_id\" }}",
                network,
            ]);
            if !command_result(&mut network_inspect).is_ok_and(|(ok, out)| ok && out.trim() == id) {
                return json!({"ok":false,"error":"Network is not managed by ARG"});
            }
            if op == "connect" {
                let mut inspect = std::process::Command::new("/usr/bin/docker");
                inspect.args([
                    "inspect",
                    "--format",
                    "{{ index .Config.Labels \"com.arg.application_id\" }}",
                    container,
                ]);
                let owned =
                    command_result(&mut inspect).is_ok_and(|(ok, out)| ok && out.trim() == app_id);
                if !owned {
                    return json!({"ok":false,"error":"Container is not labeled as belonging to this application"});
                }
                cmd.args(["network", "connect", "--alias", alias, network, container]);
            } else {
                cmd.args(["network", "disconnect", network, container]);
            }
        }
        _ => return json!({"ok":false,"error":"Network operation is not allowlisted"}),
    }
    match command_result(&mut cmd) {
        Ok((true, out)) => json!({"ok":true,"stdout":out}),
        Ok((false, out)) => json!({"ok":false,"error":out}),
        Err(err) => json!({"ok":false,"error":err}),
    }
}

fn docker_network_inspection_summary(raw: &str, expected_id: &str) -> Result<Value, &'static str> {
    let inspected: Value = serde_json::from_str(raw.trim())
        .map_err(|_| "Docker returned invalid network inspection data")?;
    let labels = inspected.get("Labels").and_then(Value::as_object);
    if labels
        .and_then(|labels| labels.get("com.arg.managed"))
        .and_then(Value::as_str)
        != Some("true")
        || labels
            .and_then(|labels| labels.get("com.arg.network_id"))
            .and_then(Value::as_str)
            != Some(expected_id)
    {
        return Err("Docker network ownership labels do not match this ARG network");
    }
    let containers =
        inspected
            .get("Containers")
            .and_then(Value::as_object)
            .map(|items| {
                items.iter().filter_map(|(id, value)| {
            Some(json!({
                "id":id,
                "name":value.get("Name")?.as_str()?,
                "ipv4":value.get("IPv4Address").and_then(Value::as_str).unwrap_or_default(),
                "ipv6":value.get("IPv6Address").and_then(Value::as_str).unwrap_or_default(),
            }))
        }).collect::<Vec<_>>()
            })
            .unwrap_or_default();
    Ok(
        json!({"state":"available","name":inspected.get("Name").and_then(Value::as_str).unwrap_or_default(),"driver":inspected.get("Driver").and_then(Value::as_str).unwrap_or_default(),"internal":inspected.get("Internal").and_then(Value::as_bool).unwrap_or(false),"containers":containers}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_names_are_bounded() {
        assert!(valid_slug("api_private-1"));
        assert!(!valid_slug("../public"));
        assert!(!valid_slug("a name"));
    }
    #[test]
    fn migration_is_additive_and_idempotent() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE applications(id TEXT PRIMARY KEY); PRAGMA foreign_keys=ON;")
            .unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        let count:i64=db.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('application_networks','application_network_members')",[],|r|r.get(0)).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn docker_inspection_requires_arg_owner_labels_and_returns_only_topology_fields() {
        let expected_id = "123e4567-e89b-12d3-a456-426614174000";
        let raw = format!(
            r#"{{"Name":"arg-appnet-123e4567e89b","Driver":"bridge","Internal":true,"Labels":{{"com.arg.managed":"true","com.arg.network_id":"{expected_id}"}},"Containers":{{"abc":{{"Name":"api","IPv4Address":"172.20.0.2/16","EndpointID":"hidden"}}}}}}"#
        );
        let summary = docker_network_inspection_summary(&raw, expected_id).unwrap();
        assert_eq!(summary["state"], "available");
        assert_eq!(summary["containers"][0]["name"], "api");
        assert!(summary["containers"][0].get("EndpointID").is_none());
        let mut unowned_json: Value = serde_json::from_str(&raw).unwrap();
        unowned_json["Labels"]["com.arg.managed"] = json!("false");
        let unowned = serde_json::to_string(&unowned_json).unwrap();
        assert!(docker_network_inspection_summary(&unowned, expected_id).is_err());
        assert!(docker_network_inspection_summary(&raw, "different-id").is_err());
    }
}
