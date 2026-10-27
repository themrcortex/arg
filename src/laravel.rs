use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    collections::BTreeMap,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path as FsPath, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};

const OUTPUT_LIMIT: usize = 64 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(20 * 60);
pub(super) static APP_EXECUTION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OperationRequest {
    action: String,
    #[serde(default)]
    confirmed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnvironmentRequest {
    variables: HashMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WebRequest {
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ServiceRequest {
    service: String,
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DatabaseProvisionRequest {
    confirmed: bool,
}

fn deployment_root() -> PathBuf {
    env::var_os("ARG_DEPLOY_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/arg/deployments"))
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS laravel_operations (
            id TEXT PRIMARY KEY,
            application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
            actor TEXT NOT NULL,
            action TEXT NOT NULL,
            state TEXT NOT NULL,
            output TEXT NOT NULL DEFAULT '',
            exit_code INTEGER,
            error_summary TEXT,
            created_at INTEGER NOT NULL,
            started_at INTEGER,
            finished_at INTEGER
         );
         CREATE INDEX IF NOT EXISTS laravel_operations_app ON laravel_operations(application_id,created_at DESC);
         CREATE UNIQUE INDEX IF NOT EXISTS laravel_one_running_operation ON laravel_operations(application_id) WHERE state IN ('queued','running');
         CREATE TABLE IF NOT EXISTS laravel_app_sites (
            application_id TEXT PRIMARY KEY REFERENCES applications(id) ON DELETE CASCADE,
            site_name TEXT NOT NULL UNIQUE,
            domain TEXT NOT NULL,
            php_fpm_socket TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
         );
         UPDATE laravel_operations SET state='interrupted',error_summary='پنل هنگام اجرای عملیات دوباره راه‌اندازی شد.' WHERE state IN ('queued','running');",
    )?;
    let has_enabled: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('laravel_app_sites') WHERE name='enabled')",
        [],
        |row| row.get(0),
    )?;
    if !has_enabled {
        db.execute(
            "ALTER TABLE laravel_app_sites ADD COLUMN enabled INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    Ok(())
}

pub(super) async fn web_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let (site, runtime_version) = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let (_, runtime_version) = app_framework(&db, &app_id, &owner)?;
        let site = db.query_row(
            "SELECT site_name,domain,php_fpm_socket,enabled FROM laravel_app_sites WHERE application_id=?1",
            [&app_id],
            |row| Ok(json!({"site_name":row.get::<_,String>(0)?,"domain":row.get::<_,String>(1)?,"php_fpm_socket":row.get::<_,String>(2)?,"enabled":row.get::<_,bool>(3)?})),
        ).optional().map_err(db_error)?;
        (site, runtime_version)
    };
    let observed = privileged_laravel_request(
        json!({"action":"laravel-web-status","app_id":app_id,"runtime_version":runtime_version}),
    )
    .await?;
    let observed = if observed["ok"] == true {
        observed["data"].clone()
    } else {
        json!({})
    };
    let mut site_value = site.unwrap_or(Value::Null);
    if let Some(site) = site_value.as_object_mut() {
        site.insert("enabled".into(), observed["enabled"].clone());
        site.insert("fpm_state".into(), observed["fpm_state"].clone());
    }
    Ok(Json(
        json!({"configured":observed["configured"].as_bool().unwrap_or(false),"site":site_value,"runtime_version":runtime_version,"services":observed["services"].clone()}),
    ))
}

pub(super) async fn configure_web(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<WebRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let _guard = APP_EXECUTION_LOCK.lock().await;
    let (actor, domain, runtime_version) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let (framework, runtime) = app_framework(&db, &app_id, &owner)?;
        if framework != "laravel" {
            return Err(error(StatusCode::CONFLICT, "قالب اپلیکیشن Laravel نیست."));
        }
        if operation_in_progress(&db, &app_id) || deployment_in_progress(&db, &app_id) {
            return Err(error(
                StatusCode::CONFLICT,
                "تا پایان عملیات جاری یا استقرار، تنظیم وب قابل تغییر نیست.",
            ));
        }
        let domain: Option<String> = db.query_row("SELECT domain FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL", params![app_id, owner], |row| row.get(0)).optional().map_err(db_error)?.flatten();
        let domain = domain
            .ok_or_else(|| error(StatusCode::CONFLICT, "برای اپلیکیشن دامنه ثبت نشده است."))?;
        if !valid_host(&domain) {
            return Err(error(StatusCode::BAD_REQUEST, "دامنهٔ اپلیکیشن معتبر نیست."));
        }
        let release = active_release(&app_id)?;
        let inspection = inspect_release(&release, &app_id, &runtime);
        if inspection["files"]["public_index"] != true
            || inspection["files"]["vendor_autoload"] != true
        {
            return Err(error(StatusCode::CONFLICT, "برای راه‌اندازی وب، public/index.php و vendor/autoload.php باید در release فعال موجود باشند."));
        }
        (actor, domain, runtime)
    };
    let runtime_version = if input.enabled {
        resolve_php_fpm_version(&runtime_version)
            .map_err(|message| error(StatusCode::CONFLICT, &message))?
    } else {
        runtime_version
    };
    let response = privileged_laravel_request(json!({"action":"laravel-web","app_id":app_id,"domain":domain,"runtime_version":runtime_version,"enabled":input.enabled})).await?;
    if response["ok"] != true {
        return Err(error(
            StatusCode::BAD_REQUEST,
            response["error"]
                .as_str()
                .unwrap_or("راه‌اندازی وب Laravel ناموفق بود."),
        ));
    }
    if input.enabled {
        if let Err(problem) = nginx::prepare_website_firewall(&state).await {
            let _ = privileged_laravel_request(json!({"action":"laravel-web","app_id":app_id,"domain":domain,"runtime_version":runtime_version,"enabled":false})).await;
            return Err(problem);
        }
    }
    let result = response["data"].clone();
    let site_name = result["site_name"].as_str().unwrap_or_default();
    let content = result["nginx_content"].as_str().unwrap_or_default();
    let socket = result["php_fpm_socket"].as_str().unwrap_or_default();
    let db = state.lock().unwrap();
    if input.enabled && !site_name.is_empty() && !content.is_empty() {
        db.execute("INSERT INTO nginx_site_metadata(name,description) VALUES (?1,?2) ON CONFLICT(name) DO NOTHING", params![site_name, format!("Laravel app {app_id}")]).map_err(db_error)?;
        nginx::store_nginx_snapshot(&db, site_name, content, None).map_err(db_error)?;
        db.execute("INSERT INTO laravel_app_sites(application_id,site_name,domain,php_fpm_socket,enabled,created_at,updated_at) VALUES (?1,?2,?3,?4,1,?5,?5) ON CONFLICT(application_id) DO UPDATE SET site_name=excluded.site_name,domain=excluded.domain,php_fpm_socket=excluded.php_fpm_socket,enabled=1,updated_at=excluded.updated_at", params![app_id,site_name,domain,socket,now()]).map_err(db_error)?;
    } else {
        db.execute(
            "UPDATE laravel_app_sites SET enabled=0,updated_at=?1 WHERE application_id=?2",
            params![now(), app_id],
        )
        .map_err(db_error)?;
    }
    audit(
        &db,
        &actor,
        if input.enabled {
            "laravel.web.enable"
        } else {
            "laravel.web.disable"
        },
        &format!("app={app_id}; domain={domain}; site={site_name}"),
    );
    Ok(Json(result))
}

async fn configure_web_handler(
    app_id: &str,
    runtime_version: &str,
    domain: &str,
    handler: &str,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let response = privileged_laravel_request(json!({
        "action":"laravel-web", "app_id":app_id, "domain":domain,
        "runtime_version":runtime_version, "enabled":true, "web_handler":handler
    })).await?;
    if response["ok"] != true {
        return Err(error(StatusCode::BAD_GATEWAY, response["error"].as_str().unwrap_or("تغییر اتصال وب Laravel ناموفق بود.")));
    }
    Ok(response["data"].clone())
}

pub(super) async fn configure_service(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<ServiceRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let _guard = APP_EXECUTION_LOCK.lock().await;
    if !["queue", "scheduler", "horizon", "reverb", "octane"].contains(&input.service.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "نوع سرویس Laravel پشتیبانی نمی‌شود."));
    }
    let (actor, runtime_version, domain, web_enabled) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let (framework, runtime) = app_framework(&db, &app_id, &owner)?;
        if framework != "laravel" {
            return Err(error(StatusCode::CONFLICT, "قالب اپلیکیشن Laravel نیست."));
        }
        if operation_in_progress(&db, &app_id) || deployment_in_progress(&db, &app_id) {
            return Err(error(
                StatusCode::CONFLICT,
                "هنگام عملیات Laravel یا استقرار، سرویس تغییر نمی‌کند.",
            ));
        }
        let _ = active_release(&app_id)?;
        let domain: Option<String> = db.query_row(
            "SELECT domain FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
            params![app_id, owner],
            |row| row.get(0),
        ).optional().map_err(db_error)?.flatten();
        let web_enabled = db.query_row(
            "SELECT enabled FROM laravel_app_sites WHERE application_id=?1",
            [&app_id],
            |row| row.get::<_, bool>(0),
        ).optional().map_err(db_error)?.unwrap_or(false);
        (actor, runtime, domain, web_enabled)
    };
    let mut web_update = None;
    if input.service == "octane" && input.enabled {
        let started = privileged_laravel_request(json!({"action":"laravel-service","app_id":app_id,"runtime_version":runtime_version,"service":"octane","enabled":true})).await?;
        if started["ok"] != true {
            return Err(error(StatusCode::BAD_GATEWAY, started["error"].as_str().unwrap_or("راه‌اندازی Octane ناموفق بود.")));
        }
        if web_enabled {
            let domain = domain.as_deref().ok_or_else(|| error(StatusCode::CONFLICT, "دامنهٔ سایت Laravel ثبت نشده است."))?;
            match configure_web_handler(&app_id, &runtime_version, domain, "octane").await {
                Ok(result) => web_update = Some(result),
                Err(problem) => {
                    let _ = privileged_laravel_request(json!({"action":"laravel-service","app_id":app_id,"runtime_version":runtime_version,"service":"octane","enabled":false})).await;
                    return Err(problem);
                }
            }
        }
    } else {
        if input.service == "octane" && !input.enabled && web_enabled {
            let domain = domain.as_deref().ok_or_else(|| error(StatusCode::CONFLICT, "دامنهٔ سایت Laravel ثبت نشده است."))?;
            web_update = Some(configure_web_handler(&app_id, &runtime_version, domain, "fpm").await?);
        }
    }
    let response = if input.service == "octane" && input.enabled {
        json!({"ok":true,"data":{"service":"octane","enabled":true,"state":"active","message":"Octane فعال شد و ترافیک سایت به سرویس loopback منتقل شد."}})
    } else {
        privileged_laravel_request(json!({"action":"laravel-service","app_id":app_id,"runtime_version":runtime_version,"service":input.service,"enabled":input.enabled})).await?
    };
    if response["ok"] != true {
        if input.service == "octane" && !input.enabled && web_enabled {
            if let Some(domain) = domain.as_deref() {
                let _ = configure_web_handler(&app_id, &runtime_version, domain, "octane").await;
            }
        }
        return Err(error(
            StatusCode::BAD_GATEWAY,
            response["error"]
                .as_str()
                .unwrap_or("تغییر سرویس Laravel ناموفق بود."),
        ));
    }
    let result = response["data"].clone();
    let db = state.lock().unwrap();
    if let Some(web) = web_update {
        let site_name = web["site_name"].as_str().unwrap_or_default();
        let content = web["nginx_content"].as_str().unwrap_or_default();
        if !site_name.is_empty() && !content.is_empty() {
            db.execute("INSERT INTO nginx_site_metadata(name,description) VALUES (?1,?2) ON CONFLICT(name) DO NOTHING", params![site_name, format!("Laravel app {app_id}")]).map_err(db_error)?;
            nginx::store_nginx_snapshot(&db, site_name, content, None).map_err(db_error)?;
        }
    }
    audit(
        &db,
        &actor,
        "laravel.service.configure",
        &format!(
            "app={app_id}; service={}; enabled={}",
            input.service, input.enabled
        ),
    );
    Ok(Json(result))
}

