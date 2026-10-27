use super::*;

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
    let networks = stmt.query_map([owner], |row| Ok(json!({"id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"driver":row.get::<_,String>(2)?,"visibility":row.get::<_,String>(3)?,"docker_network":row.get::<_,Option<String>>(4)?,"created_at":row.get::<_,i64>(5)?,"member_count":row.get::<_,i64>(6)?}))).map_err(db_error)?.filter_map(Result::ok).collect::<Vec<_>>();
    Ok(Json(
        json!({"networks":networks,"host_local_note":"Host-local endpoints use 127.0.0.1 and do not create process or network isolation."}),
    ))
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
    let members=stmt.query_map(params![id,owner],|r|Ok(json!({"application_id":r.get::<_,String>(0)?,"application_name":r.get::<_,String>(1)?,"container_name":r.get::<_,String>(2)?,"alias":r.get::<_,String>(3)?,"internal_port":r.get::<_,Option<i64>>(4)?,"created_at":r.get::<_,i64>(5)?}))).map_err(db_error)?.filter_map(Result::ok).collect::<Vec<_>>();
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
}
