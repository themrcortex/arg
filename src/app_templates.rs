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
    #[serde(default)]
    pub(super) wordpress_version: String,
    #[serde(default)]
    pub(super) wordpress_site_title: String,
    #[serde(default)]
    pub(super) wordpress_admin_username: String,
    #[serde(default)]
    pub(super) wordpress_admin_email: String,
    #[serde(default)]
    pub(super) wordpress_admin_password: String,
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
static WORDPRESS_VERSION_CACHE: std::sync::LazyLock<tokio::sync::Mutex<CachedWordPressVersions>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::const_new(None));
type CachedWordPressVersions = Option<(std::time::Instant, Vec<String>)>;

async fn current_wordpress_versions() -> Vec<String> {
    let mut cache = WORDPRESS_VERSION_CACHE.lock().await;
    if let Some((checked_at, versions)) = cache.as_ref() {
        let ttl = if versions.is_empty() {
            std::time::Duration::from_secs(60)
        } else {
            std::time::Duration::from_secs(6 * 60 * 60)
        };
        if checked_at.elapsed() < ttl {
            return versions.clone();
        }
    }
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    else {
        *cache = Some((std::time::Instant::now(), Vec::new()));
        return Vec::new();
    };
    let Ok(response) = client
        .get("https://api.wordpress.org/core/version-check/1.7/")
        .send()
        .await
    else {
        *cache = Some((std::time::Instant::now(), Vec::new()));
        return Vec::new();
    };
    if !response.status().is_success() {
        *cache = Some((std::time::Instant::now(), Vec::new()));
        return Vec::new();
    }
    let Ok(document) = response.json::<Value>().await else {
        *cache = Some((std::time::Instant::now(), Vec::new()));
        return Vec::new();
    };
    let versions = parse_wordpress_versions(&document);
    *cache = Some((std::time::Instant::now(), versions.clone()));
    versions
}

fn parse_wordpress_versions(document: &Value) -> Vec<String> {
    let mut versions = Vec::new();
    for offer in document
        .get("offers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(version) = offer.get("version").and_then(Value::as_str) else {
            continue;
        };
        if apps::valid_wordpress_version(version) && !versions.iter().any(|item| item == version) {
            versions.push(version.to_owned());
        }
        if versions.len() >= 12 {
            break;
        }
    }
    versions
}

pub(super) async fn list(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let wordpress_versions = current_wordpress_versions().await;
    let wordpress_latest = wordpress_versions.first().cloned();
    Ok(Json(json!({
        "schema_version": 1,
        "wordpress_versions": wordpress_versions,
        "wordpress_latest": wordpress_latest,
        "templates": [
            {"id":"laravel-app","version":1,"name":"Laravel application","framework":"laravel","source_type":"manual","runtime":"php","readiness":"draft_only","requirements":["PHP Runtime سازگار","دسترسی سورس Laravel","پایگاه داده جداگانه در صورت نیاز"],"creates":"application_record","installs_packages":false,"creates_services":false},
            {"id":"react-vite-static","version":1,"name":"React / Vite static site","framework":"vite","source_type":"manual","runtime":"node-build","readiness":"draft_only","requirements":["Node.js برای build","ورودی index.html در خروجی build","دامنه در صورت انتشار عمومی"],"creates":"application_record","installs_packages":false,"creates_services":false},
            {"id":"node-api","version":1,"name":"Node.js API","framework":"node","source_type":"manual","runtime":"node","readiness":"draft_only","requirements":["package.json معتبر","اسکریپت start","پورت loopback آزاد و health check"],"creates":"application_record","installs_packages":false,"creates_services":false},
            {"id":"python-web","version":1,"name":"Python web application","framework":"python","source_type":"manual","runtime":"python-venv","readiness":"draft_only","requirements":["Python نصب‌شده و نسخهٔ پشتیبانی‌شده","یکی از Flask یا Django و فایل dependency قفل‌شده","نقطهٔ شروع WSGI/ASGI و پورت loopback"],"creates":"application_record","installs_packages":false,"creates_services":false},
            {"id":"wordpress-app","version":2,"name":"WordPress site","framework":"wordpress","source_type":"git","runtime":"php-fpm","readiness":"automated_initial_deployment","requirements":["دامنهٔ معتبر؛ برای SSL خودکار باید به همین سرور اشاره کند","ایمیل مدیر سایت و گذرواژهٔ امن"],"creates":"application_and_initial_deployment","installs_packages":true,"creates_services":true}
        ],
        "note":"قالب WordPress نسخهٔ هستهٔ رسمی را دریافت می‌کند، پیش‌نیازهای PHP-FPM و MariaDB را آماده می‌کند، دیتابیس جدا و پیکربندی امن می‌سازد، سایت را نصب می‌کند و Nginx و Health Check را اعمال می‌کند. صدور SSL فقط وقتی انجام می‌شود که DNS و دسترسی HTTP آماده باشد."
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
    if template_id == "wordpress-app"
        && !apps::valid_wordpress_version(input.wordpress_version.trim())
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نسخهٔ WordPress باید یک نسخهٔ پایدار کامل مانند 7.1.4 باشد.",
        ));
    }
    if template_id == "wordpress-app" {
        if !apps::valid_domain(input.domain.trim()) || input.domain.trim().contains('*') {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "برای نصب خودکار WordPress یک دامنهٔ معتبر و بدون wildcard لازم است.",
            ));
        }
        wordpress::validate_admin_setup_fields(
            input.wordpress_site_title.trim(),
            input.wordpress_admin_username.trim(),
            input.wordpress_admin_email.trim(),
            &input.wordpress_admin_password,
        )
        .map_err(|message| error(StatusCode::BAD_REQUEST, message))?;
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

    #[test]
    fn wordpress_core_versions_are_stable_numeric_tags_only() {
        assert!(apps::valid_wordpress_version("7.1.4"));
        assert!(apps::valid_wordpress_version("6.8.3"));
        assert!(!apps::valid_wordpress_version("latest"));
        assert!(!apps::valid_wordpress_version("7.1"));
        assert!(!apps::valid_wordpress_version("7.1.4-rc1"));
        assert!(!apps::valid_wordpress_version("7.1.4;id"));
        assert!(!apps::valid_wordpress_version("07.1.4"));
    }

    #[test]
    fn wordpress_catalog_filters_unstable_and_duplicate_versions() {
        let versions = parse_wordpress_versions(&serde_json::json!({"offers":[
            {"version":"7.1.4"}, {"version":"7.1.4"}, {"version":"7.2-beta1"},
            {"version":"6.8.3"}, {"version":"latest"}
        ]}));
        assert_eq!(versions, vec!["7.1.4", "6.8.3"]);
    }
}