pub(super) async fn provision_database(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<DatabaseProvisionRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if !input.confirmed {
        return Err(error(StatusCode::BAD_REQUEST, "برای ساخت دیتابیس و کاربر محدود، تأیید صریح لازم است."));
    }
    let _guard = APP_EXECUTION_LOCK.lock().await;
    let (actor, environment) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let (framework, _) = app_framework(&db, &app_id, &owner)?;
        if framework != "laravel" { return Err(error(StatusCode::CONFLICT, "قالب اپلیکیشن Laravel نیست.")); }
        if operation_in_progress(&db, &app_id) || deployment_in_progress(&db, &app_id) {
            return Err(error(StatusCode::CONFLICT, "هنگام عملیات Laravel یا استقرار، دیتابیس قابل تغییر نیست."));
        }
        let release = active_release(&app_id)?;
        if !release.join("artisan").is_file() { return Err(error(StatusCode::CONFLICT, "release فعال ساختار Laravel ندارد.")); }
        let environment = environment_path(&app_id)?;
        let values = read_environment(&environment);
        if ["DB_DATABASE", "DB_USERNAME", "DB_PASSWORD"].iter().any(|key| values.get(*key).is_some_and(|value| !value.is_empty())) {
            return Err(error(StatusCode::CONFLICT, "تنظیم دیتابیس از قبل وجود دارد؛ برای جلوگیری از جایگزینی اتصال فعلی، دیتابیس جدید خودکار ساخته نمی‌شود."));
        }
        (actor, environment)
    };
    let (database, username, password) = mysql::provision_laravel_database(&state, &headers, &app_id).await?;
    let mut values = read_environment(&environment);
    values.insert("DB_CONNECTION".into(), "mysql".into());
    values.insert("DB_HOST".into(), "127.0.0.1".into());
    values.insert("DB_PORT".into(), "3306".into());
    values.insert("DB_DATABASE".into(), database.clone());
    values.insert("DB_USERNAME".into(), username.clone());
    values.insert("DB_PASSWORD".into(), password);
    if let Err(save_error) = save_environment_file(&environment, &values) {
        let rollback = mysql::rollback_laravel_database(&state, &headers, &app_id).await;
        let message = if rollback.is_ok() {
            format!("فایل محیط ذخیره نشد و دیتابیس/کاربر تازه rollback شدند: {save_error}")
        } else {
            format!("فایل محیط ذخیره نشد ({save_error}) و rollback دیتابیس هم ناموفق بود؛ دیتابیس {} را از صفحهٔ MySQL بررسی کن.", database)
        };
        return Err(error(StatusCode::INTERNAL_SERVER_ERROR, &message));
    }
    let db = state.lock().unwrap();
    audit(&db, &actor, "laravel.database.provision", &format!("app={app_id}; database={database}; user={username}; password stored server-side"));
    Ok(Json(json!({"created":true,"database":database,"username":username,"password_returned":false,"message":"دیتابیس و کاربر با دسترسی محدود به همین دیتابیس ساخته شدند؛ رمز تصادفی فقط در فایل خصوصی .env ذخیره شد و به مرورگر ارسال نشد."})))
}

fn laravel_service_status(app_id: &str) -> Value {
    if !valid_app_id(app_id) {
        return json!({"queue":"unknown","scheduler":"unknown","horizon":"unknown","reverb":"unknown","octane":"unknown"});
    }
    let unit_prefix = format!("arg-app-{}", &app_id[..8]);
    let release = deployment_root().join("apps").join(app_id).join("current").canonicalize().ok();
    let state = |service: &str| {
        let current_state = systemd_unit_state(&format!("{unit_prefix}-{service}.service"));
        if current_state == "active" {
            current_state
        } else if release.as_deref().is_none_or(|path| !laravel_service_available(service, path)) {
            "unsupported".to_owned()
        } else {
            current_state
        }
    };
    json!({
        "queue":state("queue"),"scheduler":state("scheduler"),
        "horizon":state("horizon"),"reverb":state("reverb"),"octane":state("octane"),
        "reverb_port":laravel_service_port(app_id,"reverb"),"octane_port":laravel_service_port(app_id,"octane")
    })
}

fn systemd_unit_state(unit: &str) -> String {
    std::process::Command::new("systemctl")
        .args(["is-active", unit])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "inactive".into())
}

fn valid_host(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 || host.starts_with("*.") {
        return false;
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.as_bytes()[0].is_ascii_alphanumeric()
            && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn helper_output(program: &str, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("اجرای {program} ناموفق بود: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

fn valid_runtime_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 32
        && version.contains('.')
        && version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
}

fn valid_managed_path(path: &FsPath) -> bool {
    path.is_absolute()
        && path
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-".contains(byte))
        && path.components().all(|component| {
            matches!(
                component,
                std::path::Component::RootDir | std::path::Component::Normal(_)
            )
        })
}

fn app_site_name(app_id: &str) -> String {
    format!("arg-app-{}", &app_id[..8])
}

fn nginx_domain_conflict(site_name: &str, domain: &str) -> Result<(), String> {
    let response = nginx::execute_privileged_request("nginx-list", &json!({}));
    if response["ok"] != true {
        return Err(response["error"].as_str().unwrap_or("فهرست Nginx برای بررسی تداخل دامنه در دسترس نیست.").to_owned());
    }
    for site in response["data"].as_array().into_iter().flatten() {
        if site["name"].as_str() == Some(site_name) { continue; }
        for pattern in site["server_names"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            if host_pattern_matches(pattern, domain) {
                return Err(format!("دامنهٔ {domain} از قبل در سایت Nginx دیگری ({}) ثبت شده است.", site["name"].as_str().unwrap_or("unknown")));
            }
        }
    }
    Ok(())
}

fn host_pattern_matches(pattern: &str, domain: &str) -> bool {
    let pattern = pattern.trim_start_matches('=');
    pattern.eq_ignore_ascii_case(domain)
        || pattern
            .strip_prefix("*.")
            .is_some_and(|suffix| domain.ends_with(&format!(".{suffix}")) && domain != suffix)
        || pattern
            .strip_prefix('.')
            .is_some_and(|suffix| domain == suffix || domain.ends_with(&format!(".{suffix}")))
}

fn php_fpm_paths(version: &str, app_id: &str) -> Result<(PathBuf, PathBuf, String), String> {
    if !valid_runtime_version(version) || !valid_app_id(app_id) {
        return Err("نسخهٔ PHP یا شناسهٔ اپلیکیشن معتبر نیست.".into());
    }
    let directory = PathBuf::from(format!("/etc/php/{version}/fpm/pool.d"));
    if !directory.is_dir()
        || fs::symlink_metadata(&directory)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(true)
    {
        return Err(format!(
            "PHP-FPM {version} نصب نیست یا پوشهٔ pool.d آن پیدا نشد."
        ));
    }
    let short = &app_id[..8];
    Ok((
        directory.join(format!("arg-app-{short}.conf")),
        PathBuf::from(format!("/run/php/arg-app-{short}.sock")),
        format!("php{version}-fpm.service"),
    ))
}

fn installed_php_fpm_versions() -> Vec<String> {
    let Ok(entries) = fs::read_dir("/etc/php") else {
        return Vec::new();
    };
    let mut versions: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let pool = entry.path().join("fpm/pool.d");
            (valid_runtime_version(&name)
                && pool.is_dir()
                && fs::symlink_metadata(&pool)
                    .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink()))
            .then_some(name)
        })
        .collect();
    versions.sort_by(|left, right| php_version_key(left).cmp(&php_version_key(right)));
    versions
}

fn php_version_key(version: &str) -> Vec<u32> {
    version
        .split('.')
        .filter_map(|part| part.parse::<u32>().ok())
        .collect()
}

