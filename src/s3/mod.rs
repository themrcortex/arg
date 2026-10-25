use super::*;
use ::s3::{bucket::Bucket, bucket_ops::BucketConfiguration, creds::Credentials, Region};
use axum::{body::Body, http::header, response::Response};
use std::{
    net::{IpAddr, ToSocketAddrs},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
};
use url::Url;

const SECRET_DIR: &str = "/etc/arg/s3";
const MAX_PAGE_SIZE: usize = 1000;

#[derive(Serialize)]
struct S3ConnectionRecord {
    id: i64,
    display_name: String,
    description: String,
    provider_type: String,
    endpoint: String,
    region: String,
    address_style: String,
    allow_private_endpoint: bool,
    credential_present: bool,
    last_rotated_at: i64,
    status: String,
    last_test_at: Option<i64>,
    last_error: Option<String>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Deserialize)]
pub(super) struct ConnectionInput {
    display_name: String,
    #[serde(default)]
    description: String,
    provider_type: String,
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    region: String,
    #[serde(default)]
    access_key_id: Option<String>,
    #[serde(default)]
    secret_access_key: Option<String>,
    #[serde(default)]
    session_token: Option<String>,
    #[serde(default = "default_address_style")]
    address_style: String,
    #[serde(default)]
    allow_private_endpoint: bool,
}
fn default_address_style() -> String {
    "auto".into()
}
#[derive(Deserialize)]
pub(super) struct BucketInput {
    bucket_name: String,
    display_name: String,
    #[serde(default)]
    description: String,
}
#[derive(Deserialize)]
pub(super) struct CreateBucketInput {
    bucket_name: String,
    confirm_name: String,
}
#[derive(Deserialize)]
pub(super) struct DeleteConnectionInput {
    confirm_name: String,
}
#[derive(Deserialize)]
pub(super) struct TestInput {
    bucket_name: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SecretConfig {
    provider_type: String,
    endpoint: String,
    region: String,
    access_key_id: String,
    secret_access_key: String,
    session_token: Option<String>,
    address_style: String,
    allow_private_endpoint: bool,
}

#[derive(Deserialize)]
struct SecretConfigPatch {
    provider_type: String,
    endpoint: String,
    region: String,
    address_style: String,
    allow_private_endpoint: bool,
}

fn providers() -> Value {
    json!([
      {"id":"aws","label":"Amazon S3","rclone_provider":"AWS","endpoint_required":false,"region_required":true,"create_bucket":true,"docs":"https://docs.aws.amazon.com/AmazonS3/latest/userguide/Welcome.html"},
      {"id":"cloudflare_r2","label":"Cloudflare R2","rclone_provider":"Cloudflare","endpoint_required":true,"region_required":false,"region_default":"auto","create_bucket":true,"docs":"https://developers.cloudflare.com/r2/get-started/s3/"},
      {"id":"arvancloud","label":"ابر آروان · ArvanCloud","rclone_provider":"ArvanCloud","endpoint_required":true,"region_required":true,"create_bucket":false,"docs":"https://docs.arvancloud.ir/fa/developer-tools/sdk/object-storage/","endpoints":[{"id":"simin","label":"سیمین","endpoint":"https://s3.ir-thr-at1.arvanstorage.ir"},{"id":"shahriar","label":"شهریار","endpoint":"https://s3.ir-tbz-sh1.arvanstorage.ir"}]},
      {"id":"parspack","label":"پارس‌پک · ParsPack","rclone_provider":"Other","endpoint_required":true,"region_required":false,"create_bucket":false,"docs":"https://docs.parspack.com/cloud-storage/"},
      {"id":"backblaze_b2","label":"Backblaze B2 S3","rclone_provider":"Other","endpoint_required":true,"region_required":true,"create_bucket":false,"docs":"https://www.backblaze.com/docs/cloud-storage-s3-compatible-api"},
      {"id":"digitalocean_spaces","label":"DigitalOcean Spaces","rclone_provider":"DigitalOcean","endpoint_required":true,"region_required":true,"create_bucket":true,"docs":"https://docs.digitalocean.com/products/spaces/"},
      {"id":"wasabi","label":"Wasabi","rclone_provider":"Wasabi","endpoint_required":true,"region_required":true,"create_bucket":true,"docs":"https://docs.wasabi.com/"},
      {"id":"minio","label":"MinIO","rclone_provider":"Minio","endpoint_required":true,"region_required":false,"create_bucket":true,"docs":"https://min.io/docs/minio/linux/developers/s3-api.html"},
      {"id":"generic","label":"S3-compatible / Custom","rclone_provider":"Other","endpoint_required":true,"region_required":false,"create_bucket":false,"docs":"https://rclone.org/s3/"}
    ])
}

fn provider_type_valid(value: &str) -> bool {
    [
        "aws",
        "cloudflare_r2",
        "arvancloud",
        "parspack",
        "backblaze_b2",
        "digitalocean_spaces",
        "wasabi",
        "minio",
        "generic",
    ]
    .contains(&value)
}
fn provider_can_create_bucket(value: &str) -> bool {
    [
        "aws",
        "cloudflare_r2",
        "digitalocean_spaces",
        "wasabi",
        "minio",
    ]
    .contains(&value)
}
fn valid_bucket_name(name: &str) -> bool {
    let b = name.as_bytes();
    if !(3..=63).contains(&b.len())
        || !b[0].is_ascii_lowercase() && !b[0].is_ascii_digit()
        || !b[b.len() - 1].is_ascii_lowercase() && !b[b.len() - 1].is_ascii_digit()
    {
        return false;
    }
    if !b.iter().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'.' || *byte == b'-'
    }) || name.contains("..")
        || name.contains(".-")
        || name.contains("-.")
    {
        return false;
    }
    if name.parse::<std::net::Ipv4Addr>().is_ok() {
        return false;
    }
    true
}

