use super::*;
use std::path::Path;

/// Read-only host diagnostics. This endpoint does not repair, install, restart, or contact providers.
pub(super) async fn read(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;

    let os_release = fs::read_to_string("/etc/os-release").unwrap_or_default();
    let os_value = |key: &str| -> String {
        os_release
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .map(|value| value.trim_matches('"').to_owned())
            .unwrap_or_default()
    };
    let os_id = os_value("ID");
    let os_like = os_value("ID_LIKE");
    let supported_family = [os_id.as_str(), os_like.as_str()].iter().any(|value| {
        value
            .split_whitespace()
            .any(|part| matches!(part, "debian" | "ubuntu"))
    });
    let binary = |name: &str| -> bool {
        ["/usr/bin", "/usr/sbin", "/usr/local/bin", "/bin", "/sbin"]
            .iter()
            .any(|dir| Path::new(dir).join(name).is_file())
    };

    let inventory = storage_mounts::inventory();
    let root = inventory["volumes"]
        .as_array()
        .and_then(|volumes| volumes.iter().find(|volume| volume["mount_point"] == "/"));
    let root_available = root.and_then(|volume| volume["available_bytes"].as_u64());
    let cert = env::var("ARG_TLS_CERT").unwrap_or_default();
    let key = env::var("ARG_TLS_KEY").unwrap_or_default();
    let tls_enabled = !cert.is_empty()
        && !key.is_empty()
        && Path::new(&cert).is_file()
        && Path::new(&key).is_file();

    let (s3_connections, s3_connected, s3_buckets, s3_mounts): (i64, i64, i64, i64) = db
        .query_row(
            "SELECT (SELECT COUNT(*) FROM s3_connections),
                    (SELECT COUNT(*) FROM s3_connections WHERE status='connected'),
                    (SELECT COUNT(*) FROM s3_buckets WHERE enabled=1),
                    (SELECT COUNT(*) FROM s3_mounts WHERE status='mounted')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(db_error)?;
    let (nodes, connected_nodes): (i64, i64) = db
        .query_row(
            "SELECT COUNT(*),SUM(CASE WHEN connection_state IN ('healthy','connected') THEN 1 ELSE 0 END) FROM federated_nodes",
            [],
            |row| Ok((row.get(0)?, row.get::<_, Option<i64>>(1)?.unwrap_or(0))),
        )
        .map_err(db_error)?;
    let node_entries: Vec<Value> = {
        let mut statement = db.prepare("SELECT COALESCE(NULLIF(public_name,''),NULLIF(name,''),endpoint),endpoint,connection_state,compatibility_state,last_seen,last_check,last_error FROM federated_nodes ORDER BY name COLLATE NOCASE LIMIT 100").map_err(db_error)?;
        let rows = statement.query_map([], |row| {
            let name: String = row.get(0)?;
            let endpoint: String = row.get(1)?;
            let state: String = row.get(2)?;
            let compatibility: String = row.get(3)?;
            let last_seen: Option<i64> = row.get(4)?;
            let last_check: Option<i64> = row.get(5)?;
            let last_error: Option<String> = row.get(6)?;
            let origin = url::Url::parse(&endpoint).ok().and_then(|parsed| {
                let host = parsed.host_str()?;
                let port = parsed.port().map(|port| format!(":{port}")).unwrap_or_default();
                Some(format!("{}://{host}{port}", parsed.scheme()))
            }).unwrap_or_else(|| "آدرس نامعتبر".to_owned());
            Ok(json!({"name":name,"endpoint":origin,"state":state,"compatibility":compatibility,"last_seen":last_seen,"last_check":last_check,"last_error":last_error}))
        }).map_err(db_error)?;
        rows.filter_map(Result::ok).collect()
    };
    let active_controllers: Vec<Value> = {
        let mut statement = db.prepare("SELECT COALESCE(NULLIF(local_label,''),controller_name),last_seen FROM federation_controllers WHERE enabled=1 ORDER BY created_at DESC LIMIT 100").map_err(db_error)?;
        let rows = statement.query_map([], |row| Ok(json!({"name":row.get::<_,String>(0)?,"last_seen":row.get::<_,Option<i64>>(1)?}))).map_err(db_error)?;
        rows.filter_map(Result::ok).collect()
    };
    let (controllers, enabled_controllers): (i64, i64) = db.query_row(
        "SELECT COUNT(*),SUM(CASE WHEN enabled=1 THEN 1 ELSE 0 END) FROM federation_controllers",
        [],
        |row| Ok((row.get(0)?, row.get::<_, Option<i64>>(1)?.unwrap_or(0))),
    ).map_err(db_error)?;
    let (applications, deployments): (i64, i64) = db
        .query_row(
            "SELECT (SELECT COUNT(*) FROM applications WHERE deleted_at IS NULL),
                    (SELECT COUNT(*) FROM deployment_jobs)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(db_error)?;
    let runtime_tools: Vec<&str> = ["python3", "php", "node", "npm", "nginx", "rclone", "s3fs"]
        .into_iter()
        .filter(|name| binary(name))
        .collect();
    let systemd = Path::new("/run/systemd/system").is_dir();

    let checks = vec![
        json!({
            "id":"os", "label":"Debian / Ubuntu family",
            "status":if supported_family {"pass"} else {"warn"},
            "value":if os_id.is_empty() {"نامشخص".to_owned()} else {format!("{} {}", os_id, os_value("VERSION_ID"))},
            "detail":if supported_family {"خانوادهٔ سیستم‌عامل شناسایی شد؛ این بررسی جایگزین ماتریس کامل سازگاری نسخه‌ها نیست."} else {"این خانواده در مسیر نصب معمول ARG تأیید نشده است."}
        }),
        json!({
            "id":"systemd", "label":"systemd runtime",
            "status":if systemd {"pass"} else {"warn"},
            "value":if systemd {"در حال اجرا"} else {"در دسترس نیست"},
            "detail":"فقط وجود محیط systemd بررسی شد؛ سلامت واحدهای سرویس جداگانه ارزیابی نشده است."
        }),
        json!({
            "id":"docker", "label":"Docker", "status":if binary("docker") {"pass"} else {"info"},
            "value":if binary("docker") {if Path::new("/run/docker.sock").exists() {"نصب است · سوکت موجود"} else {"نصب است · سوکت یافت نشد"}} else {"نصب نیست"},
            "detail":"این بررسی فقط فایل اجرایی و سوکت را می‌سنجد؛ کانتینرها اجرا یا تغییر داده نمی‌شوند."
        }),
        json!({
            "id":"root-disk", "label":"فضای آزاد دیسک ریشه",
            "status":if root_available.map(|bytes| bytes > 1024*1024*1024).unwrap_or(false) {"pass"} else if root_available.is_some() {"warn"} else {"info"},
            "available_bytes":root_available,
            "detail":"از inventory فایل‌سیستم ریشه خوانده شده است؛ مقدار نامشخص به‌معنی صفر نیست."
        }),
        json!({
            "id":"panel-tls", "label":"گواهی TLS پنل",
            "status":if tls_enabled {"pass"} else {"warn"},
            "value":if tls_enabled {"گواهی و کلید پیکربندی شده‌اند"} else {"TLS پنل فعال تشخیص داده نشد"},
            "detail":"محتوا و مسیر فایل کلید خصوصی خوانده یا در پاسخ افشا نمی‌شود."
        }),
        json!({
            "id":"runtimes", "label":"ابزارهای Runtime", "status":"info", "value":runtime_tools,
            "detail":"فقط وجود executable بررسی می‌شود؛ نسخه، پیکربندی و سلامت برنامه‌ها نیازمند بررسی اختصاصی است."
        }),
        json!({
            "id":"s3", "label":"اتصال‌های S3",
            "status":if s3_connections == 0 {"info"} else if s3_connections == s3_connected {"pass"} else {"warn"},
            "connections":s3_connections,"connected":s3_connected,"buckets":s3_buckets,"mounted":s3_mounts,
            "detail":"آمار محلی ARG؛ هیچ درخواست شبکه‌ای یا اسکن Bucket اجرا نشده است."
        }),
        json!({
            "id":"nodes", "label":"Nodeهای جفت‌شده",
            "status":if nodes == 0 {"info"} else if nodes == connected_nodes {"pass"} else {"warn"},
            "total":nodes,"connected":connected_nodes,"items":node_entries,
            "detail":"وضعیت آخرین ارتباط ذخیره‌شده در ARG؛ healthy و connected متصل شمرده می‌شوند. برای هر Node نام عمومی، آدرس پنل، سازگاری، آخرین ارتباط و خطای ثبت‌شده آمده است؛ این گزارش خودش اتصال شبکه‌ای تازه برقرار نمی‌کند."
        }),
        json!({
            "id":"controllers", "label":"پنل‌های کنترل‌کنندهٔ مجاز",
            "status":if enabled_controllers == 0 {"info"} else {"pass"},
            "total":controllers,"connected":enabled_controllers,"items":active_controllers,
            "detail":"فهرست پنل‌هایی که روی همین VPS اجازهٔ مدیریت دارند؛ کلیدها و شناسه‌های محرمانه در گزارش نمایش داده نمی‌شوند."
        }),
        json!({
            "id":"apps", "label":"اپلیکیشن‌ها و Deployها", "status":"info",
            "applications":applications,"deployments":deployments,
            "detail":"فقط تعداد رکوردها؛ محتوای سورس، environment و credential در خروجی نیست."
        }),
    ];
    let warnings = checks
        .iter()
        .filter(|item| item["status"] == "warn")
        .count();
    Ok(Json(json!({
        "read_only":true,
        "generated_at":now(),
        "summary":{"checks":checks.len(),"warnings":warnings},
        "checks":checks
    })))
}