fn resolve_php_fpm_version(version: &str) -> Result<String, String> {
    let installed = installed_php_fpm_versions();
    if version != "system" {
        if !valid_runtime_version(version) || !installed.iter().any(|candidate| candidate == version) {
            return Err(format!("PHP-FPM {version} نصب نیست؛ نسخهٔ Runtime سازگار را نصب یا انتخاب کن."));
        }
        return Ok(version.to_owned());
    }
    let cli_version = std::process::Command::new("/usr/bin/php")
        .args(["-r", "echo PHP_MAJOR_VERSION.'.'.PHP_MINOR_VERSION;"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    if let Some(cli_version) = cli_version {
        if installed.iter().any(|candidate| candidate == &cli_version) {
            return Ok(cli_version);
        }
    }
    installed
        .last()
        .cloned()
        .ok_or_else(|| "هیچ نسخهٔ PHP-FPM نصب‌شده‌ای پیدا نشد؛ PHP-FPM را از مدیریت Runtime نصب کن.".into())
}

fn php_fpm_pool_exists(app_id: &str, preferred_version: &str) -> bool {
    if preferred_version != "system" {
        return php_fpm_paths(preferred_version, app_id)
            .is_ok_and(|(pool, _, _)| pool.is_file());
    }
    installed_php_fpm_versions().iter().any(|version| {
        php_fpm_paths(version, app_id).is_ok_and(|(pool, _, _)| pool.is_file())
    })
}

fn app_nginx_config(app_id: &str, domain: &str, socket: &str, octane_port: Option<u16>) -> String {
    let root = deployment_root()
        .join("apps")
        .join(app_id)
        .join("current/public");
    if let Some(port) = octane_port {
        return format!(
            "# ARG managed Laravel site app={app_id}\nserver {{\n    listen 80;\n    server_name {domain};\n    root {};\n    index index.html;\n    charset utf-8;\n    server_tokens off;\n    client_max_body_size 32m;\n\n    location / {{\n        proxy_pass http://127.0.0.1:{port};\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n        proxy_set_header X-Forwarded-Proto $scheme;\n        proxy_read_timeout 120s;\n        proxy_send_timeout 120s;\n    }}\n\n    location ~* \\.(?:php|phtml|phar)(?:/|$) {{ return 404; }}\n    location ~ /\\.(?!well-known(?:/|$)) {{ deny all; }}\n    location ~* \\.(?:env|ini|log|sql|sqlite|bak|zip|gz|tar)$ {{ deny all; }}\n}}\n",
            root.display()
        );
    }
    format!(
        "# ARG managed Laravel site app={app_id}\nserver {{\n    listen 80;\n    server_name {domain};\n    root {};\n    index index.php;\n    charset utf-8;\n    server_tokens off;\n    client_max_body_size 32m;\n\n    location / {{\n        try_files $uri $uri/ /index.php?$query_string;\n    }}\n\n    location = /index.php {{\n        include fastcgi_params;\n        fastcgi_param SCRIPT_FILENAME $realpath_root/index.php;\n        fastcgi_param DOCUMENT_ROOT $realpath_root;\n        fastcgi_pass unix:{socket};\n        fastcgi_hide_header X-Powered-By;\n    }}\n\n    location ~* \\.(?:php|phtml|phar)(?:/|$) {{ return 404; }}\n    location ~ /\\.(?!well-known(?:/|$)) {{ deny all; }}\n    location ~* \\.(?:env|ini|log|sql|sqlite|bak|zip|gz|tar)$ {{ deny all; }}\n}}\n",
        root.display()
    )
}

fn php_pool_config(app_id: &str, socket: &str) -> String {
    let root = deployment_root().join("apps").join(app_id);
    format!(
        "[arg-app-{}]\nuser = www-data\ngroup = www-data\nlisten = {socket}\nlisten.owner = www-data\nlisten.group = www-data\nlisten.mode = 0660\npm = ondemand\npm.max_children = 8\npm.process_idle_timeout = 15s\npm.max_requests = 500\nclear_env = yes\nchdir = /\nphp_admin_value[open_basedir] = {}:/tmp:/usr/share/php:/usr/lib/php\n",
        &app_id[..8],
        root.display()
    )
}

fn atomic_write(path: &FsPath, content: &[u8], mode: u32) -> Result<(), String> {
    let parent = path.parent().ok_or("مسیر فایل تنظیمات معتبر نیست.")?;
    let temp = parent.join(format!(".arg-tmp-{}", random_job_id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&temp)
        .map_err(|error| format!("فایل موقت تنظیمات ساخته نشد: {error}"))?;
    use std::io::Write as _;
    let result = (|| {
        file.write_all(content).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        fs::rename(&temp, path).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn apply_web_read_permissions(app_id: &str, release: &FsPath) -> Result<(), String> {
    if std::process::Command::new("setfacl")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_err()
    {
        return Err("ابزار ACL نصب نیست. بستهٔ acl را نصب کن تا Nginx و PHP-FPM فقط به فایل‌های لازم این اپ دسترسی بگیرند.".into());
    }
    let app_root = deployment_root().join("apps").join(app_id);
    let mut parents = vec![
        PathBuf::from("/var/lib/arg"),
        deployment_root(),
        deployment_root().join("apps"),
        app_root.clone(),
        app_root.join("releases"),
        app_root.join("shared"),
        app_root.join("shared/storage"),
        release.to_path_buf(),
    ];
    parents.dedup();
    for path in parents {
        if !path.is_dir() {
            return Err("مسیر release برای تنظیم دسترسی Nginx یک پوشهٔ معتبر نیست.".into());
        }
        helper_output(
            "setfacl",
            &[
                "-m",
                "u:www-data:--x",
                path.to_str().ok_or("مسیر فایل معتبر نیست.")?,
            ],
        )
        .map_err(|error| format!("دسترسی مسیر release برای Nginx تنظیم نشد: {error}"))?;
    }
    helper_output(
        "setfacl",
        &[
            "-P",
            "-R",
            "-m",
            "u:www-data:r-X",
            release.to_str().ok_or("مسیر release معتبر نیست.")?,
        ],
    )
    .map_err(|error| format!("دسترسی خواندن فایل‌های release تنظیم نشد: {error}"))?;
    let shared = app_root.join("shared");
    let environment = shared.join(".env");
    if environment.is_file() {
        helper_output(
            "setfacl",
            &[
                "-m",
                "u:www-data:r",
                environment.to_str().ok_or("مسیر فایل محیط معتبر نیست.")?,
            ],
        )
        .map_err(|error| format!("دسترسی محدود PHP-FPM به فایل محیط تنظیم نشد: {error}"))?;
    }
    let storage = shared.join("storage");
    if storage.is_dir() {
        apply_storage_acl(&storage)?;
    }
    Ok(())
}

fn collect_storage_paths(root: &FsPath, output: &mut Vec<(PathBuf, bool)>) -> Result<(), String> {
    if output.len() >= 20_000 {
        return Err("پوشهٔ storage بیش از ۲۰٬۰۰۰ مورد دارد؛ برای جلوگیری از یک اسکن ACL سنگین، دسترسی بازگشتی اعمال نشد.".into());
    }
    let metadata = fs::symlink_metadata(root).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("symlink غیرمنتظره در مسیر storage پیدا شد.".into());
    }
    let is_directory = metadata.is_dir();
    output.push((root.to_path_buf(), is_directory));
    if is_directory {
        for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let child = entry.path();
            if fs::symlink_metadata(&child)
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false)
            {
                continue;
            }
            collect_storage_paths(&child, output)?;
        }
    }
    Ok(())
}

fn apply_storage_acl(storage: &FsPath) -> Result<(), String> {
    let mut paths = Vec::new();
    collect_storage_paths(storage, &mut paths)?;
    for (path, is_directory) in paths {
        let permission = if is_directory {
            "u:www-data:rwX,d:u:www-data:rwX"
        } else {
            "u:www-data:rw"
        };
        helper_output(
            "setfacl",
            &[
                "-m",
                permission,
                path.to_str().ok_or("مسیر storage معتبر نیست.")?,
            ],
        )
        .map_err(|error| format!("دسترسی PHP-FPM به storage تنظیم نشد: {error}"))?;
    }
    Ok(())
}

fn php_fpm_test(version: &str) -> Result<(), String> {
    let program = format!("/usr/sbin/php-fpm{version}");
    let output = std::process::Command::new(&program)
        .arg("-t")
        .output()
        .or_else(|_| {
            std::process::Command::new(format!("/usr/bin/php-fpm{version}"))
                .arg("-t")
                .output()
        })
        .map_err(|error| format!("بررسی تنظیمات PHP-FPM ناموفق بود: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if detail.is_empty() {
            "آزمون تنظیمات PHP-FPM شکست خورد.".into()
        } else {
            detail
        })
    }
}

fn php_fpm_reload(unit: &str) -> Result<(), String> {
    helper_output("systemctl", &["reload-or-restart", unit]).map(|_| ())
}

fn laravel_web_helper(request: &Value) -> Result<Value, String> {
    let app_id = request["app_id"]
        .as_str()
        .ok_or("شناسهٔ اپلیکیشن ارسال نشده است.")?;
    if !valid_app_id(app_id) {
        return Err("شناسهٔ اپلیکیشن معتبر نیست.".into());
    }
    let enabled = request["enabled"]
        .as_bool()
        .ok_or("وضعیت وب ارسال نشده است.")?;
    let site_name = app_site_name(app_id);
    if !enabled {
        let disabled = nginx::execute_privileged_request(
            "nginx-toggle",
            &json!({"name":site_name,"enabled":false}),
        );
        if disabled["ok"] != true {
            return Err(disabled["error"]
                .as_str()
                .unwrap_or("سایت Nginx این اپ پیدا نشد.")
                .to_owned());
        }
        return Ok(
            json!({"configured":true,"enabled":false,"site_name":site_name,"message":"سایت Laravel در Nginx غیرفعال شد؛ فایل‌ها و releaseها حفظ شدند."}),
        );
    }
    let domain = request["domain"].as_str().ok_or("دامنه ارسال نشده است.")?;
    if !valid_host(domain) {
        return Err("دامنهٔ اپلیکیشن معتبر نیست.".into());
    }
    let version = request["runtime_version"]
        .as_str()
        .ok_or("نسخهٔ PHP ارسال نشده است.")?;
    let version = resolve_php_fpm_version(version)?;
    let (pool_path, socket_path, fpm_unit) = php_fpm_paths(&version, app_id)?;
    let requested_handler = request["web_handler"].as_str().unwrap_or("auto");
    if !["auto", "fpm", "octane"].contains(&requested_handler) {
        return Err("نوع پردازش وب Laravel معتبر نیست.".into());
    }
    let octane_unit = format!("arg-app-{}-octane.service", &app_id[..8]);
    let octane_active = systemd_unit_state(&octane_unit) == "active";
    let octane_port = match requested_handler {
        "fpm" => None,
        "octane" if octane_active => laravel_service_port(app_id, "octane"),
        "octane" => return Err("سرویس Octane باید پیش از تغییر Nginx فعال باشد.".into()),
        _ if octane_active => laravel_service_port(app_id, "octane"),
        _ => None,
    };
    if requested_handler == "octane" && octane_port.is_none() {
        return Err("پورت Octane قابل محاسبه نیست.".into());
    }
    let root = deployment_root().join("apps").join(app_id);
    if !valid_managed_path(&deployment_root()) {
        return Err(
            "مسیر استقرار سفارشی فقط می‌تواند شامل حروف لاتین، عدد، نقطه، خط تیره و اسلش باشد."
                .into(),
        );
    }
    let canonical_root = root.canonicalize().map_err(|_| "پوشهٔ اپلیکیشن پیدا نشد.")?;
    if canonical_root != root || !canonical_root.is_dir() {
        return Err("مسیر اپلیکیشن از محدودهٔ مدیریت‌شده خارج است.".into());
    }
    let releases = root
        .join("releases")
        .canonicalize()
        .map_err(|_| "پوشهٔ releaseها پیدا نشد.")?;
    if fs::symlink_metadata(root.join("releases"))
        .map(|metadata| !metadata.is_dir() || metadata.file_type().is_symlink())
        .unwrap_or(true)
    {
        return Err("پوشهٔ releaseها نباید symlink یا فایل باشد.".into());
    }
    if fs::symlink_metadata(root.join("current"))
        .map(|metadata| !metadata.file_type().is_symlink())
        .unwrap_or(true)
    {
        return Err("اشاره‌گر current باید symlink مدیریت‌شدهٔ release باشد.".into());
    }
    let release = root
        .join("current")
        .canonicalize()
        .map_err(|_| "release فعال پیدا نشد.")?;
    if !release.starts_with(&releases)
        || !release.is_dir()
        || !release.join("public/index.php").is_file()
    {
        return Err("release فعال خارج از مسیر امن است یا public/index.php ندارد.".into());
    }
    let env_file = root.join("shared/.env");
    if fs::symlink_metadata(root.join("shared"))
        .map(|metadata| !metadata.is_dir() || metadata.file_type().is_symlink())
        .unwrap_or(true)
    {
        return Err("پوشهٔ shared باید پوشهٔ معمولی در محدودهٔ اپلیکیشن باشد.".into());
    }
    if !env_file.is_file()
        || fs::symlink_metadata(&env_file)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(true)
    {
        return Err("فایل .env پایدار پیدا نشد یا symlink نامعتبر است.".into());
    }
    let socket = socket_path.to_string_lossy().into_owned();
    let content = app_nginx_config(app_id, domain, &socket, octane_port);
    let existing = nginx::execute_privileged_request("nginx-get", &json!({"name":site_name}));
    let already_exists = existing["ok"] == true;
    let update_managed = already_exists
        && existing["data"]["content"].as_str().is_some_and(|old| {
            old.lines().next() == Some(format!("# ARG managed Laravel site app={app_id}").as_str())
        });
    if already_exists
        && existing["data"]["content"].as_str() != Some(content.as_str())
        && !update_managed
    {
        return Err(
            "فایل Nginx با نام رزروشدهٔ این اپ از قبل وجود دارد و توسط ARG مدیریت نمی‌شود.".into(),
        );
    }
    nginx_domain_conflict(&site_name, domain)?;
    apply_web_read_permissions(app_id, &release)?;
    let pool = php_pool_config(app_id, &socket);
    let previous_pool = fs::read(&pool_path).ok();
    if let Some(old) = &previous_pool {
        if old != pool.as_bytes()
            && !String::from_utf8_lossy(old).starts_with(&format!("[arg-app-{}]", &app_id[..8]))
        {
            return Err(
                "فایل PHP-FPM با نام رزروشدهٔ این اپ از قبل وجود دارد و توسط ARG مدیریت نمی‌شود."
                    .into(),
            );
        }
    }
    atomic_write(&pool_path, pool.as_bytes(), 0o644)?;
    if let Err(error) = php_fpm_test(&version).and_then(|_| php_fpm_reload(&fpm_unit)) {
        match previous_pool {
            Some(old) => {
                let _ = atomic_write(&pool_path, &old, 0o644);
            }
            None => {
                let _ = fs::remove_file(&pool_path);
            }
        }
        let _ = php_fpm_test(&version);
        let _ = php_fpm_reload(&fpm_unit);
        return Err(format!(
            "تنظیم PHP-FPM اعمال نشد و فایل قبلی بازگردانده شد: {error}"
        ));
    }
    if !already_exists {
        let created = nginx::execute_privileged_request(
            "nginx-create",
            &json!({"name":site_name,"content":content}),
        );
        if created["ok"] != true {
            match previous_pool {
                Some(old) => {
                    let _ = atomic_write(&pool_path, &old, 0o644);
                }
                None => {
                    let _ = fs::remove_file(&pool_path);
                }
            }
            let _ = php_fpm_test(&version);
            let _ = php_fpm_reload(&fpm_unit);
            return Err(created["error"]
                .as_str()
                .unwrap_or("ساخت تنظیم Nginx شکست خورد.")
                .to_owned());
        }
    } else if update_managed && existing["data"]["content"].as_str() != Some(content.as_str()) {
        let updated = nginx::execute_privileged_request(
            "nginx-save",
            &json!({"name":site_name,"content":content}),
        );
        if updated["ok"] != true {
            match previous_pool {
                Some(old) => {
                    let _ = atomic_write(&pool_path, &old, 0o644);
                }
                None => {
                    let _ = fs::remove_file(&pool_path);
                }
            }
            let _ = php_fpm_test(&version);
            let _ = php_fpm_reload(&fpm_unit);
            return Err(updated["error"]
                .as_str()
                .unwrap_or("ویرایش تنظیم Nginx شکست خورد.")
                .to_owned());
        }
    }
    let enabled_site = nginx::execute_privileged_request(
        "nginx-toggle",
        &json!({"name":site_name,"enabled":true}),
    );
    if enabled_site["ok"] != true {
        if !already_exists {
            let _ = nginx::execute_privileged_request("nginx-delete", &json!({"name":site_name}));
        }
        match previous_pool {
            Some(old) => {
                let _ = atomic_write(&pool_path, &old, 0o644);
            }
            None => {
                let _ = fs::remove_file(&pool_path);
            }
        }
        let _ = php_fpm_test(&version);
        let _ = php_fpm_reload(&fpm_unit);
        return Err(enabled_site["error"]
            .as_str()
            .unwrap_or("فعال‌سازی تنظیم Nginx شکست خورد.")
            .to_owned());
    }
    Ok(
        json!({"configured":true,"enabled":true,"handler":if octane_port.is_some() {"octane"} else {"fpm"},"octane_port":octane_port,"site_name":site_name,"domain":domain,"php_fpm_socket":socket,"nginx_content":content,"message":if octane_port.is_some() {"Nginx به Octane روی loopback وصل شد."} else {"PHP-FPM و Nginx بررسی و فعال شدند. ACL فقط دسترسی لازم Nginx/PHP-FPM به release عمومی و storage این اپ را می‌دهد."}}),
    )
}

fn laravel_service_helper(request: &Value) -> Result<Value, String> {
    let app_id = request["app_id"]
        .as_str()
        .ok_or("شناسهٔ اپلیکیشن ارسال نشده است.")?;
    if !valid_app_id(app_id) {
        return Err("شناسهٔ اپلیکیشن معتبر نیست.".into());
    }
    let service = request["service"]
        .as_str()
        .ok_or("نوع سرویس ارسال نشده است.")?;
    let enabled = request["enabled"]
        .as_bool()
        .ok_or("وضعیت سرویس ارسال نشده است.")?;
    if !["queue", "scheduler", "horizon", "reverb", "octane"].contains(&service) {
        return Err("نوع سرویس مجاز نیست.".into());
    }
    let root = deployment_root().join("apps").join(app_id);
    if !valid_managed_path(&deployment_root()) {
        return Err("مسیر استقرار سفارشی برای ساخت امن سرویس systemd معتبر نیست.".into());
    }
    let short = &app_id[..8];
    let unit_name = format!("arg-app-{short}-{service}.service");
    let unit_path = PathBuf::from("/etc/systemd/system").join(&unit_name);
    let marker = format!("# ARG managed Laravel service app={app_id} kind={service}\n");
    let previous_unit = fs::read(&unit_path).ok();
    if previous_unit
        .as_deref()
        .is_some_and(|content| !content.starts_with(marker.as_bytes()))
    {
        return Err("فایل systemd با نام رزروشدهٔ سرویس از قبل وجود دارد و توسط ARG مدیریت نمی‌شود.".into());
    }
    if !enabled {
        if previous_unit.is_none() {
            return Err("فایل سرویس پیدا نشد یا توسط ARG برای همین اپ ساخته نشده است.".into());
        }
        helper_output("systemctl", &["disable", "--now", &unit_name])
            .map_err(|error| format!("توقف سرویس Laravel ناموفق بود: {error}"))?;
        fs::remove_file(&unit_path).map_err(|error| format!("فایل سرویس حذف نشد: {error}"))?;
        helper_output("systemctl", &["daemon-reload"])?;
        return Ok(json!({"service":service,"enabled":false,"unit":unit_name,"state":"inactive","port":laravel_service_port(app_id,service),"message":"سرویس متوقف و غیرفعال شد؛ لاگ‌های قبلی در journal باقی می‌مانند."}));
    }
    let version = request["runtime_version"]
        .as_str()
        .ok_or("نسخهٔ PHP ارسال نشده است.")?;
    if version != "system" && !valid_runtime_version(version) {
        return Err("نسخهٔ PHP معتبر نیست.".into());
    }
    let releases = root
        .join("releases")
        .canonicalize()
        .map_err(|_| "پوشهٔ releaseها پیدا نشد.")?;
    let current = root
        .join("current")
        .canonicalize()
        .map_err(|_| "release فعال پیدا نشد.")?;
    if !current.starts_with(&releases)
        || !current.join("artisan").is_file()
        || !current.join("vendor/autoload.php").is_file()
    {
        return Err("release فعال برای اجرای Artisan معتبر یا کامل نیست.".into());
    }
    if !laravel_service_available(service, &current) {
        return Err(format!("وابستگی لازم برای سرویس {service} در release فعال نصب نشده است."));
    }
    if service == "horizon" {
        let queue_unit = format!("arg-app-{short}-queue.service");
        if systemd_unit_state(&queue_unit) == "active" {
            return Err("Queue Worker فعال است؛ برای جلوگیری از اجرای تکراری مصرف‌کننده‌ها، ابتدا آن را متوقف کن.".into());
        }
    }
    if service == "queue" {
        let horizon_unit = format!("arg-app-{short}-horizon.service");
        if systemd_unit_state(&horizon_unit) == "active" {
            return Err("Horizon فعال است؛ Queue Worker جداگانه را هم‌زمان اجرا نکن.".into());
        }
    }
    let php = php_binary(version).map_err(|message| message)?;
    let port = laravel_service_port(app_id, service);
    let was_active = systemd_unit_state(&unit_name) == "active";
    if !was_active && port.is_some_and(|port| std::net::TcpListener::bind(("127.0.0.1", port)).is_err()) {
        return Err(format!("پورت loopback سرویس {service} در حال استفاده است؛ سرویس شروع نشد."));
    }
    let command = laravel_service_command(service, port)?;
    let unit = format!("# ARG managed Laravel service app={app_id} kind={service}\n[Unit]\nDescription=ARG Laravel {service} for app {short}\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nUser=arg\nGroup=arg\nWorkingDirectory={}\nExecStart={} artisan {command}\nRestart=on-failure\nRestartSec=5s\nKillSignal=SIGTERM\nTimeoutStopSec=120s\nNoNewPrivileges=yes\nPrivateTmp=yes\nProtectHome=read-only\nProtectSystem=strict\nReadWritePaths={} {}\nUMask=0027\n\n[Install]\nWantedBy=multi-user.target\n",current.display(),php.display(),root.join("shared/storage").display(),current.join("bootstrap/cache").display());
    atomic_write(&unit_path, unit.as_bytes(), 0o644)?;
    helper_output("systemctl", &["daemon-reload"])?;
    if let Err(error) = helper_output("systemctl", &["enable", "--now", &unit_name]) {
        restore_service_unit(&unit_path, previous_unit.as_deref(), &unit_name, was_active);
        let _ = helper_output("systemctl", &["daemon-reload"]);
        return Err(format!("سرویس Laravel راه‌اندازی نشد: {error}"));
    }
    let state =
        helper_output("systemctl", &["is-active", &unit_name]).unwrap_or_else(|_| "failed".into());
    if state != "active" {
        restore_service_unit(&unit_path, previous_unit.as_deref(), &unit_name, was_active);
        let _ = helper_output("systemctl", &["daemon-reload"]);
        return Err(format!("سرویس Laravel فعال نماند (وضعیت {state})؛ تنظیم قبلی بازیابی شد."));
    }
    Ok(
        json!({"service":service,"enabled":true,"unit":unit_name,"state":state,"port":port,"message":"سرویس systemd ساخته و فعال شد. اجرا با PHP انتخابی، کاربر محدود ARG و فرمان ثابت انجام می‌شود."}),
    )
}

fn restore_service_unit(path: &FsPath, previous: Option<&[u8]>, unit: &str, was_active: bool) {
    match previous {
        Some(content) => {
            let _ = atomic_write(path, content, 0o644);
            if was_active {
                let _ = helper_output("systemctl", &["restart", unit]);
            } else {
                let _ = helper_output("systemctl", &["disable", "--now", unit]);
            }
        }
        None => {
            let _ = helper_output("systemctl", &["disable", "--now", unit]);
            let _ = fs::remove_file(path);
        }
    }
}

fn laravel_service_available(service: &str, release: &FsPath) -> bool {
    match service {
        "queue" | "scheduler" => true,
        "horizon" => release.join("vendor/laravel/horizon/src/HorizonServiceProvider.php").is_file(),
        "reverb" => release.join("vendor/laravel/reverb/src/ReverbServiceProvider.php").is_file(),
        "octane" => release.join("vendor/laravel/octane/src/OctaneServiceProvider.php").is_file()
            && release.join("vendor/bin/rr").is_file(),
        _ => false,
    }
}

fn laravel_service_command(service: &str, port: Option<u16>) -> Result<String, String> {
    match service {
        "queue" => Ok("queue:work --sleep=3 --tries=3 --timeout=90 --no-interaction".into()),
        "scheduler" => Ok("schedule:work".into()),
        "horizon" => Ok("horizon".into()),
        "reverb" => Ok(format!("reverb:start --host=127.0.0.1 --port={}", port.ok_or("پورت Reverb معتبر نیست.")?)),
        "octane" => Ok(format!("octane:start --server=roadrunner --host=127.0.0.1 --port={} --max-requests=500", port.ok_or("پورت Octane معتبر نیست.")?)),
        _ => Err("نوع سرویس Laravel مجاز نیست.".into()),
    }
}

fn laravel_service_port(app_id: &str, service: &str) -> Option<u16> {
    if !["reverb", "octane"].contains(&service) || !valid_app_id(app_id) {
        return None;
    }
    let seed = u32::from_str_radix(&app_id[..8], 16).ok()?;
    let base = if service == "reverb" { 20_000 } else { 32_000 };
    Some(base + (seed % 10_000) as u16)
}

fn laravel_web_status_helper(request: &Value) -> Result<Value, String> {
    let app_id = request["app_id"]
        .as_str()
        .ok_or("شناسهٔ اپلیکیشن ارسال نشده است.")?;
    if !valid_app_id(app_id) {
        return Err("شناسهٔ اپلیکیشن معتبر نیست.".into());
    }
    let version = request["runtime_version"]
        .as_str()
        .filter(|value| *value == "system" || valid_runtime_version(value))
        .ok_or("نسخهٔ PHP معتبر نیست.")?;
    let site_name = app_site_name(app_id);
    let pool_exists = php_fpm_pool_exists(app_id, version);
    let sites = nginx::execute_privileged_request("nginx-list", &json!({}));
    let enabled = sites["ok"] == true
        && sites["data"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["name"].as_str() == Some(site_name.as_str()) && item["enabled"] == true
            })
        });
    Ok(
        json!({"configured":pool_exists,"enabled":enabled,"fpm_state":if pool_exists {"configured"} else {"not_configured"},"services":laravel_service_status(app_id)}),
    )
}