fn validate_endpoint_syntax(endpoint: &str) -> Result<Url, String> {
    let url = Url::parse(endpoint).map_err(|_| "آدرس Endpoint معتبر نیست.".to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !["", "/"].contains(&url.path())
    {
        return Err("Endpoint باید یک HTTPS origin بدون مسیر، پارامتر یا اطلاعات ورود باشد.".into());
    }
    Ok(url)
}

fn validate_endpoint(endpoint: &str, allow_private: bool) -> Result<(), String> {
    let url = validate_endpoint_syntax(endpoint)?;
    let host = url.host_str().unwrap();
    if matches!(
        host.to_ascii_lowercase().as_str(),
        "metadata.google.internal" | "metadata" | "instance-data.ec2.internal"
    ) {
        return Err("Endpointهای metadata سرور قابل استفاده نیستند.".into());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses: Vec<IpAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![ip]
    } else {
        (host, port)
            .to_socket_addrs()
            .map_err(|_| "DNS مربوط به Endpoint پاسخ نداد.".to_string())?
            .map(|address| address.ip())
            .collect()
    };
    if addresses.is_empty() {
        return Err("Endpoint به هیچ نشانی IP نگاشت نشد.".into());
    }
    for address in addresses {
        let metadata_ip = address == IpAddr::V4(std::net::Ipv4Addr::new(169, 254, 169, 254));
        if metadata_ip || (!allow_private && !metrics::is_public_ip(&address.to_string())) {
            return Err(
                "Endpoint خصوصی است. فقط پس از بررسی مقصد و تأیید دسترسی خصوصی آن را فعال کن."
                    .into(),
            );
        }
    }
    Ok(())
}

fn input_error(input: &ConnectionInput, is_create: bool) -> Option<String> {
    if input.display_name.trim().is_empty()
        || input.display_name.len() > 96
        || input.description.len() > 1000
        || input.display_name.chars().any(char::is_control)
        || input.description.chars().any(char::is_control)
    {
        return Some("نام یا توضیحات اتصال معتبر نیست.".into());
    }
    if !provider_type_valid(&input.provider_type) {
        return Some("ارائه‌دهندهٔ S3 پشتیبانی نمی‌شود.".into());
    }
    if !["auto", "path", "virtual"].contains(&input.address_style.as_str()) {
        return Some("Addressing Style معتبر نیست.".into());
    }
    let endpoint_required = input.provider_type != "aws";
    if endpoint_required && input.endpoint.is_empty() {
        return Some("Endpoint را از پنل ارائه‌دهنده وارد کن.".into());
    }
    if !input.endpoint.is_empty() {
        if let Err(message) = validate_endpoint_syntax(&input.endpoint) {
            return Some(message);
        }
    }
    if [
        "arvancloud",
        "aws",
        "backblaze_b2",
        "digitalocean_spaces",
        "wasabi",
    ]
    .contains(&input.provider_type.as_str())
        && input.region.trim().is_empty()
    {
        return Some("Region یا Signing Region باید طبق اطلاعات سرویس‌دهنده وارد شود.".into());
    }
    let credentials = input.access_key_id.as_deref().unwrap_or("");
    let secret = input.secret_access_key.as_deref().unwrap_or("");
    if is_create && (credentials.is_empty() || secret.is_empty()) {
        return Some("Access Key ID و Secret Access Key الزامی هستند.".into());
    }
    if credentials.len() > 256
        || secret.len() > 512
        || input
            .session_token
            .as_deref()
            .is_some_and(|value| value.len() > 4096)
    {
        return Some("طول اطلاعات دسترسی بیش از حد مجاز است.".into());
    }
    if [
        credentials,
        secret,
        input.session_token.as_deref().unwrap_or(""),
    ]
    .iter()
    .any(|value| value.bytes().any(|byte| byte.is_ascii_control()))
    {
        return Some("اطلاعات دسترسی شامل نویسهٔ کنترلی نامعتبر است.".into());
    }
    if input.access_key_id.is_some() != input.secret_access_key.is_some() {
        return Some("برای چرخش کلید، Access Key و Secret Key را باهم وارد کن.".into());
    }
    None
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS s3_connections (id INTEGER PRIMARY KEY,display_name TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',provider_type TEXT NOT NULL,endpoint TEXT NOT NULL DEFAULT '',region TEXT NOT NULL DEFAULT '',address_style TEXT NOT NULL DEFAULT 'auto',allow_private_endpoint INTEGER NOT NULL DEFAULT 0,credential_present INTEGER NOT NULL DEFAULT 1,last_rotated_at INTEGER NOT NULL,status TEXT NOT NULL DEFAULT 'unverified',last_test_at INTEGER,last_error TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS s3_buckets (id INTEGER PRIMARY KEY,connection_id INTEGER NOT NULL REFERENCES s3_connections(id) ON DELETE CASCADE,bucket_name TEXT NOT NULL,display_name TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',enabled INTEGER NOT NULL DEFAULT 1,verified_at INTEGER,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,UNIQUE(connection_id,bucket_name)); CREATE INDEX IF NOT EXISTS s3_buckets_connection ON s3_buckets(connection_id); CREATE TABLE IF NOT EXISTS s3_usage_snapshots (id INTEGER PRIMARY KEY,bucket_id INTEGER NOT NULL REFERENCES s3_buckets(id) ON DELETE CASCADE,used_bytes INTEGER,object_count INTEGER,quota_bytes INTEGER,quota_source TEXT,usage_source TEXT NOT NULL DEFAULT 'unknown',scan_status TEXT NOT NULL DEFAULT 'not_measured',measured_at INTEGER,error TEXT); CREATE INDEX IF NOT EXISTS s3_usage_bucket_time ON s3_usage_snapshots(bucket_id,measured_at DESC); CREATE TABLE IF NOT EXISTS s3_mounts(id INTEGER PRIMARY KEY,bucket_id INTEGER NOT NULL REFERENCES s3_buckets(id) ON DELETE CASCADE,method TEXT NOT NULL,mount_path TEXT NOT NULL UNIQUE,read_only INTEGER NOT NULL DEFAULT 1,auto_mount INTEGER NOT NULL DEFAULT 0,cache_config_json TEXT NOT NULL DEFAULT '{}',status TEXT NOT NULL DEFAULT 'unmounted',last_error TEXT,updated_at INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS s3_contracts(id INTEGER PRIMARY KEY,title TEXT NOT NULL,seller TEXT,contract_ref TEXT,billing_interval_months INTEGER NOT NULL DEFAULT 1,billing_calendar TEXT NOT NULL DEFAULT 'gregorian',billing_anchor_day INTEGER NOT NULL,timezone TEXT NOT NULL DEFAULT 'Asia/Tehran',start_date TEXT NOT NULL,next_due_date TEXT NOT NULL,end_date TEXT,amount_minor_units INTEGER,currency TEXT,included_quota_bytes INTEGER,auto_renew_recorded INTEGER NOT NULL DEFAULT 0,status TEXT NOT NULL DEFAULT 'active',notes TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);")
}

fn read_connection(
    db: &Connection,
    id: i64,
) -> Result<S3ConnectionRecord, (StatusCode, Json<Value>)> {
    db.query_row("SELECT id,display_name,description,provider_type,endpoint,region,address_style,allow_private_endpoint,credential_present,last_rotated_at,status,last_test_at,last_error,created_at,updated_at FROM s3_connections WHERE id=?1", [id], |row| Ok(S3ConnectionRecord { id:row.get(0)?,display_name:row.get(1)?,description:row.get(2)?,provider_type:row.get(3)?,endpoint:row.get(4)?,region:row.get(5)?,address_style:row.get(6)?,allow_private_endpoint:row.get::<_,i64>(7)? != 0,credential_present:row.get::<_,i64>(8)? != 0,last_rotated_at:row.get(9)?,status:row.get(10)?,last_test_at:row.get(11)?,last_error:row.get(12)?,created_at:row.get(13)?,updated_at:row.get(14)?})).map_err(|_| error(StatusCode::NOT_FOUND, "اتصال S3 پیدا نشد."))
}
fn read_secret_file(id: i64) -> Result<SecretConfig, String> {
    if id <= 0 {
        return Err("شناسهٔ اتصال معتبر نیست.".into());
    }
    let path = PathBuf::from(SECRET_DIR).join(format!("{id}.json"));
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| "اطلاعات امن اتصال وجود ندارد؛ کلید را دوباره ذخیره کن.".to_string())?;
    if !metadata.file_type().is_file()
        || metadata.uid() != 0
        || metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err("سطح دسترسی فایل کلید S3 ناامن است.".into());
    }
    let secret: SecretConfig = serde_json::from_slice(
        &fs::read(path).map_err(|_| "خواندن کلید اتصال ناموفق بود.".to_string())?,
    )
    .map_err(|_| "فایل کلید اتصال معتبر نیست.".to_string())?;
    if !secret.endpoint.is_empty() {
        validate_endpoint(&secret.endpoint, secret.allow_private_endpoint)?;
    }
    Ok(secret)
}

