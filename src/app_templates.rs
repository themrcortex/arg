use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TemplateDraftInput {
    pub(super) name: String,
    #[serde(default)]
    pub(super) description: String,
    #[serde(default)]
    pub(super) domain: String,
    #[serde(default)]
    pub(super) listen_port: Option<u16>,
    #[serde(default)]
    pub(super) environment_id: Option<String>,
    #[serde(default = "default_runtime")]
    pub(super) runtime_version: String,
    #[serde(default = "default_runtime")]
    pub(super) build_runtime_version: String,
}

fn default_runtime() -> String {
    "system".to_owned()
}

const TEMPLATE_IDS: &[&str] = &[
    "laravel-app",
    "react-vite-static",
    "node-api",
    "python-web",
    "wordpress-app",
];

pub(super) async fn list(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    Ok(Json(json!({
        "schema_version": 1,
        "templates": [
            {"id":"laravel-app","version":1,"name":"Laravel application","framework":"laravel","source_type":"manual","runtime":"php","readiness":"draft_only","requirements":["PHP Runtime سازگار","دسترسی سورس Laravel","پایگاه داده جداگانه در صورت نیاز"],"creates":"application_record","installs_packages":false,"creates_services":false},
            {"id":"react-vite-static","version":1,"name":"React / Vite static site","framework":"vite","source_type":"manual","runtime":"node-build","readiness":"draft_only","requirements":["Node.js برای build","ورودی index.html در خروجی build","دامنه در صورت انتشار عمومی"],"creates":"application_record","installs_packages":false,"creates_services":false},
            {"id":"node-api","version":1,"name":"Node.js API","framework":"node","source_type":"manual","runtime":"node","readiness":"draft_only","requirements":["package.json معتبر","اسکریپت start","پورت loopback آزاد و health check"],"creates":"application_record","installs_packages":false,"creates_services":false},
            {"id":"python-web","version":1,"name":"Python web application","framework":"python","source_type":"manual","runtime":"python-venv","readiness":"draft_only","requirements":["Python نصب‌شده و نسخهٔ پشتیبانی‌شده","یکی از Flask یا Django و فایل dependency قفل‌شده","نقطهٔ شروع WSGI/ASGI و پورت loopback"],"creates":"application_record","installs_packages":false,"creates_services":false},
            {"id":"wordpress-app","version":1,"name":"WordPress site","framework":"wordpress","source_type":"manual","runtime":"php-fpm","readiness":"draft_only","requirements":["کد WordPress موجود یا منبعی که خودت بررسی کرده‌ای","PHP-FPM سازگار و دیتابیس MySQL/MariaDB آماده","دامنه، TLS و مسیرهای backup بررسی‌شده"],"creates":"application_record","installs_packages":false,"creates_services":false}
        ],
        "note":"این preset فقط پیش‌نویس رکورد می‌سازد؛ سورس دریافت، پکیج نصب یا سرویس اجرا نمی‌کند. برای Deploy از Job استقرار همان اپ استفاده کن."
    })))
}

pub(super) async fn create_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(template_id): Path<String>,
    Json(input): Json<TemplateDraftInput>,
) -> ApiResult {
    if !TEMPLATE_IDS.contains(&template_id.as_str()) {
        return Err(error(StatusCode::NOT_FOUND, "Preset اپلیکیشن پیدا نشد."));
    }
    if input.name.trim().is_empty() || input.name.chars().count() > 80 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام اپلیکیشن باید بین ۱ تا ۸۰ نویسه باشد.",
        ));
    }
    if input.description.chars().count() > 1000 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "توضیحات اپلیکیشن بیش از حد طولانی است.",
        ));
    }
    apps::create_from_template(State(state), headers, template_id, input).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique_and_versioned() {
        let mut ids = TEMPLATE_IDS.to_vec();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), TEMPLATE_IDS.len());
        assert!(TEMPLATE_IDS.contains(&"laravel-app"));
        assert!(TEMPLATE_IDS.contains(&"react-vite-static"));
        assert!(TEMPLATE_IDS.contains(&"node-api"));
        assert!(TEMPLATE_IDS.contains(&"python-web"));
        assert!(TEMPLATE_IDS.contains(&"wordpress-app"));
    }
}