fn laravel_refresh_services_helper(request: &Value) -> Result<Value, String> {
    let app_id = request["app_id"]
        .as_str()
        .ok_or("شناسهٔ اپلیکیشن ارسال نشده است.")?;
    if !valid_app_id(app_id) {
        return Err("شناسهٔ اپلیکیشن معتبر نیست.".into());
    }
    let short = &app_id[..8];
    let mut restarted = Vec::new();
    for service in ["queue", "scheduler", "horizon", "reverb", "octane"] {
        let unit = format!("arg-app-{short}-{service}.service");
        let path = PathBuf::from("/etc/systemd/system").join(&unit);
        let marker = format!("# ARG managed Laravel service app={app_id} kind={service}\n");
        let owned = fs::read_to_string(&path)
            .ok()
            .is_some_and(|content| content.starts_with(&marker));
        if !owned || systemd_unit_state(&unit) != "active" {
            continue;
        }
        helper_output("systemctl", &["restart", &unit]).map_err(|error| {
            format!("Restart سرویس {service} پس از استقرار ناموفق بود: {error}")
        })?;
        restarted.push(service);
    }
    Ok(
        json!({"restarted":restarted,"message":"سرویس‌های فعال Laravel پس از جابه‌جایی release دوباره بارگذاری شدند."}),
    )
}