fn secret_path(id: i64) -> Result<PathBuf, String> {
    if id <= 0 {
        return Err("شناسهٔ اتصال معتبر نیست.".into());
    }
    Ok(PathBuf::from(SECRET_DIR).join(format!("{id}.json")))
}

fn write_secret_file(id: i64, secret: &SecretConfig) -> Result<(), String> {
    if !secret.endpoint.is_empty() {
        validate_endpoint(&secret.endpoint, secret.allow_private_endpoint)?;
    }
    let path = secret_path(id)?;
    fs::create_dir_all(SECRET_DIR).map_err(|_| "ساخت محل امن کلیدها ناموفق بود.".to_string())?;
    fs::set_permissions(SECRET_DIR, fs::Permissions::from_mode(0o700))
        .map_err(|_| "تنظیم دسترسی پوشهٔ کلیدها ناموفق بود.".to_string())?;
    let temp = PathBuf::from(SECRET_DIR).join(format!(".{id}.tmp-{}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|_| "فایل موقت امن ساخته نشد.".to_string())?;
    file.write_all(
        &serde_json::to_vec(secret).map_err(|_| "رمزگذاری اطلاعات اتصال ناموفق بود.".to_string())?,
    )
    .map_err(|_| "نوشتن کلید امن ناموفق بود.".to_string())?;
    file.sync_all()
        .map_err(|_| "ذخیرهٔ امن کلید ناموفق بود.".to_string())?;
    fs::rename(&temp, &path).map_err(|_| {
        let _ = fs::remove_file(&temp);
        "ثبت امن کلید ناموفق بود.".to_string()
    })?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|_| "تنظیم دسترسی کلید ناموفق بود.".to_string())?;
    Ok(())
}

fn s3_region(secret: &SecretConfig) -> Result<Region, String> {
    if secret.endpoint.is_empty() {
        return secret
            .region
            .parse::<Region>()
            .map_err(|_| "Region AWS معتبر نیست.".into());
    }
    Ok(Region::Custom {
        region: secret.region.clone(),
        endpoint: secret.endpoint.clone(),
    })
}
fn s3_credentials(secret: &SecretConfig) -> Result<Credentials, String> {
    Credentials::new(
        Some(&secret.access_key_id),
        Some(&secret.secret_access_key),
        secret.session_token.as_deref(),
        None,
        None,
    )
    .map_err(|_| "اطلاعات کلید S3 معتبر نیست.".into())
}
fn make_bucket<'a>(name: &'a str, secret: &SecretConfig) -> Result<Box<Bucket>, String> {
    let region = s3_region(secret)?;
    let credentials = s3_credentials(secret)?;
    let bucket = Bucket::new(name, region, credentials)
        .map_err(|_| "ساخت S3 client ناموفق بود.".to_string())?;
    Ok(
        if secret.address_style == "path"
            || secret.address_style == "auto" && secret.provider_type != "aws"
        {
            bucket.with_path_style()
        } else {
            bucket
        },
    )
}

fn helper_error(err: &::s3::error::S3Error) -> String {
    let message = err.to_string().to_ascii_lowercase();
    if message.contains("accessdenied")
        || message.contains("access denied")
        || message.contains("403")
    {
        "ارائه‌دهنده دسترسی این عملیات را رد کرد؛ مجوز Bucket را بررسی کن.".into()
    } else if message.contains("nosuchbucket")
        || message.contains("no such bucket")
        || message.contains("404")
    {
        "Bucket پیدا نشد؛ نام و Endpoint را بررسی کن.".into()
    } else if message.contains("signaturedoesnotmatch")
        || message.contains("invalidaccesskeyid")
        || message.contains("signature")
    {
        "احراز هویت یا Signing Region با سرویس‌دهنده سازگار نیست.".into()
    } else if message.contains("certificate") || message.contains("tls") {
        "اعتبارسنجی گواهی TLS ناموفق بود.".into()
    } else {
        "اتصال S3 ناموفق بود؛ DNS، Endpoint، Region و وضعیت سرویس را بررسی کن.".into()
    }
}