pub(super) fn execute_privileged_request(action: &str, request: &Value) -> Value {
    let result = match action {
        "laravel-web" => laravel_web_helper(request),
        "laravel-web-status" => laravel_web_status_helper(request),
        "laravel-service" => laravel_service_helper(request),
        "laravel-refresh-services" => laravel_refresh_services_helper(request),
        _ => Err("درخواست privileged مربوط به Laravel معتبر نیست.".into()),
    };
    match result {
        Ok(data) => json!({"ok":true,"data":data}),
        Err(error) => json!({"ok":false,"error":error}),
    }
}

async fn privileged_laravel_request(request: Value) -> Result<Value, (StatusCode, Json<Value>)> {
    privileged_request_async(request).await
}

pub(super) fn prepare_release(app_id: &str, release: &FsPath) -> Result<(), String> {
    if !valid_app_id(app_id) {
        return Err("شناسهٔ اپلیکیشن برای آماده‌سازی Laravel معتبر نیست.".into());
    }
    let app_root = deployment_root().join("apps").join(app_id);
    prepare_release_at(&app_root, release)
}

fn prepare_release_at(app_root: &FsPath, release: &FsPath) -> Result<(), String> {
    let shared = app_root.join("shared");
    fs::create_dir_all(&shared).map_err(|e| format!("پوشهٔ پایدار Laravel ساخته نشد: {e}"))?;
    if !shared.is_dir()
        || fs::symlink_metadata(&shared).is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err("مسیر shared یک پوشهٔ عادی نیست.".into());
    }
    let storage = shared.join("storage");
    let incoming_storage = release.join("storage");
    if fs::symlink_metadata(&incoming_storage)
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err("storage به‌صورت symlink در منبع Laravel مجاز نیست.".into());
    }
    if incoming_storage.exists() && !incoming_storage.is_dir() {
        return Err("storage در منبع Laravel باید یک پوشهٔ عادی باشد.".into());
    }
    if !storage.exists() {
        if incoming_storage.is_dir() {
            fs::rename(&incoming_storage, &storage)
                .map_err(|e| format!("انتقال storage به فضای پایدار ناموفق بود: {e}"))?;
        } else {
            fs::create_dir_all(&storage)
                .map_err(|e| format!("پوشهٔ storage پایدار ساخته نشد: {e}"))?;
        }
    } else if !storage.is_dir()
        || fs::symlink_metadata(&storage).is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err("مسیر storage پایدار یک پوشهٔ عادی نیست.".into());
    } else if incoming_storage.is_dir() {
        fs::remove_dir_all(&incoming_storage).map_err(|e| {
            format!("حذف storage نسخهٔ جدید برای استفاده از دادهٔ پایدار ناموفق بود: {e}")
        })?;
    }
    for directory in [
        "app",
        "framework/cache/data",
        "framework/sessions",
        "framework/views",
        "logs",
    ] {
        fs::create_dir_all(storage.join(directory))
            .map_err(|e| format!("پوشهٔ runtime/storage/{directory} آماده نشد: {e}"))?;
    }

    let shared_env = shared.join(".env");
    let incoming_env = release.join(".env");
    if fs::symlink_metadata(&incoming_env).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("فایل .env به‌صورت symlink در منبع Laravel پذیرفته نمی‌شود.".into());
    }
    if incoming_env.exists() && !incoming_env.is_file() {
        return Err(".env در منبع Laravel باید یک فایل عادی باشد.".into());
    }
    if !shared_env.exists() && incoming_env.is_file() {
        fs::rename(&incoming_env, &shared_env)
            .map_err(|e| format!("انتقال .env به فضای پایدار ناموفق بود: {e}"))?;
    }
    if !shared_env.exists() {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&shared_env)
            .map_err(|e| format!("فایل .env پایدار ساخته نشد: {e}"))?;
    }
    if !shared_env.is_file()
        || fs::symlink_metadata(&shared_env).is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err("مسیر .env پایدار یک فایل عادی نیست.".into());
    }
    fs::set_permissions(&shared_env, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("دسترسی خصوصی فایل .env ثبت نشد: {e}"))?;

    for (path, target, label) in [
        (
            incoming_storage,
            PathBuf::from("../../shared/storage"),
            "storage",
        ),
        (incoming_env, PathBuf::from("../../shared/.env"), ".env"),
    ] {
        if path.exists() || fs::symlink_metadata(&path).is_ok() {
            if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
                fs::remove_file(&path).map_err(|e| format!("پیوند قبلی {label} پاک نشد: {e}"))?;
            } else if path.is_dir() {
                fs::remove_dir_all(&path).map_err(|e| format!("مسیر {label} نسخه حذف نشد: {e}"))?;
            } else {
                fs::remove_file(&path).map_err(|e| format!("مسیر {label} نسخه حذف نشد: {e}"))?;
            }
        }
        std::os::unix::fs::symlink(target, &path)
            .map_err(|e| format!("پیوند پایدار {label} ساخته نشد: {e}"))?;
    }
    let bootstrap_cache = release.join("bootstrap/cache");
    if fs::symlink_metadata(&bootstrap_cache)
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err("bootstrap/cache نباید symlink باشد.".into());
    }
    fs::create_dir_all(&bootstrap_cache)
        .map_err(|e| format!("پوشهٔ bootstrap/cache آماده نشد: {e}"))?;
    Ok(())
}

fn valid_app_id(value: &str) -> bool {
    value.len() == 36
        && value.chars().enumerate().all(|(index, character)| {
            if [8, 13, 18, 23].contains(&index) {
                character == '-'
            } else {
                character.is_ascii_hexdigit()
            }
        })
}

fn app_root(app_id: &str) -> Option<PathBuf> {
    valid_app_id(app_id).then(|| deployment_root().join("apps").join(app_id))
}

fn active_release(app_id: &str) -> Result<PathBuf, (StatusCode, Json<Value>)> {
    let root = app_root(app_id)
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "شناسهٔ اپلیکیشن نامعتبر است."))?;
    let releases = root.join("releases").canonicalize().map_err(|_| {
        error(
            StatusCode::NOT_FOUND,
            "هنوز release استقراریافته‌ای برای این اپلیکیشن وجود ندارد.",
        )
    })?;
    let current = root.join("current").canonicalize().map_err(|_| {
        error(
            StatusCode::NOT_FOUND,
            "اشاره‌گر release جاری پیدا نشد؛ ابتدا سورس را مستقر کن.",
        )
    })?;
    if !current.starts_with(&releases) || !current.is_dir() {
        return Err(error(
            StatusCode::CONFLICT,
            "مسیر release جاری خارج از فضای استقرار امن است.",
        ));
    }
    Ok(current)
}

fn app_framework(
    db: &Connection,
    app_id: &str,
    owner: &str,
) -> Result<(String, String), (StatusCode, Json<Value>)> {
    db.query_row(
        "SELECT framework,runtime_version FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
        params![app_id, owner],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(db_error)?
    .ok_or_else(|| error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد یا آرشیو شده است."))
}

fn parse_composer_manifest(root: &FsPath) -> Value {
    let manifest_path = root.join("composer.json");
    let manifest = fs::metadata(&manifest_path)
        .ok()
        .filter(|metadata| metadata.is_file() && metadata.len() <= 2 * 1024 * 1024)
        .and_then(|_| fs::read(manifest_path).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .unwrap_or(Value::Null);
    let require = manifest.get("require").and_then(Value::as_object);
    let php_constraint = require
        .and_then(|items| items.get("php"))
        .and_then(Value::as_str);
    let extensions = require
        .map(|items| {
            items
                .keys()
                .filter(|name| name.starts_with("ext-"))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let laravel_constraint = require
        .and_then(|items| items.get("laravel/framework"))
        .and_then(Value::as_str);
    json!({
        "valid": manifest.is_object(),
        "php_constraint": php_constraint,
        "laravel_constraint": laravel_constraint,
        "required_extensions": extensions,
        "has_lockfile": root.join("composer.lock").is_file(),
    })
}

fn valid_env_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_uppercase() || byte == b'_')
        && name.len() <= 64
        && bytes.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        && !name.starts_with("ARG_")
        && !["PATH", "HOME", "SHELL", "ENV", "BASH_ENV", "LD_PRELOAD"].contains(&name)
}

fn decode_env_value(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
            .replace("\\$", "$")
    } else {
        value.to_owned()
    }
}

fn read_environment(path: &FsPath) -> BTreeMap<String, String> {
    let Ok(contents) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (name, value) = line.split_once('=')?;
            let name = name.trim();
            valid_env_name(name).then(|| (name.to_owned(), decode_env_value(value)))
        })
        .collect()
}

fn encode_env_value(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._/-:@+".contains(&byte))
    {
        value.to_owned()
    } else {
        format!(
            "\"{}\"",
            value
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('$', "\\$")
        )
    }
}

fn save_environment_file(path: &FsPath, values: &BTreeMap<String, String>) -> io::Result<()> {
    let mut bytes = String::from(
        "# Managed by ARG. Values are stored on this VPS and are never returned by the API.\n",
    );
    for (name, value) in values {
        bytes.push_str(name);
        bytes.push('=');
        bytes.push_str(&encode_env_value(value));
        bytes.push('\n');
    }
    if bytes.len() > 64 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Laravel environment exceeds 64 KiB",
        ));
    }
    let temp = path.with_extension(format!("env-{}.tmp", random_job_id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)?;
    use std::io::Write as _;
    let result = (|| {
        file.write_all(bytes.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn inspect_release(root: &FsPath, app_id: &str, runtime_version: &str) -> Value {
    let env_path = deployment_root()
        .join("apps")
        .join(app_id)
        .join("shared/.env");
    let environment = read_environment(&env_path);
    let environment_configured = environment
        .get("APP_KEY")
        .is_some_and(|value| !value.is_empty())
        && (environment
            .get("DB_CONNECTION")
            .is_some_and(|value| value == "sqlite")
            || (environment
                .get("DB_DATABASE")
                .is_some_and(|value| !value.is_empty())
                && environment
                    .get("DB_USERNAME")
                    .is_some_and(|value| !value.is_empty())));
    let files = json!({
        "artisan": root.join("artisan").is_file(),
        "composer_manifest": root.join("composer.json").is_file(),
        "composer_lock": root.join("composer.lock").is_file(),
        "vendor_autoload": root.join("vendor/autoload.php").is_file(),
        "public_index": root.join("public/index.php").is_file(),
        "environment_configured": environment_configured,
        "app_key_configured": environment.get("APP_KEY").is_some_and(|value| !value.is_empty()),
        "environment_keys": environment.keys().collect::<Vec<_>>(),
        "persistent_storage": deployment_root().join("apps").join(app_id).join("shared/storage").is_dir(),
    });
    let composer = parse_composer_manifest(root);
    let complete = files["artisan"] == true
        && files["composer_manifest"] == true
        && composer["valid"] == true
        && files["public_index"] == true;
    let dependencies_ready = files["composer_lock"] == true && files["vendor_autoload"] == true;
    let state = if !complete {
        "not_laravel"
    } else if !dependencies_ready {
        "dependencies_missing"
    } else if files["environment_configured"] != true {
        "environment_missing"
    } else {
        "ready"
    };
    json!({
        "framework": "laravel",
        "state": state,
        "release_path": root,
        "runtime_version": runtime_version,
        "public_path": root.join("public"),
        "files": files,
        "composer": composer,
        "message": match state {
            "not_laravel" => "ساختار این release با قالب Laravel کامل تطبیق ندارد.",
            "dependencies_missing" => "composer.lock یا vendor/autoload.php موجود نیست؛ پیش از اجرای Artisan، وابستگی‌ها را با lockfile نصب کن.",
            "environment_missing" => "APP_KEY یا تنظیمات اتصال دیتابیس در فایل محیط پایدار کامل نیست؛ مقدارها را از همین پنل ثبت کن.",
            _ => "ساختار Laravel، وابستگی‌ها و فایل محیط پایدار پیدا شد؛ وضعیت سرویس وب و دیتابیس جداگانه بررسی شود.",
        }
    })
}

pub(super) async fn inspect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let (framework, runtime_version) = app_framework(&db, &app_id, &owner)?;
    if framework != "laravel" {
        return Err(error(
            StatusCode::CONFLICT,
            "Laravel Runner فقط برای اپلیکیشنی با قالب Laravel فعال است.",
        ));
    }
    let release = active_release(&app_id)?;
    let inspection = inspect_release(&release, &app_id, &runtime_version);
    let operations = recent_operations(&db, &app_id).map_err(db_error)?;
    Ok(Json(
        json!({"inspection":inspection,"operations":operations}),
    ))
}

pub(super) async fn operations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let (framework, _) = app_framework(&db, &app_id, &owner)?;
    if framework != "laravel" {
        return Err(error(StatusCode::CONFLICT, "قالب اپلیکیشن Laravel نیست."));
    }
    Ok(Json(
        json!({"operations":recent_operations(&db,&app_id).map_err(db_error)?}),
    ))
}

fn environment_path(app_id: &str) -> Result<PathBuf, (StatusCode, Json<Value>)> {
    let root = app_root(app_id)
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "شناسهٔ اپلیکیشن نامعتبر است."))?;
    let root = root
        .canonicalize()
        .map_err(|_| error(StatusCode::NOT_FOUND, "پوشهٔ اپلیکیشن پیدا نشد."))?;
    let shared = root.join("shared");
    let canonical_shared = shared.canonicalize().map_err(|_| {
        error(
            StatusCode::NOT_FOUND,
            "فضای پایدار Laravel هنوز ساخته نشده است.",
        )
    })?;
    if canonical_shared != shared || !canonical_shared.is_dir() {
        return Err(error(
            StatusCode::CONFLICT,
            "مسیر فضای پایدار Laravel یک پوشهٔ عادی در محدودهٔ اپلیکیشن نیست.",
        ));
    }
    let path = shared.join(".env");
    if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(error(
            StatusCode::CONFLICT,
            "فایل محیط Laravel به‌صورت symlink است و برای جلوگیری از نوشتن خارج از مسیر رد شد.",
        ));
    }
    Ok(path)
}

fn require_laravel_admin(
    db: &Connection,
    headers: &HeaderMap,
    app_id: &str,
) -> Result<(String, String), (StatusCode, Json<Value>)> {
    require_admin(db, headers)?;
    let owner = federation::local_server_uuid(db).map_err(db_error)?;
    let (framework, _) = app_framework(db, app_id, &owner)?;
    if framework != "laravel" {
        return Err(error(StatusCode::CONFLICT, "قالب اپلیکیشن Laravel نیست."));
    }
    Ok((owner, framework))
}

fn operation_in_progress(db: &Connection, app_id: &str) -> bool {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM laravel_operations WHERE application_id=?1 AND state IN ('queued','running'))",
        [app_id],
        |row| row.get::<_, bool>(0),
    )
    .unwrap_or(true)
}

fn deployment_in_progress(db: &Connection, app_id: &str) -> bool {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM deployment_jobs WHERE application_id=?1 AND state IN ('queued','validating','running','verifying'))",
        [app_id],
        |row| row.get::<_, bool>(0),
    )
    .unwrap_or(true)
}

pub(super) async fn save_environment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<EnvironmentRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if input.variables.is_empty() || input.variables.len() > 100 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "در هر بار ذخیره بین ۱ تا ۱۰۰ متغیر قابل ارسال است.",
        ));
    }
    if input.variables.iter().any(|(name, value)| {
        !valid_env_name(name)
            || value.len() > 4096
            || value.contains(['\n', '\r', '\0'])
            || value.chars().any(char::is_control)
    }) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام متغیر یا مقدار آن نامعتبر است؛ نام باید بزرگ‌حرف لاتین باشد و مقدار نباید خط جدید داشته باشد.",
        ));
    }
    let _environment_guard = APP_EXECUTION_LOCK.lock().await;
    let (actor, path) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        require_laravel_admin(&db, &headers, &app_id)?;
        if operation_in_progress(&db, &app_id) {
            return Err(error(
                StatusCode::CONFLICT,
                "هنگام اجرای Artisan یا Composer، فایل محیط را تغییر نده؛ پس از پایان دوباره تلاش کن.",
            ));
        }
        if deployment_in_progress(&db, &app_id) {
            return Err(error(
                StatusCode::CONFLICT,
                "هنگام دریافت یا فعال‌سازی release نمی‌توان محیط Laravel را تغییر داد.",
            ));
        }
        let _ = active_release(&app_id)?;
        (actor, environment_path(&app_id)?)
    };
    let mut values = read_environment(&path);
    let changed_keys = input.variables.keys().cloned().collect::<Vec<_>>();
    values.extend(input.variables);
    save_environment_file(&path, &values).map_err(|error_value| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("ثبت فایل محیط Laravel ناموفق بود: {error_value}"),
        )
    })?;
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor,
        "laravel.environment.update",
        &format!("app={app_id}; keys={}", changed_keys.join(",")),
    );
    Ok(Json(
        json!({"saved":true,"keys":changed_keys,"secrets_returned":false,"message":"متغیرها روی همین VPS در فایل .env خصوصی ذخیره شدند؛ مقدار محرمانه در پاسخ بازگردانده نمی‌شود."}),
    ))
}

pub(super) async fn generate_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    check_origin(&headers)?;
    let _environment_guard = APP_EXECUTION_LOCK.lock().await;
    let (actor, path) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        require_laravel_admin(&db, &headers, &app_id)?;
        if operation_in_progress(&db, &app_id) {
            return Err(error(
                StatusCode::CONFLICT,
                "هنگام اجرای Artisan یا Composer نمی‌توان APP_KEY را تغییر داد.",
            ));
        }
        if deployment_in_progress(&db, &app_id) {
            return Err(error(
                StatusCode::CONFLICT,
                "هنگام دریافت یا فعال‌سازی release نمی‌توان APP_KEY را تغییر داد.",
            ));
        }
        let _ = active_release(&app_id)?;
        (actor, environment_path(&app_id)?)
    };
    let mut values = read_environment(&path);
    if values.get("APP_KEY").is_some_and(|value| !value.is_empty()) {
        return Err(error(
            StatusCode::CONFLICT,
            "APP_KEY از قبل تنظیم شده است. تولید کلید جدید داده‌های رمزگذاری‌شدهٔ موجود را غیرقابل‌خواندن می‌کند؛ برای جلوگیری از قفل‌شدن برنامه، این دکمه کلید را جایگزین نمی‌کند.",
        ));
    }
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    values.insert("APP_KEY".into(), format!("base64:{}", STANDARD.encode(key)));
    save_environment_file(&path, &values).map_err(|error_value| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("ساخت APP_KEY ناموفق بود: {error_value}"),
        )
    })?;
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor,
        "laravel.environment.generate_key",
        &format!("app={app_id}; APP_KEY was generated and stored without returning it"),
    );
    Ok(Json(
        json!({"saved":true,"key_returned":false,"message":"APP_KEY تصادفی ساخته و در فایل .env خصوصی ذخیره شد؛ مقدار آن از API قابل خواندن نیست."}),
    ))
}

fn recent_operations(db: &Connection, app_id: &str) -> rusqlite::Result<Vec<Value>> {
    let mut statement = db.prepare("SELECT id,action,state,output,exit_code,error_summary,created_at,started_at,finished_at FROM laravel_operations WHERE application_id=?1 ORDER BY created_at DESC LIMIT 10")?;
    let rows = statement.query_map([app_id], |row| Ok(json!({"id":row.get::<_,String>(0)?,"action":row.get::<_,String>(1)?,"state":row.get::<_,String>(2)?,"output":row.get::<_,String>(3)?,"exit_code":row.get::<_,Option<i64>>(4)?,"error":row.get::<_,Option<String>>(5)?,"created_at":row.get::<_,i64>(6)?,"started_at":row.get::<_,Option<i64>>(7)?,"finished_at":row.get::<_,Option<i64>>(8)?})))?.collect();
    rows
}