pub(super) fn execute_privileged_request(action: &str, request: &Value) -> Value {
    let id = request["id"].as_i64().unwrap_or(0);
    match action {
        "s3-secret-write" => {
            let Some(secret) =
                serde_json::from_value::<SecretConfig>(request["secret"].clone()).ok()
            else {
                return json!({"ok":false,"error":"اطلاعات کلید اتصال معتبر نیست."});
            };
            match write_secret_file(id, &secret) {
                Ok(()) => json!({"ok":true}),
                Err(error) => json!({"ok":false,"error":error}),
            }
        }
        "s3-secret-update-config" => {
            let Some(patch) =
                serde_json::from_value::<SecretConfigPatch>(request["config"].clone()).ok()
            else {
                return json!({"ok":false,"error":"تنظیمات اتصال معتبر نیست."});
            };
            let mut secret = match read_secret_file(id) {
                Ok(secret) => secret,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            secret.provider_type = patch.provider_type;
            secret.endpoint = patch.endpoint;
            secret.region = patch.region;
            secret.address_style = patch.address_style;
            secret.allow_private_endpoint = patch.allow_private_endpoint;
            match write_secret_file(id, &secret) {
                Ok(()) => json!({"ok":true}),
                Err(error) => json!({"ok":false,"error":error}),
            }
        }
        "s3-secret-delete" => match secret_path(id).and_then(|path| {
            fs::remove_file(path).map_err(|_| "حذف کلید امن ناموفق بود.".to_string())
        }) {
            Ok(()) => json!({"ok":true}),
            Err(error) => json!({"ok":false,"error":error}),
        },
        "s3-list-buckets" | "s3-test" | "s3-create-bucket" | "s3-list-objects"
        | "s3-delete-object" | "s3-upload-object" | "s3-download-object" => {
            let secret = match read_secret_file(id) {
                Ok(secret) => secret,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(_) => return json!({"ok":false,"error":"S3 worker could not start."}),
            };
            runtime.block_on(async { execute_network_action(action, request, &secret).await })
        }
        _ => json!({"ok":false,"error":"Unsupported S3 operation"}),
    }
}

async fn execute_network_action(action: &str, request: &Value, secret: &SecretConfig) -> Value {
    match action {
        "s3-list-buckets" => {
            let (Ok(region), Ok(credentials)) = (s3_region(secret), s3_credentials(secret)) else {
                return json!({"ok":false,"error":"تنظیمات S3 معتبر نیست."});
            };
            match Bucket::list_buckets(region, credentials).await {
                Ok(response) => {
                    json!({"ok":true,"buckets":response.bucket_names().collect::<Vec<_>>()})
                }
                Err(error) => json!({"ok":false,"error":helper_error(&error)}),
            }
        }
        "s3-test" => {
            let Some(name) = request["bucket"]
                .as_str()
                .filter(|value| valid_bucket_name(value))
            else {
                return json!({"ok":false,"error":"نام Bucket معتبر نیست."});
            };
            let bucket = match make_bucket(name, secret) {
                Ok(bucket) => bucket,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            match bucket
                .list_page(String::new(), Some("/".into()), None, None, Some(1))
                .await
            {
                Ok((_, _)) => {
                    json!({"ok":true,"status":"connected","capabilities":{"list_objects":true,"create_bucket":false},"message":"ارتباط و مجوز فهرست‌کردن Objectها تأیید شد."})
                }
                Err(error) => {
                    let message = helper_error(&error);
                    let endpoint_reached = message.contains("دسترسی این عملیات را رد کرد")
                        || message.contains("احراز هویت یا Signing Region")
                        || message.contains("Bucket پیدا نشد");
                    if endpoint_reached {
                        json!({"ok":true,"status":"degraded","capabilities":{"list_objects":false,"create_bucket":false},"message":message})
                    } else {
                        json!({"ok":false,"error":message})
                    }
                }
            }
        }
        "s3-create-bucket" => {
            let Some(name) = request["bucket"]
                .as_str()
                .filter(|value| valid_bucket_name(value))
            else {
                return json!({"ok":false,"error":"نام Bucket معتبر نیست."});
            };
            if !provider_can_create_bucket(&secret.provider_type) {
                return json!({"ok":false,"error":"ساخت Bucket برای این Preset تا زمان تأیید پشتیبانی ارائه‌دهنده غیرفعال است."});
            }
            let (Ok(region), Ok(credentials)) = (s3_region(secret), s3_credentials(secret)) else {
                return json!({"ok":false,"error":"تنظیمات S3 معتبر نیست."});
            };
            match Bucket::create_with_path_style(
                name,
                region,
                credentials,
                BucketConfiguration::default(),
            )
            .await
            {
                Ok(_) => json!({"ok":true}),
                Err(error) => json!({"ok":false,"error":helper_error(&error)}),
            }
        }
        "s3-list-objects" => {
            let Some(name) = request["bucket"]
                .as_str()
                .filter(|value| valid_bucket_name(value))
            else {
                return json!({"ok":false,"error":"نام Bucket معتبر نیست."});
            };
            let prefix = request["prefix"].as_str().unwrap_or("");
            let token = request["continuation_token"]
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            let max_keys = request["max_keys"]
                .as_u64()
                .unwrap_or(200)
                .clamp(1, MAX_PAGE_SIZE as u64) as usize;
            if prefix.len() > 1024 || prefix.chars().any(char::is_control) {
                return json!({"ok":false,"error":"Prefix معتبر نیست."});
            }
            let bucket = match make_bucket(name, secret) {
                Ok(bucket) => bucket,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            match bucket
                .list_page(
                    prefix.to_owned(),
                    Some("/".into()),
                    token,
                    None,
                    Some(max_keys),
                )
                .await
            {
                Ok((page, _)) => {
                    let key_count =
                        page.contents.len() + page.common_prefixes.as_ref().map_or(0, Vec::len);
                    let objects=page.contents.into_iter().map(|object| json!({"key":object.key,"size_bytes":object.size,"last_modified":object.last_modified,"etag":object.e_tag,"storage_class":object.storage_class})).collect::<Vec<_>>();
                    let prefixes = page
                        .common_prefixes
                        .unwrap_or_default()
                        .into_iter()
                        .map(|item| item.prefix)
                        .collect::<Vec<_>>();
                    json!({"ok":true,"objects":objects,"prefixes":prefixes,"is_truncated":page.is_truncated,"next_token":page.next_continuation_token,"key_count":key_count})
                }
                Err(error) => json!({"ok":false,"error":helper_error(&error)}),
            }
        }
        "s3-delete-object" => {
            let (Some(name), Some(key)) = (
                request["bucket"]
                    .as_str()
                    .filter(|value| valid_bucket_name(value)),
                request["key"].as_str().filter(|value| {
                    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
                }),
            ) else {
                return json!({"ok":false,"error":"Bucket یا Object Key معتبر نیست."});
            };
            if request["confirm_key"].as_str() != Some(key) {
                return json!({"ok":false,"error":"برای حذف Object، نام کلید را دقیقاً تأیید کن."});
            }
            let bucket = match make_bucket(name, secret) {
                Ok(bucket) => bucket,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            match bucket.delete_object(key).await {
                Ok(_) => json!({"ok":true}),
                Err(error) => json!({"ok":false,"error":helper_error(&error)}),
            }
        }
        "s3-upload-object" => {
            let (Some(name), Some(key), Some(path)) = (
                request["bucket"]
                    .as_str()
                    .filter(|value| valid_bucket_name(value)),
                request["key"]
                    .as_str()
                    .filter(|value| valid_object_key(value)),
                request["transfer_path"]
                    .as_str()
                    .filter(|value| valid_transfer_path(value)),
            ) else {
                return json!({"ok":false,"error":"مشخصات بارگذاری معتبر نیست."});
            };
            let content_type = request["content_type"]
                .as_str()
                .filter(|value| {
                    value.len() <= 128 && value.bytes().all(|byte| byte.is_ascii_graphic())
                })
                .unwrap_or("application/octet-stream");
            let metadata = match fs::symlink_metadata(path) {
                Ok(value) => value,
                Err(_) => return json!({"ok":false,"error":"فایل موقت بارگذاری پیدا نشد."}),
            };
            if !metadata.file_type().is_file()
                || metadata.uid() == 0
                || metadata.permissions().mode() & 0o777 != 0o600
                || metadata.len() > 512 * 1024 * 1024
            {
                return json!({"ok":false,"error":"فایل موقت بارگذاری ناامن یا بزرگ‌تر از حد مجاز است."});
            }
            let bucket = match make_bucket(name, secret) {
                Ok(bucket) => bucket,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            let mut file = match tokio::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path)
                .await
            {
                Ok(file) => file,
                Err(_) => {
                    return json!({"ok":false,"error":"خواندن فایل موقت بارگذاری ناموفق بود."})
                }
            };
            match bucket
                .put_object_stream_with_content_type(&mut file, key, content_type)
                .await
            {
                Ok(response) if response.status_code() < 300 => {
                    json!({"ok":true,"size_bytes":metadata.len()})
                }
                Ok(_) => json!({"ok":false,"error":"ارائه‌دهنده بارگذاری را تأیید نکرد."}),
                Err(error) => json!({"ok":false,"error":helper_error(&error)}),
            }
        }
        "s3-download-object" => {
            let (Some(name), Some(key), Some(path)) = (
                request["bucket"]
                    .as_str()
                    .filter(|value| valid_bucket_name(value)),
                request["key"]
                    .as_str()
                    .filter(|value| valid_object_key(value)),
                request["transfer_path"]
                    .as_str()
                    .filter(|value| valid_transfer_path(value)),
            ) else {
                return json!({"ok":false,"error":"مشخصات دانلود معتبر نیست."});
            };
            let metadata = match fs::symlink_metadata(path) {
                Ok(value) => value,
                Err(_) => return json!({"ok":false,"error":"فایل مقصد موقت پیدا نشد."}),
            };
            if !metadata.file_type().is_file()
                || metadata.uid() == 0
                || metadata.permissions().mode() & 0o777 != 0o600
                || metadata.len() != 0
            {
                return json!({"ok":false,"error":"فایل مقصد موقت ناامن است."});
            }
            let bucket = match make_bucket(name, secret) {
                Ok(bucket) => bucket,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            let mut response = match bucket.get_object_stream(key).await {
                Ok(response) => response,
                Err(error) => return json!({"ok":false,"error":helper_error(&error)}),
            };
            if response.status_code >= 300 {
                return json!({"ok":false,"error":"دریافت Object از ارائه‌دهنده تأیید نشد."});
            }
            let mut file = match tokio::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path)
                .await
            {
                Ok(file) => file,
                Err(_) => {
                    return json!({"ok":false,"error":"آماده‌سازی فایل موقت دانلود ناموفق بود."})
                }
            };
            let mut size = 0u64;
            while let Some(chunk) = response.bytes.next().await {
                let Ok(chunk) = chunk else {
                    return json!({"ok":false,"error":"انتقال Object از ارائه‌دهنده قطع شد."});
                };
                size = size.saturating_add(chunk.len() as u64);
                if size > 2 * 1024 * 1024 * 1024 {
                    return json!({"ok":false,"error":"حجم Object از سقف دانلود 2 GiB بیشتر است."});
                }
                if file.write_all(&chunk).await.is_err() {
                    return json!({"ok":false,"error":"نوشتن فایل دانلود ناموفق بود."});
                }
            }
            let _ = file.sync_all().await;
            json!({"ok":true,"size_bytes":size})
        }
        _ => json!({"ok":false,"error":"Unsupported S3 network action"}),
    }
}

fn valid_object_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 1024 && !key.chars().any(char::is_control)
}
fn valid_transfer_path(path: &str) -> bool {
    path.starts_with("/tmp/arg-s3-transfer-")
        && path[5..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_iranian_presets_keep_independent_provider_ids() {
        let list = providers();
        let ids = list
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["id"].as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"arvancloud"));
        assert!(ids.contains(&"parspack"));
        assert_eq!(
            list.as_array()
                .unwrap()
                .iter()
                .find(|item| item["id"] == "parspack")
                .unwrap()["endpoint_required"],
            true
        );
    }

    #[test]
    fn bucket_names_and_endpoint_origins_are_validated() {
        assert!(valid_bucket_name("arg-backups-2026"));
        assert!(!valid_bucket_name("BadBucket"));
        assert!(!valid_bucket_name("127.0.0.1"));
        assert!(validate_endpoint_syntax("https://objects.example.net").is_ok());
        assert!(validate_endpoint_syntax("http://objects.example.net").is_err());
        assert!(validate_endpoint_syntax("https://user:pass@objects.example.net").is_err());
        assert!(validate_endpoint_syntax("https://objects.example.net/path").is_err());
    }

    #[test]
    fn transfer_paths_cannot_escape_the_private_temp_name() {
        assert!(valid_transfer_path("/tmp/arg-s3-transfer-a1b2-c3d4"));
        assert!(!valid_transfer_path(
            "/tmp/arg-s3-transfer-../../etc/passwd"
        ));
        assert!(!valid_transfer_path("/etc/arg/s3/1.json"));
    }
}

pub(super) async fn list_providers(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    Ok(Json(json!({"providers":providers()})))
}
pub(super) async fn summary(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    let (connections, healthy, buckets, mounted): (i64,i64,i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM s3_connections),(SELECT COUNT(*) FROM s3_connections WHERE status='connected'),(SELECT COUNT(*) FROM s3_buckets WHERE enabled=1),(SELECT COUNT(*) FROM s3_mounts WHERE status='mounted')", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap_or((0,0,0,0));
    let due_soon: i64 = db.query_row("SELECT COUNT(*) FROM s3_contracts WHERE status IN ('active','upcoming') AND next_due_date BETWEEN date('now') AND date('now','+7 days')", [], |row| row.get(0)).unwrap_or(0);
    Ok(Json(
        json!({"connections":connections,"healthy_connections":healthy,"buckets":buckets,"mounted_buckets":mounted,"contracts_due_soon":due_soon,"usage_bytes":null,"quota_bytes":null,"usage_status":"not_measured"}),
    ))
}
pub(super) async fn list_connections(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let mut statement = db
        .prepare("SELECT id FROM s3_connections ORDER BY updated_at DESC,id DESC")
        .map_err(db_error)?;
    let ids: Vec<i64> = statement
        .query_map([], |row| row.get(0))
        .map_err(db_error)?
        .filter_map(Result::ok)
        .collect();
    let connections: Vec<S3ConnectionRecord> = ids
        .into_iter()
        .filter_map(|id| read_connection(&db, id).ok())
        .collect();
    Ok(Json(json!({"connections":connections})))
}
pub(super) async fn get_connection(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    Ok(Json(json!({"connection":read_connection(&db,id)?})))
}
pub(super) async fn create_connection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ConnectionInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    if let Some(message) = input_error(&input, true) {
        return Err(error(StatusCode::BAD_REQUEST, &message));
    }
    let access_key = input.access_key_id.clone().unwrap_or_default();
    let secret_key = input.secret_access_key.clone().unwrap_or_default();
    let token = input.session_token.clone();
    let now = now();
    let id = {
        let db = state.lock().unwrap();
        db.execute("INSERT INTO s3_connections(display_name,description,provider_type,endpoint,region,address_style,allow_private_endpoint,credential_present,last_rotated_at,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,1,?8,'unverified',?8,?8)",params![input.display_name.trim(),input.description,input.provider_type,input.endpoint,input.region,input.address_style,input.allow_private_endpoint,now]).map_err(db_error)?;
        db.last_insert_rowid()
    };
    let secret = SecretConfig {
        provider_type: input.provider_type.clone(),
        endpoint: input.endpoint.clone(),
        region: input.region.clone(),
        access_key_id: access_key,
        secret_access_key: secret_key,
        session_token: token,
        address_style: input.address_style.clone(),
        allow_private_endpoint: input.allow_private_endpoint,
    };
    let stored =
        privileged_request_async(json!({"action":"s3-secret-write","id":id,"secret":secret}))
            .await?;
    if stored["ok"] != true {
        let db = state.lock().unwrap();
        let _ = db.execute("DELETE FROM s3_connections WHERE id=?1", [id]);
        return Err(error(
            StatusCode::BAD_GATEWAY,
            stored["error"]
                .as_str()
                .unwrap_or("ثبت امن کلید ناموفق بود."),
        ));
    }
    {
        let db = state.lock().unwrap();
        audit(
            &db,
            &actor.username,
            "s3.connection.create",
            &format!("id={id} provider={}", input.provider_type),
        );
    }
    let db = state.lock().unwrap();
    Ok(Json(json!({"connection":read_connection(&db,id)?})))
}
pub(super) async fn update_connection(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<ConnectionInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    if let Some(message) = input_error(&input, false) {
        return Err(error(StatusCode::BAD_REQUEST, &message));
    }
    let old = {
        let db = state.lock().unwrap();
        read_connection(&db, id)?
    };
    let rotated = input.access_key_id.is_some();
    let now = now();
    let stored = if rotated {
        let secret = SecretConfig {
            provider_type: input.provider_type.clone(),
            endpoint: input.endpoint.clone(),
            region: input.region.clone(),
            access_key_id: input.access_key_id.unwrap_or_default(),
            secret_access_key: input.secret_access_key.unwrap_or_default(),
            session_token: input.session_token,
            address_style: input.address_style.clone(),
            allow_private_endpoint: input.allow_private_endpoint,
        };
        let stored =
            privileged_request_async(json!({"action":"s3-secret-write","id":id,"secret":secret}))
                .await?;
        stored
    } else {
        privileged_request_async(json!({"action":"s3-secret-update-config","id":id,"config":{"provider_type":input.provider_type.clone(),"endpoint":input.endpoint.clone(),"region":input.region.clone(),"address_style":input.address_style.clone(),"allow_private_endpoint":input.allow_private_endpoint}})).await?
    };
    if stored["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            stored["error"]
                .as_str()
                .unwrap_or("ذخیرهٔ امن تنظیمات اتصال ناموفق بود."),
        ));
    }
    {
        let db = state.lock().unwrap();
        db.execute("UPDATE s3_connections SET display_name=?1,description=?2,provider_type=?3,endpoint=?4,region=?5,address_style=?6,allow_private_endpoint=?7,status=CASE WHEN endpoint<>?4 OR region<>?5 OR provider_type<>?3 THEN 'unverified' ELSE status END,last_error=CASE WHEN endpoint<>?4 OR region<>?5 OR provider_type<>?3 THEN NULL ELSE last_error END,last_rotated_at=CASE WHEN ?8 THEN ?9 ELSE last_rotated_at END,updated_at=?9 WHERE id=?10",params![input.display_name.trim(),input.description,input.provider_type,input.endpoint,input.region,input.address_style,input.allow_private_endpoint,rotated,now,id]).map_err(db_error)?;
    }
    {
        let db = state.lock().unwrap();
        audit(
            &db,
            &actor.username,
            "s3.connection.update",
            &format!("id={id} credentials_rotated={rotated}"),
        );
    }
    let db = state.lock().unwrap();
    Ok(Json(
        json!({"connection":read_connection(&db,id)?,"previous_provider":old.provider_type}),
    ))
}
pub(super) async fn delete_connection(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<DeleteConnectionInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    let connection = {
        let db = state.lock().unwrap();
        read_connection(&db, id)?
    };
    if input.confirm_name != connection.display_name {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "برای قطع اتصال، نام نمایشی آن را دقیقاً وارد کن.",
        ));
    }
    {
        let db = state.lock().unwrap();
        db.execute("DELETE FROM s3_buckets WHERE connection_id=?1", [id])
            .map_err(db_error)?;
        db.execute("DELETE FROM s3_connections WHERE id=?1", [id])
            .map_err(db_error)?;
        audit(
            &db,
            &actor.username,
            "s3.connection.delete",
            &format!("id={id}; remote data untouched"),
        );
    }
    let _ = privileged_request_async(json!({"action":"s3-secret-delete","id":id})).await;
    Ok(Json(json!({"ok":true,"remote_bucket_data_deleted":false})))
}
pub(super) async fn test_connection(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<TestInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    let connection = {
        let db = state.lock().unwrap();
        read_connection(&db, id)?
    };
    let bucket = input.bucket_name.or_else(|| {
        let db = state.lock().ok()?;
        db.query_row(
            "SELECT bucket_name FROM s3_buckets WHERE connection_id=?1 ORDER BY id LIMIT 1",
            [id],
            |row| row.get(0),
        )
        .ok()
    });
    let response = if let Some(ref bucket) = bucket {
        if !valid_bucket_name(&bucket) {
            return Err(error(StatusCode::BAD_REQUEST, "نام Bucket معتبر نیست."));
        }
        privileged_request_async(json!({"action":"s3-test","id":id,"bucket":bucket})).await?
    } else {
        privileged_request_async(json!({"action":"s3-list-buckets","id":id})).await?
    };
    let success = response["ok"] == true;
    let status = if !success {
        "failed"
    } else {
        response["status"].as_str().unwrap_or("connected")
    };
    let message = response["error"]
        .as_str()
        .or(response["message"].as_str())
        .unwrap_or("بررسی اتصال ناموفق بود.");
    {
        let db = state.lock().unwrap();
        db.execute("UPDATE s3_connections SET status=?1,last_test_at=?2,last_error=?3,updated_at=?2 WHERE id=?4",params![status,now(),if success {None::<String>} else {Some(message.to_string())},id]).map_err(db_error)?;
        audit(
            &db,
            &actor.username,
            "s3.connection.test",
            &format!(
                "id={id}; bucket={}",
                bucket.as_deref().unwrap_or("list-buckets")
            ),
        );
    }
    let _ = connection;
    Ok(Json(json!({"result":response})))
}
pub(super) async fn list_buckets(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
        let _ = read_connection(&db, id)?;
    }
    let provider_buckets = if query.get("local_only").is_some_and(|value| value == "1") {
        Value::Null
    } else {
        let remote = privileged_request_async(json!({"action":"s3-list-buckets","id":id})).await?;
        if remote["ok"] != true {
            return Err(error(
                StatusCode::BAD_GATEWAY,
                remote["error"]
                    .as_str()
                    .unwrap_or("فهرست Bucketها در دسترس نیست."),
            ));
        }
        remote["buckets"].clone()
    };
    let db = state.lock().unwrap();
    let mut statement=db.prepare("SELECT id,bucket_name,display_name,description,enabled,verified_at,created_at FROM s3_buckets WHERE connection_id=?1 ORDER BY display_name").map_err(db_error)?;
    let stored:Vec<Value>=statement.query_map([id],|row|Ok(json!({"id":row.get::<_,i64>(0)?,"bucket_name":row.get::<_,String>(1)?,"display_name":row.get::<_,String>(2)?,"description":row.get::<_,String>(3)?,"enabled":row.get::<_,bool>(4)?,"verified_at":row.get::<_,Option<i64>>(5)?,"created_at":row.get::<_,i64>(6)?}))).map_err(db_error)?.filter_map(Result::ok).collect();
    Ok(Json(
        json!({"provider_buckets":provider_buckets,"connections":stored}),
    ))
}
pub(super) async fn attach_bucket(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<BucketInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    if !valid_bucket_name(&input.bucket_name)
        || input.display_name.trim().is_empty()
        || input.display_name.len() > 96
        || input.description.len() > 1000
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام Bucket یا توضیحات معتبر نیست.",
        ));
    }
    let db = state.lock().unwrap();
    let connection = read_connection(&db, id)?;
    db.execute("INSERT INTO s3_buckets(connection_id,bucket_name,display_name,description,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?5)",params![id,input.bucket_name,input.display_name.trim(),input.description,now()]).map_err(|_|error(StatusCode::CONFLICT,"این Bucket قبلاً به اتصال اضافه شده است."))?;
    let bucket_id = db.last_insert_rowid();
    audit(
        &db,
        &actor.username,
        "s3.bucket.attach",
        &format!("connection={id} bucket_id={bucket_id}"),
    );
    Ok(Json(
        json!({"bucket":{"id":bucket_id,"connection_id":id,"bucket_name":input.bucket_name,"display_name":input.display_name,"verified_at":null},"provider_type":connection.provider_type}),
    ))
}
pub(super) async fn create_bucket(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<CreateBucketInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    if !valid_bucket_name(&input.bucket_name) || input.confirm_name != input.bucket_name {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام Bucket را دقیقاً تأیید کن.",
        ));
    }
    let connection = {
        let db = state.lock().unwrap();
        read_connection(&db, id)?
    };
    if !provider_can_create_bucket(&connection.provider_type) {
        return Err(error(
            StatusCode::CONFLICT,
            "ساخت Bucket برای این سرویس تا زمان تأیید capability آن غیرفعال است.",
        ));
    }
    let result = privileged_request_async(
        json!({"action":"s3-create-bucket","id":id,"bucket":input.bucket_name}),
    )
    .await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("ساخت Bucket ناموفق بود."),
        ));
    }
    let db = state.lock().unwrap();
    db.execute("INSERT INTO s3_buckets(connection_id,bucket_name,display_name,created_at,updated_at,verified_at) VALUES (?1,?2,?2,?3,?3,?3)",params![id,input.bucket_name,now()]).map_err(db_error)?;
    audit(
        &db,
        &actor.username,
        "s3.bucket.create",
        &format!("connection={id}; bucket={}", input.bucket_name),
    );
    Ok(Json(
        json!({"ok":true,"bucket_name":input.bucket_name,"remote_created":true}),
    ))
}
pub(super) async fn detach_bucket(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    let confirm = input["confirm_bucket"].as_str().unwrap_or("");
    let db = state.lock().unwrap();
    let bucket: String = db
        .query_row(
            "SELECT bucket_name FROM s3_buckets WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .map_err(|_| error(StatusCode::NOT_FOUND, "Bucket در ARG پیدا نشد."))?;
    if confirm != bucket {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "برای قطع اتصال، نام Bucket را دقیقاً تأیید کن.",
        ));
    }
    db.execute("DELETE FROM s3_buckets WHERE id=?1", [id])
        .map_err(db_error)?;
    audit(
        &db,
        &actor.username,
        "s3.bucket.detach",
        &format!("bucket_id={id}; remote data untouched"),
    );
    Ok(Json(json!({"ok":true,"remote_bucket_data_deleted":false})))
}
pub(super) async fn list_objects(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
        let _ = read_connection(&db, id)?;
    }
    let bucket = query.get("bucket").map(String::as_str).unwrap_or("");
    if !valid_bucket_name(bucket) {
        return Err(error(StatusCode::BAD_REQUEST, "نام Bucket معتبر نیست."));
    }
    let result=privileged_request_async(json!({"action":"s3-list-objects","id":id,"bucket":bucket,"prefix":query.get("prefix").map(String::as_str).unwrap_or(""),"continuation_token":query.get("continuation_token"),"max_keys":query.get("max_keys").and_then(|value|value.parse::<usize>().ok()).unwrap_or(200)})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("فهرست Objectها در دسترس نیست."),
        ));
    }
    Ok(Json(result))
}
pub(super) async fn delete_object(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    let result=privileged_request_async(json!({"action":"s3-delete-object","id":id,"bucket":input["bucket"],"key":input["key"],"confirm_key":input["confirm_key"]})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"].as_str().unwrap_or("حذف Object ناموفق بود."),
        ));
    }
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor.username,
        "s3.object.delete",
        &format!(
            "connection={id}; bucket={}; key_length={}",
            input["bucket"].as_str().unwrap_or(""),
            input["key"].as_str().unwrap_or("").len()
        ),
    );
    Ok(Json(json!({"ok":true})))
}

#[derive(Deserialize)]
pub(super) struct ObjectTransferQuery {
    bucket: String,
    key: String,
    #[serde(default)]
    content_type: Option<String>,
}

fn transfer_path() -> String {
    format!("/tmp/arg-s3-transfer-{}", token())
}

pub(super) async fn upload_object(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<ObjectTransferQuery>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?;
        let _ = read_connection(&db, id)?;
        actor
    };
    if !valid_bucket_name(&query.bucket) || !valid_object_key(&query.key) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام Bucket یا Object Key معتبر نیست.",
        ));
    }
    let path = transfer_path();
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "فایل موقت امن ساخته نشد.",
            )
        })?;
    let mut stream = body.into_data_stream();
    let mut total = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(_) => {
                drop(file);
                let _ = tokio::fs::remove_file(&path).await;
                return Err(error(StatusCode::BAD_REQUEST, "دریافت فایل کامل نشد."));
            }
        };
        total = total.saturating_add(chunk.len() as u64);
        if total > 512 * 1024 * 1024 {
            drop(file);
            let _ = tokio::fs::remove_file(&path).await;
            return Err(error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "حداکثر اندازه فایل برای بارگذاری 512 MiB است.",
            ));
        }
        if file.write_all(&chunk).await.is_err() {
            drop(file);
            let _ = tokio::fs::remove_file(&path).await;
            return Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ذخیره موقت فایل ناموفق بود.",
            ));
        }
    }
    if file.sync_all().await.is_err() {
        drop(file);
        let _ = tokio::fs::remove_file(&path).await;
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ذخیره موقت فایل کامل نشد.",
        ));
    }
    drop(file);
    let result=privileged_request_async(json!({"action":"s3-upload-object","id":id,"bucket":query.bucket,"key":query.key,"content_type":query.content_type,"transfer_path":path})).await;
    let _ = tokio::fs::remove_file(&path).await;
    let result = result?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("بارگذاری Object ناموفق بود."),
        ));
    }
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor.username,
        "s3.object.upload",
        &format!(
            "connection={id}; bucket={}; key_length={}; size_bytes={total}",
            query.bucket,
            query.key.len()
        ),
    );
    Ok(Json(json!({"ok":true,"size_bytes":total})))
}