fn action_argv(
    action: &str,
    confirmed: bool,
) -> Result<Vec<&'static str>, (StatusCode, Json<Value>)> {
    let args = match action {
        "about" => vec!["about", "--no-interaction"],
        "migrate-status" => vec!["migrate:status", "--no-interaction"],
        "migrate" if confirmed => vec!["migrate", "--force", "--no-interaction"],
        "migrate" => {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "اجرای migration به تأیید صریح نیاز دارد؛ عملیات دیتابیس با rollback کد برگردانده نمی‌شود.",
            ));
        }
        "cache-clear" => vec!["optimize:clear", "--no-interaction"],
        "config-cache" => vec!["config:cache", "--no-interaction"],
        "route-cache" => vec!["route:cache", "--no-interaction"],
        "view-cache" => vec!["view:cache", "--no-interaction"],
        "queue-restart" => vec!["queue:restart", "--no-interaction"],
        "install-dependencies" if confirmed => vec!["composer", "install"],
        "install-dependencies" => {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "نصب وابستگی‌ها از شبکه و فضای دیسک استفاده می‌کند؛ برای ادامه تأیید صریح لازم است.",
            ));
        }
        _ => {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "این عملیات Artisan در فهرست مجاز Laravel Runner نیست.",
            ));
        }
    };
    Ok(args)
}

pub(super) async fn start_operation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<OperationRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let _environment_guard = APP_EXECUTION_LOCK.lock().await;
    let (actor, runtime_version, app_root, release) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let (framework, runtime_version) = app_framework(&db, &app_id, &owner)?;
        if framework != "laravel" {
            return Err(error(StatusCode::CONFLICT, "قالب اپلیکیشن Laravel نیست."));
        }
        if deployment_in_progress(&db, &app_id) {
            return Err(error(
                StatusCode::CONFLICT,
                "هنگام دریافت یا فعال‌سازی release نمی‌توان عملیات Laravel را شروع کرد.",
            ));
        }
        let root = active_release(&app_id)?;
        let inspection = inspect_release(&root, &app_id, &runtime_version);
        let required_files_present = if input.action == "install-dependencies" {
            inspection["files"]["artisan"] == true
                && inspection["files"]["composer_manifest"] == true
                && inspection["files"]["composer_lock"] == true
        } else {
            inspection["files"]["artisan"] == true && inspection["files"]["vendor_autoload"] == true
        };
        if !required_files_present {
            return Err(error(StatusCode::CONFLICT, "پیش‌نیازهای Laravel کامل نیستند؛ artisan و vendor/autoload.php باید در release فعال موجود باشند."));
        }
        let args = action_argv(&input.action, input.confirmed)?;
        if args.first() == Some(&"migrate") && inspection["files"]["environment_configured"] != true
        {
            return Err(error(
                StatusCode::CONFLICT,
                "پیش از migration، فایل محیط پایدار برنامه را تنظیم کن.",
            ));
        }
        (
            actor,
            runtime_version,
            app_root(&app_id)
                .ok_or_else(|| error(StatusCode::BAD_REQUEST, "شناسهٔ اپلیکیشن نامعتبر است."))?,
            root,
        )
    };
    let id = random_job_id();
    let at = now();
    {
        let db = state.lock().unwrap();
        db.execute("INSERT INTO laravel_operations(id,application_id,actor,action,state,created_at) VALUES (?1,?2,?3,?4,'queued',?5)",params![id,app_id,actor,input.action,at]).map_err(|error_value| {
            if error_value.sqlite_error_code()==Some(rusqlite::ErrorCode::ConstraintViolation) {
                error(StatusCode::CONFLICT,"برای این اپلیکیشن یک عملیات Laravel دیگر در حال اجراست.")
            } else { db_error(error_value) }
        })?;
        audit(
            &db,
            &actor,
            "laravel.operation.start",
            &format!(
                "app={app_id}; action={}; release={}",
                input.action,
                release.display()
            ),
        );
    }
    let action = input.action.clone();
    let state_clone = state.clone();
    let operation_id = id.clone();
    tokio::spawn(async move {
        run_operation(
            state_clone,
            operation_id,
            app_id,
            action,
            runtime_version,
            app_root,
            release,
        )
        .await;
    });
    Ok(Json(
        json!({"id":id,"action":input.action,"state":"queued","message":"عملیات Laravel ثبت شد؛ نتیجه و خروجی امن از همین صفحه قابل پیگیری است."}),
    ))
}

fn random_job_id() -> String {
    let mut bytes = [0_u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    let hex = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

async fn run_operation(
    state: AppState,
    operation_id: String,
    app_id: String,
    action: String,
    runtime_version: String,
    app_root: PathBuf,
    release: PathBuf,
) {
    {
        let db = state.lock().unwrap();
        let _ = db.execute("UPDATE laravel_operations SET state='running',started_at=?1 WHERE id=?2 AND state='queued'",params![now(),operation_id]);
    }
    let command_result = execute_action(&action, &runtime_version, &app_root, &release).await;
    let (state_name, exit_code, output, error_summary) = match command_result {
        Ok((code, output)) if code == 0 => ("succeeded", Some(code as i64), output, None),
        Ok((code, output)) => (
            "failed",
            Some(code as i64),
            output,
            Some(format!("Artisan با کد خروج {code} پایان یافت.")),
        ),
        Err(message) => ("failed", None, String::new(), Some(message)),
    };
    let db = state.lock().unwrap();
    let _ = db.execute("UPDATE laravel_operations SET state=?1,output=?2,exit_code=?3,error_summary=?4,finished_at=?5 WHERE id=?6",params![state_name,output,exit_code,error_summary,now(),operation_id]);
    let actor: String = db
        .query_row(
            "SELECT actor FROM laravel_operations WHERE id=?1",
            [&operation_id],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "unknown".to_owned());
    audit(
        &db,
        &actor,
        "laravel.operation.finish",
        &format!("app={app_id}; action={action}; state={state_name}; operation={operation_id}"),
    );
}

async fn execute_action(
    action: &str,
    runtime_version: &str,
    app_root: &FsPath,
    release: &FsPath,
) -> Result<(i32, String), String> {
    let php = php_binary(runtime_version)?;
    let (program, args, cwd) = if action == "install-dependencies" {
        if !release.join("composer.json").is_file() || !release.join("composer.lock").is_file() {
            return Err(
                "برای نصب امن وابستگی‌ها، composer.json و composer.lock هر دو لازم‌اند.".into(),
            );
        }
        let composer = ["/usr/bin/composer", "/usr/local/bin/composer"]
            .iter()
            .map(PathBuf::from)
            .find(|path| path.is_file())
            .ok_or_else(|| {
                "Composer نصب نیست؛ از مدیریت APT نسخهٔ تأییدشده را نصب کن.".to_owned()
            })?;
        if available_disk_bytes(app_root).is_none_or(|bytes| bytes < 512 * 1024 * 1024) {
            return Err(
                "برای نصب Composer حداقل 512 MiB فضای آزاد در مسیر release لازم است.".into(),
            );
        }
        (
            php,
            vec![
                composer.to_string_lossy().into_owned(),
                "install".into(),
                "--no-dev".into(),
                "--no-interaction".into(),
                "--no-progress".into(),
                "--prefer-dist".into(),
                "--optimize-autoloader".into(),
                "--no-scripts".into(),
                "--no-plugins".into(),
            ],
            release,
        )
    } else {
        let args = action_argv(action, true).map_err(|(_, body)| {
            body["error"]
                .as_str()
                .unwrap_or("عملیات مجاز نیست.")
                .to_owned()
        })?;
        if !release.join("vendor/autoload.php").is_file() {
            return Err(
                "vendor/autoload.php پیدا نشد؛ ابتدا وابستگی‌ها را با lockfile نصب کن.".into(),
            );
        }
        let mut argv = vec!["artisan".to_owned()];
        argv.extend(args.into_iter().map(str::to_owned));
        (php, argv, release)
    };
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("HOME", "/var/lib/arg")
        .env("COMPOSER_HOME", "/var/lib/arg/composer")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|e| format!("اجرای ابزار Laravel ناموفق بود: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "خروجی stdout در دسترس نیست.".to_owned())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "خروجی stderr در دسترس نیست.".to_owned())?;
    let stdout_task = tokio::spawn(read_capped(stdout));
    let stderr_task = tokio::spawn(read_capped(stderr));
    let status = match tokio::time::timeout(COMMAND_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(error_value)) => {
            return Err(format!("خواندن وضعیت عملیات ناموفق بود: {error_value}"))
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(
                "عملیات از مهلت ۲۰ دقیقه عبور کرد و متوقف شد؛ release فعال تغییر نکرد.".into(),
            );
        }
    };
    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    let mut output = String::from_utf8_lossy(&stdout).into_owned();
    if !stderr.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&String::from_utf8_lossy(&stderr));
    }
    let output = if action == "install-dependencies" {
        if status.success() {
            "وابستگی‌های قفل‌شده نصب شدند؛ Composer script و plugin اجرا نشدند.".to_owned()
        } else {
            "Composer نتوانست وابستگی‌های composer.lock را نصب کند. منبع بسته‌ها، نیازمندی‌های PHP و شبکهٔ سرور را بررسی کن؛ خروجی خام پنهان است چون ممکن است اطلاعات مخزن خصوصی داشته باشد.".to_owned()
        }
    } else {
        sanitize_output(&output, &app_root.join("shared/.env"))
    };
    Ok((status.code().unwrap_or(128), output))
}

async fn read_capped<R: tokio::io::AsyncRead + Unpin>(mut stream: R) -> Vec<u8> {
    let mut captured = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                let remaining = OUTPUT_LIMIT.saturating_sub(captured.len());
                captured.extend_from_slice(&buffer[..count.min(remaining)]);
            }
        }
    }
    captured
}