pub(super) async fn download_object(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<ObjectTransferQuery>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let actor = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?;
        let _ = read_connection(&db, id)?;
        actor
    };
    if !valid_bucket_name(&query.bucket) || !valid_object_key(&query.key) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام Bucket یا Object Key معتبر نیست.",
        ));
    }
    let path = transfer_path();
    let file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "فایل موقت امن ساخته نشد.",
            )
        })?;
    drop(file);
    let result=privileged_request_async(json!({"action":"s3-download-object","id":id,"bucket":query.bucket,"key":query.key,"transfer_path":path})).await;
    let result = match result {
        Ok(value) => value,
        Err(error) => {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(error);
        }
    };
    if result["ok"] != true {
        let _ = tokio::fs::remove_file(&path).await;
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("دانلود Object ناموفق بود."),
        ));
    }
    let file = tokio::fs::File::open(&path).await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "بازکردن فایل دانلود ناموفق بود.",
        )
    })?;
    let filename = query
        .key
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("object")
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || "._-".contains(*character))
        .take(128)
        .collect::<String>();
    let content_disposition = header::HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        if filename.is_empty() {
            "object"
        } else {
            &filename
        }
    ))
    .unwrap_or_else(|_| header::HeaderValue::from_static("attachment; filename=\"object\""));
    let stream = tokio_util::io::ReaderStream::new(file);
    let mut response = Response::new(Body::from_stream(stream));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/octet-stream"),
    );
    response
        .headers_mut()
        .insert(header::CONTENT_DISPOSITION, content_disposition);
    {
        let db = state.lock().unwrap();
        audit(
            &db,
            &actor.username,
            "s3.object.download",
            &format!(
                "connection={id}; bucket={}; key_length={}",
                query.bucket,
                query.key.len()
            ),
        );
    }
    let cleanup = path.clone();
    tokio::spawn(async move {
        let _ = tokio::fs::remove_file(cleanup).await;
    });
    Ok(response)
}