fn sanitize_output(output: &str, env_file: &FsPath) -> String {
    const MARKERS: &[&str] = &[
        "password=",
        "password:",
        "passwd=",
        "passwd:",
        "token=",
        "token:",
        "secret=",
        "secret:",
        "authorization=",
        "authorization:",
        "api_key=",
        "api-key=",
        "api_key:",
        "api-key:",
    ];
    let values = read_environment(env_file)
        .into_iter()
        .filter(|(name, value)| {
            !value.is_empty()
                && (name == "APP_KEY"
                    || name.ends_with("_PASSWORD")
                    || name.ends_with("_SECRET")
                    || name.ends_with("_TOKEN")
                    || name.contains("CREDENTIAL"))
        })
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    output
        .lines()
        .take(1200)
        .map(|line| {
            let mut result = line.to_owned();
            for marker in MARKERS {
                let lower = result.to_ascii_lowercase();
                let Some(index) = lower.find(marker) else {
                    continue;
                };
                let mut value_start = index + marker.len();
                while result[value_start..]
                    .chars()
                    .next()
                    .is_some_and(char::is_whitespace)
                {
                    value_start += result[value_start..].chars().next().unwrap().len_utf8();
                }
                let value_end = result[value_start..]
                    .find(|character: char| {
                        character.is_whitespace() || matches!(character, ',' | ';')
                    })
                    .map(|offset| value_start + offset)
                    .unwrap_or(result.len());
                if value_end > value_start {
                    result.replace_range(value_start..value_end, "[REDACTED]");
                }
            }
            for value in &values {
                result = result.replace(value, "[REDACTED]");
            }
            redact_url_credentials(&mut result);
            result
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_url_credentials(line: &mut String) {
    for scheme in ["https://", "http://"] {
        let mut from = 0;
        while from < line.len() {
            let Some(offset) = line[from..].find(scheme) else {
                break;
            };
            let start = from + offset + scheme.len();
            let authority_end = line[start..]
                .find('/')
                .map(|offset| start + offset)
                .unwrap_or(line.len());
            let Some(at_offset) = line[start..authority_end].find('@') else {
                from = start;
                continue;
            };
            let at = start + at_offset;
            if line[start..at].contains(':') {
                line.replace_range(start..at, "[REDACTED]");
                from = start + "[REDACTED]@".len();
            } else {
                from = at + 1;
            }
        }
    }
}

fn php_binary(runtime_version: &str) -> Result<PathBuf, String> {
    if runtime_version == "system" {
        return ["/usr/bin/php", "/usr/local/bin/php"]
            .iter()
            .map(PathBuf::from)
            .find(|path| path.is_file())
            .ok_or_else(|| "PHP CLI نصب نیست؛ نسخهٔ سازگار را از مدیریت Runtime نصب کن.".into());
    }
    if runtime_version.len() > 32
        || !runtime_version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
        || !runtime_version.contains('.')
    {
        return Err("نسخهٔ PHP ذخیره‌شده نامعتبر است.".into());
    }
    let path = PathBuf::from(format!("/usr/bin/php{runtime_version}"));
    path.is_file().then_some(path).ok_or_else(|| {
        format!("PHP {runtime_version} روی این سرور نصب نیست؛ از مدیریت Runtime آن را نصب کن.")
    })
}

fn available_disk_bytes(path: &FsPath) -> Option<u64> {
    use std::ffi::CString;
    let path = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    (unsafe { libc::statvfs(path.as_ptr(), &mut stat) } == 0).then(|| {
        (stat.f_bavail as u128)
            .saturating_mul(stat.f_frsize as u128)
            .min(u64::MAX as u128) as u64
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn laravel_manifest_reports_only_non_secret_runtime_requirements() {
        let root = std::env::temp_dir().join(format!("arg-laravel-manifest-{}", now()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("composer.json"), r#"{"require":{"php":"^8.2","laravel/framework":"^11.0","ext-mbstring":"*","ext-openssl":"*"},"extra":{"secret":"do-not-return"}}"#).unwrap();
        fs::write(root.join("composer.lock"), "{}").unwrap();
        let parsed = parse_composer_manifest(&root);
        assert_eq!(parsed["php_constraint"], "^8.2");
        assert_eq!(parsed["required_extensions"].as_array().unwrap().len(), 2);
        assert!(parsed["extra"].is_null());
        assert!(parsed.to_string().find("do-not-return").is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artisan_allowlist_requires_confirmation_for_migrations_and_rejects_custom_commands() {
        assert!(action_argv("migrate", false).is_err());
        assert_eq!(
            action_argv("migrate", true).unwrap(),
            vec!["migrate", "--force", "--no-interaction"]
        );
        assert!(action_argv("config:clear;touch /tmp/x", true).is_err());
        assert!(action_argv("cache-clear", true).is_ok());
    }

    #[test]
    fn operation_output_redacts_common_secret_fields_and_caps_lines() {
        let output = sanitize_output(
            "password=abc token: xyz\nartisan ok",
            FsPath::new("/missing/env"),
        );
        assert!(!output.contains("abc"));
        assert!(!output.contains("xyz"));
        assert!(output.contains("artisan ok"));
    }

    #[test]
    fn environment_values_round_trip_without_being_part_of_a_response() {
        let root = std::env::temp_dir().join(format!("arg-laravel-env-{}", now()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join(".env");
        let values = BTreeMap::from([
            ("APP_ENV".to_owned(), "production".to_owned()),
            (
                "DB_PASSWORD".to_owned(),
                "a secret with $ and \"quotes\"".to_owned(),
            ),
        ]);
        save_environment_file(&path, &values).unwrap();
        let stored = read_environment(&path);
        assert_eq!(stored, values);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(valid_env_name("DB_PASSWORD"));
        assert!(!valid_env_name("LD_PRELOAD"));
        assert!(!valid_env_name("lowercase"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn persistent_env_and_storage_survive_release_switches() {
        let root = std::env::temp_dir().join(format!("arg-laravel-shared-{}", now()));
        let first = root.join("releases/first");
        let second = root.join("releases/second");
        fs::create_dir_all(first.join("storage/app")).unwrap();
        fs::create_dir_all(second.join("storage/app")).unwrap();
        fs::create_dir_all(first.join("bootstrap")).unwrap();
        fs::create_dir_all(second.join("bootstrap")).unwrap();
        fs::write(first.join(".env"), "APP_KEY=first\nDB_PASSWORD=secret\n").unwrap();
        fs::write(first.join("storage/app/persist.txt"), "preserve me").unwrap();
        fs::write(second.join(".env"), "APP_KEY=replace-me\n").unwrap();
        fs::write(second.join("storage/app/replace.txt"), "new release data").unwrap();

        prepare_release_at(&root, &first).unwrap();
        prepare_release_at(&root, &second).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("shared/.env")).unwrap(),
            "APP_KEY=first\nDB_PASSWORD=secret\n"
        );
        assert_eq!(
            fs::read_to_string(second.join("storage/app/persist.txt")).unwrap(),
            "preserve me"
        );
        assert_eq!(
            fs::read_link(second.join(".env")).unwrap(),
            PathBuf::from("../../shared/.env")
        );
        assert_eq!(
            fs::metadata(root.join("shared/.env"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn laravel_migration_is_additive_and_interrupted_operations_are_recoverable() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE applications(id TEXT PRIMARY KEY);")
            .unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        assert!(db.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('laravel_operations') WHERE name='output')", [], |row| row.get::<_,bool>(0)).unwrap());
    }

    #[test]
    fn web_templates_are_fixed_per_app_and_restrict_php_and_dotfiles() {
        let app_id = "12345678-1234-4234-8234-123456789abc";
        let nginx = app_nginx_config(
            app_id,
            "shop.example.test",
            "/run/php/arg-app-12345678.sock",
            None,
        );
        assert!(nginx.starts_with(&format!("# ARG managed Laravel site app={app_id}")));
        assert!(nginx.contains("server_name shop.example.test;"));
        assert!(nginx.contains("location = /index.php"));
        assert!(nginx.contains("location ~* \\.(?:php|phtml|phar)(?:/|$) { return 404; }"));
        assert!(nginx.contains("location ~ /\\.(?!well-known(?:/|$)) { deny all; }"));
        let octane = app_nginx_config(
            app_id,
            "shop.example.test",
            "/run/php/arg-app-12345678.sock",
            Some(32_345),
        );
        assert!(octane.contains("proxy_pass http://127.0.0.1:32345;"));
        assert!(octane.contains("X-Forwarded-Proto $scheme"));
        assert!(!octane.contains("fastcgi_pass"));
        assert!(octane.contains("location ~* \\.(?:php|phtml|phar)(?:/|$) { return 404; }"));
        assert!(!valid_host("shop.example.test; include /tmp/evil.conf;"));
        assert!(!valid_host("*.example.test"));

        let pool = php_pool_config(app_id, "/run/php/arg-app-12345678.sock");
        assert!(pool.contains("pm = ondemand"));
        assert!(pool.contains("pm.max_children = 8"));
        assert!(pool.contains("php_admin_value[open_basedir]"));
        assert!(pool.contains("clear_env = yes"));
        assert!(!pool.contains("/tmp/evil"));
    }

    #[test]
    fn service_helper_rejects_unrecognized_commands_before_system_changes() {
        let result = laravel_service_helper(&json!({
            "app_id":"12345678-1234-4234-8234-123456789abc",
            "runtime_version":"8.3",
            "service":"queue; touch /tmp/pwned",
            "enabled":true
        }));
        assert!(result.unwrap_err().contains("نوع سرویس مجاز نیست"));
        assert!(php_fpm_paths("8.3;id", "12345678-1234-4234-8234-123456789abc").is_err());
        let response = execute_privileged_request(
            "laravel-web",
            &json!({"app_id":"../../etc/passwd","enabled":true}),
        );
        assert_eq!(response["ok"], false);
    }

    #[test]
    fn laravel_background_commands_are_fixed_and_ports_are_stable_loopback_values() {
        let app_id = "12345678-1234-4234-8234-123456789abc";
        assert_eq!(laravel_service_command("queue", None).unwrap(), "queue:work --sleep=3 --tries=3 --timeout=90 --no-interaction");
        assert_eq!(laravel_service_command("scheduler", None).unwrap(), "schedule:work");
        assert_eq!(laravel_service_command("horizon", None).unwrap(), "horizon");
        assert!(laravel_service_command("queue;id", None).is_err());
        assert!(laravel_service_command("reverb", None).is_err());
        let reverb = laravel_service_port(app_id, "reverb").unwrap();
        let octane = laravel_service_port(app_id, "octane").unwrap();
        assert!((20_000..30_000).contains(&reverb));
        assert!((32_000..42_000).contains(&octane));
        assert_eq!(reverb, laravel_service_port(app_id, "reverb").unwrap());
        assert_ne!(reverb, octane);
        assert_eq!(laravel_service_port(app_id, "queue"), None);
    }

    #[test]
    fn php_fpm_versions_are_compared_numerically_not_lexicographically() {
        assert!(php_version_key("8.10") > php_version_key("8.9"));
        assert!(php_version_key("8.5") > php_version_key("8.4"));
        assert_eq!(php_version_key("8.x"), vec![8]);
    }

    #[test]
    fn optional_laravel_services_require_their_composer_runtime_files() {
        let root = std::env::temp_dir().join(format!("arg-laravel-services-{}", now()));
        let release = root.join("release");
        fs::create_dir_all(release.join("vendor/laravel/horizon/src")).unwrap();
        fs::create_dir_all(release.join("vendor/laravel/reverb/src")).unwrap();
        fs::create_dir_all(release.join("vendor/laravel/octane/src")).unwrap();
        fs::create_dir_all(release.join("vendor/bin")).unwrap();
        fs::write(release.join("vendor/laravel/horizon/src/HorizonServiceProvider.php"), "").unwrap();
        fs::write(release.join("vendor/laravel/reverb/src/ReverbServiceProvider.php"), "").unwrap();
        fs::write(release.join("vendor/laravel/octane/src/OctaneServiceProvider.php"), "").unwrap();
        assert!(laravel_service_available("horizon", &release));
        assert!(laravel_service_available("reverb", &release));
        assert!(!laravel_service_available("octane", &release));
        fs::write(release.join("vendor/bin/rr"), "").unwrap();
        assert!(laravel_service_available("octane", &release));
        assert!(!laravel_service_available("custom", &release));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn nginx_host_overlap_detects_wildcards_without_matching_sibling_domains() {
        assert!(host_pattern_matches("*.example.test", "shop.example.test"));
        assert!(host_pattern_matches(".example.test", "shop.example.test"));
        assert!(host_pattern_matches("=shop.example.test", "shop.example.test"));
        assert!(!host_pattern_matches("*.example.test", "example.test"));
        assert!(!host_pattern_matches("*.example.test", "other.test"));
    }

    #[test]
    fn migration_adds_web_state_column_to_an_existing_phase_six_schema() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE applications(id TEXT PRIMARY KEY); CREATE TABLE laravel_operations(id TEXT PRIMARY KEY,application_id TEXT NOT NULL,actor TEXT NOT NULL,action TEXT NOT NULL,state TEXT NOT NULL,output TEXT NOT NULL DEFAULT '',exit_code INTEGER,error_summary TEXT,created_at INTEGER NOT NULL,started_at INTEGER,finished_at INTEGER); CREATE TABLE laravel_app_sites(application_id TEXT PRIMARY KEY,site_name TEXT NOT NULL UNIQUE,domain TEXT NOT NULL,php_fpm_socket TEXT NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);").unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        assert!(db.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('laravel_app_sites') WHERE name='enabled')", [], |row| row.get::<_, bool>(0)).unwrap());
    }
}
