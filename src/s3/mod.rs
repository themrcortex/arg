use super::*;
use ::s3::{bucket::Bucket, bucket_ops::BucketConfiguration, creds::Credentials, Region};
use axum::{body::Body, http::header, response::Response};
use std::{
    net::{IpAddr, ToSocketAddrs},
    os::fd::AsRawFd,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path as FsPath, PathBuf},
};
use url::Url;

const SECRET_DIR: &str = "/etc/arg/s3";
const S3_TRANSFER_DIR: &str = "/var/lib/arg/s3-transfer";
const MAX_PAGE_SIZE: usize = 1000;
static S3_SCAN_SEMAPHORE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

fn invalidate_usage_cache(db: &Connection, connection_id: i64, bucket_name: &str) {
    let _ = db.execute(
        "UPDATE s3_usage_snapshots SET scan_status='stale',measured_at=0 WHERE id=(SELECT u.id FROM s3_usage_snapshots u JOIN s3_buckets b ON b.id=u.bucket_id WHERE b.connection_id=?1 AND b.bucket_name=?2 ORDER BY u.id DESC LIMIT 1)",
        params![connection_id,bucket_name],
    );
}

fn usage_scan_allowed(status: Option<&str>, measured_at: Option<i64>, now: i64, force: bool) -> bool {
    let age = measured_at.map(|time| now.saturating_sub(time));
    if status == Some("scanning") {
        return age.is_some_and(|seconds| seconds >= 6 * 3600);
    }
    if force { return true; }
    let max_age = if status == Some("failed") { 3600 } else { 24 * 3600 };
    age.is_none_or(|seconds| seconds >= max_age)
}

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
pub(super) struct BucketQuotaInput {
    quota_bytes: Option<u64>,
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
      {"id":"arvancloud","label":"ابر آروان · ArvanCloud","rclone_provider":"ArvanCloud","endpoint_required":true,"region_required":true,"create_bucket":false,"docs":"https://docs.arvancloud.ir/fa/developer-tools/sdk/object-storage/","endpoints":[{"id":"central1","label":"ایران مرکز ۱","endpoint":"https://s3.ir-central1.arvanstorage.ir","region":"ir-central1"},{"id":"simin","label":"سیمین","endpoint":"https://s3.ir-thr-at1.arvanstorage.ir"},{"id":"shahriar","label":"شهریار","endpoint":"https://s3.ir-tbz-sh1.arvanstorage.ir"}]},
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
    let normalized_host = host
        .trim_end_matches('.')
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    if matches!(
        normalized_host.as_str(),
        "metadata.google.internal"
            | "metadata.google"
            | "metadata"
            | "instance-data.ec2.internal"
            | "metadata.tencentyun.com"
    ) {
        return Err("Endpointهای metadata سرور قابل استفاده نیستند.".into());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses: Vec<IpAddr> = if let Ok(ip) = normalized_host.parse::<IpAddr>() {
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
        if is_cloud_metadata_ip(address)
            || (!allow_private && !metrics::is_public_ip(&address.to_string()))
        {
            return Err(
                "Endpoint خصوصی است. فقط پس از بررسی مقصد و تأیید دسترسی خصوصی آن را فعال کن."
                    .into(),
            );
        }
    }
    Ok(())
}

fn is_cloud_metadata_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            ip == std::net::Ipv4Addr::new(169, 254, 169, 254)
                || ip == std::net::Ipv4Addr::new(100, 100, 100, 200)
        }
        IpAddr::V6(ip) => ip == "fd00:ec2::254".parse::<std::net::Ipv6Addr>().unwrap(),
    }
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
    if input.region.len() > 128 || input.region.chars().any(char::is_control) {
        return Some("Region نباید بیش از 128 نویسه یا نویسهٔ کنترلی داشته باشد.".into());
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
    db.execute_batch("CREATE TABLE IF NOT EXISTS s3_connections (id INTEGER PRIMARY KEY,display_name TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',provider_type TEXT NOT NULL,endpoint TEXT NOT NULL DEFAULT '',region TEXT NOT NULL DEFAULT '',address_style TEXT NOT NULL DEFAULT 'auto',allow_private_endpoint INTEGER NOT NULL DEFAULT 0,credential_present INTEGER NOT NULL DEFAULT 1,last_rotated_at INTEGER NOT NULL,status TEXT NOT NULL DEFAULT 'unverified',last_test_at INTEGER,last_error TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS s3_buckets (id INTEGER PRIMARY KEY,connection_id INTEGER NOT NULL REFERENCES s3_connections(id) ON DELETE CASCADE,bucket_name TEXT NOT NULL,display_name TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',enabled INTEGER NOT NULL DEFAULT 1,verified_at INTEGER,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,UNIQUE(connection_id,bucket_name)); CREATE INDEX IF NOT EXISTS s3_buckets_connection ON s3_buckets(connection_id); CREATE TABLE IF NOT EXISTS s3_usage_snapshots (id INTEGER PRIMARY KEY,bucket_id INTEGER NOT NULL REFERENCES s3_buckets(id) ON DELETE CASCADE,used_bytes INTEGER,object_count INTEGER,quota_bytes INTEGER,quota_source TEXT,usage_source TEXT NOT NULL DEFAULT 'unknown',scan_status TEXT NOT NULL DEFAULT 'not_measured',measured_at INTEGER,error TEXT); CREATE INDEX IF NOT EXISTS s3_usage_bucket_time ON s3_usage_snapshots(bucket_id,measured_at DESC); CREATE TABLE IF NOT EXISTS s3_mounts(id INTEGER PRIMARY KEY,bucket_id INTEGER NOT NULL REFERENCES s3_buckets(id) ON DELETE CASCADE,method TEXT NOT NULL,mount_path TEXT NOT NULL UNIQUE,read_only INTEGER NOT NULL DEFAULT 1,auto_mount INTEGER NOT NULL DEFAULT 0,cache_config_json TEXT NOT NULL DEFAULT '{}',status TEXT NOT NULL DEFAULT 'unmounted',last_error TEXT,updated_at INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS s3_contracts(id INTEGER PRIMARY KEY,title TEXT NOT NULL,seller TEXT,contract_ref TEXT,billing_interval_months INTEGER NOT NULL DEFAULT 1,billing_calendar TEXT NOT NULL DEFAULT 'gregorian',billing_anchor_day INTEGER NOT NULL,timezone TEXT NOT NULL DEFAULT 'Asia/Tehran',start_date TEXT NOT NULL,next_due_date TEXT NOT NULL,end_date TEXT,amount_minor_units INTEGER,currency TEXT,included_quota_bytes INTEGER,auto_renew_recorded INTEGER NOT NULL DEFAULT 0,status TEXT NOT NULL DEFAULT 'active',notes TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);")?;
    db.execute("UPDATE s3_usage_snapshots SET scan_status='stale',measured_at=0 WHERE scan_status='scanning'", [])?;
    db.execute("UPDATE s3_usage_snapshots SET used_bytes=NULL,object_count=NULL,usage_source='inventory',scan_status='not_measured',measured_at=NULL,error=NULL WHERE usage_source='manual'", [])?;
    Ok(())
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
    let directory = FsPath::new(SECRET_DIR);
    let reference = id.to_string();
    if secret_vault::contains(directory, &reference)? {
        let contents = secret_vault::read(directory, &reference)?;
        let secret: SecretConfig = serde_json::from_slice(&contents)
            .map_err(|_| "مقدار رمز‌شدهٔ اتصال قالب معتبر ندارد.".to_string())?;
        if !secret.endpoint.is_empty() {
            validate_endpoint(&secret.endpoint, secret.allow_private_endpoint)?;
        }
        return Ok(secret);
    }
    let path = PathBuf::from(SECRET_DIR).join(format!("{id}.json"));
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&path)
        .map_err(|_| "اطلاعات اتصال پیدا نشد؛ تنظیمات اتصال را دوباره ذخیره کن.".to_string())?;
    let metadata = file.metadata().map_err(|_| "بررسی فایل تنظیمات اتصال ناموفق بود.".to_string())?;
    if !metadata.file_type().is_file() {
        return Err("فایل تنظیمات اتصال معتبر نیست؛ آن را دوباره ذخیره کن.".into());
    }
    // Repair legacy ownership/mode issues in place instead of making every S3
    // operation fail. This runs only through the root-owned privileged helper
    // and uses the already-open, O_NOFOLLOW file descriptor to avoid path races.
    if metadata.uid() != 0
        && unsafe { libc::fchown(file.as_raw_fd(), 0, 0) } != 0
    {
        return Err("ترمیم مالکیت فایل اتصال انجام نشد؛ تنظیمات اتصال را دوباره ذخیره کن.".into());
    }
    if metadata.permissions().mode() & 0o777 != 0o600
        && unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } != 0
    {
        return Err("ترمیم مجوز فایل اتصال انجام نشد؛ تنظیمات اتصال را دوباره ذخیره کن.".into());
    }
    let mut contents = Vec::new();
    file.read_to_end(&mut contents).map_err(|_| "خواندن تنظیمات اتصال ناموفق بود.".to_string())?;
    let secret: SecretConfig = serde_json::from_slice(&contents)
        .map_err(|_| "فایل تنظیمات اتصال معتبر نیست؛ کلیدها را دوباره ذخیره کن.".to_string())?;
    if !secret.endpoint.is_empty() {
        validate_endpoint(&secret.endpoint, secret.allow_private_endpoint)?;
    }
    // Existing installations stored root-only JSON files. Convert them on the
    // first successful read so the S3 UI and old endpoint metadata stay intact.
    secret_vault::store(directory, &reference, &contents)?;
    fs::remove_file(path).map_err(|_| "فایل قدیمی کلید پس از رمزنگاری پاک نشد.".to_string())?;
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
    let directory = FsPath::new(SECRET_DIR);
    let contents = serde_json::to_vec(secret)
        .map_err(|_| "رمزگذاری اطلاعات اتصال ناموفق بود.".to_string())?;
    secret_vault::store(directory, &id.to_string(), &contents)?;
    let legacy = secret_path(id)?;
    match fs::remove_file(legacy) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("فایل قدیمی کلید پس از رمزنگاری پاک نشد.".into()),
    }
    Ok(())
}

fn delete_secret_files(id: i64) -> Result<(), String> {
    let directory = FsPath::new(SECRET_DIR);
    let reference = id.to_string();
    if secret_vault::contains(directory, &reference)? {
        secret_vault::delete(directory, &reference)?;
    }
    let legacy = secret_path(id)?;
    match fs::symlink_metadata(&legacy) {
        Ok(metadata) if metadata.file_type().is_file() && metadata.uid() == 0 => {
            fs::remove_file(legacy).map_err(|_| "حذف فایل قدیمی کلید ناموفق بود.".to_string())?;
        }
        Ok(_) => return Err("فایل قدیمی کلید، فایل معمولی تحت مالکیت root نیست.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("بررسی فایل قدیمی کلید ناموفق بود.".into()),
    }
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
        || message.contains("bucket not found")
        || message.contains("bucket does not exist")
    {
        "Bucket پیدا نشد؛ نام و Endpoint را بررسی کن.".into()
    } else if message.contains("nosuchkey") || message.contains("no such key") {
        "Object مقصد هنوز وجود ندارد و می‌توان آن را بارگذاری کرد.".into()
    } else if message.contains("404") {
        "Object یا Bucket مقصد پیدا نشد؛ نام Bucket و مسیر Object را بررسی کن.".into()
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

// Some S3-compatible services return a 404 HEAD response as an error rather
// than `(headers, 404)`. For a missing object this is the normal preflight
// result when uploading a new key. A missing bucket remains a hard failure.
fn head_error_means_object_missing(error: &::s3::error::S3Error) -> bool {
    match error {
        ::s3::error::S3Error::HttpFailWithBody(404, body) => {
            let body = body.to_ascii_lowercase();
            !(body.contains("nosuchbucket")
                || body.contains("no such bucket")
                || body.contains("bucket not found")
                || body.contains("bucket does not exist")
                || body.contains("bucketnotfound"))
        }
        _ => false,
    }
}

pub(super) fn execute_privileged_request(action: &str, request: &Value) -> Value {
    let id = request["id"].as_i64().unwrap_or(0);
    match action {
        "s3-mount-start" => return start_s3_mount(request),
        "s3-mount-stop" => return stop_s3_mount(request),
        "s3-mount-status" => return s3_mount_status(request),
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
        "s3-secret-delete" => match delete_secret_files(id) {
            Ok(()) => json!({"ok":true}),
            Err(error) => json!({"ok":false,"error":error}),
        },
        "s3-list-buckets" | "s3-test" | "s3-create-bucket" | "s3-list-objects" | "s3-scan-usage"
        | "s3-delete-object" | "s3-upload-object" | "s3-download-object"
        | "s3-upload-local-file" | "s3-download-local-file" => {
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

fn mount_control_paths(mount_id: i64, connection_id: i64, bucket_id: i64) -> Option<(PathBuf, String)> {
    if mount_id <= 0 || connection_id <= 0 || bucket_id <= 0 {
        return None;
    }
    let config_path = PathBuf::from(format!("{SECRET_DIR}/mount-{mount_id}.conf"));
    let unit = format!("arg-s3-mount-{mount_id}.service");
    Some((config_path, unit))
}

fn valid_named_mount_path(value: &str) -> bool {
    let Ok(relative) = std::path::Path::new(value).strip_prefix("/mnt/arg-s3") else {
        return false;
    };
    let components = relative.components().collect::<Vec<_>>();
    components.len() == 2 && components.iter().all(|component| match component {
        std::path::Component::Normal(name) => {
            let name = name.to_string_lossy();
            !name.is_empty()
                && name.len() <= 64
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        }
        _ => false,
    })
}

fn mount_slug(value: &str, fallback: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() && slug.len() < 47 {
                slug.push('-');
            }
            if slug.len() < 48 {
                slug.push((byte as char).to_ascii_lowercase());
            }
            separator = false;
        } else {
            separator = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() { fallback.to_owned() } else { slug.to_owned() }
}

fn bucket_mount_path(
    db: &Connection,
    connection_id: i64,
    bucket_id: i64,
    connection_name: &str,
    bucket_name: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    let connection_slug = mount_slug(connection_name, &format!("connection-{connection_id}"));
    let bucket_slug = mount_slug(bucket_name, &format!("bucket-{bucket_id}"));
    let readable = format!("/mnt/arg-s3/{connection_slug}/{bucket_slug}");
    let used: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM s3_mounts WHERE mount_path=?1 AND bucket_id<>?2)",
            params![readable, bucket_id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if used {
        Ok(format!(
            "/mnt/arg-s3/{connection_slug}-c{connection_id}/{bucket_slug}-b{bucket_id}"
        ))
    } else {
        Ok(readable)
    }
}

fn run_systemctl(args: &[&str]) -> Result<String, String> {
    let mut command = std::process::Command::new("systemctl");
    command.args(args);
    match super::command_result(&mut command) {
        Ok((true, output)) => Ok(output),
        Ok((false, output)) => Err(if output.trim().is_empty() { "systemd operation failed".into() } else { output }),
        Err(error) => Err(error),
    }
}

fn is_mount_point(path: &FsPath) -> bool {
    let expected = path.to_string_lossy();
    fs::read_to_string("/proc/self/mountinfo").is_ok_and(|mountinfo| {
        mountinfo.lines().any(|line| line.split_whitespace().nth(4) == Some(expected.as_ref()))
    })
}

fn wait_for_mount(unit: &str, path: &FsPath) -> Result<(), String> {
    for _ in 0..80 {
        if is_mount_point(path) {
            return Ok(());
        }
        if run_systemctl(&["is-active", "--quiet", unit]).is_err() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    Err("سرویس اجرا شد، اما نقطهٔ Mount در فایل‌سیستم دیده نشد. وضعیت systemd و اتصال Bucket را بررسی کن.".into())
}

fn s3_mount_status(request: &Value) -> Value {
    let mount_id = request["mount_id"].as_i64().unwrap_or(0);
    let connection_id = request["connection_id"].as_i64().unwrap_or(0);
    let bucket_id = request["bucket_id"].as_i64().unwrap_or(0);
    let Some((_, unit)) = mount_control_paths(mount_id, connection_id, bucket_id) else {
        return json!({"ok":false,"error":"شناسه Mount نامعتبر است."});
    };
    let Some(path) = request["mount_path"].as_str().filter(|path| valid_named_mount_path(path)).map(PathBuf::from) else {
        return json!({"ok":false,"error":"مسیر Mount نامعتبر است."});
    };
    let active_state = match run_systemctl(&["show", "--property=ActiveState", "--value", &unit]) {
        Ok(value) => value.trim().to_owned(),
        Err(error) if error.to_ascii_lowercase().contains("could not be found") => "inactive".into(),
        Err(error) => return json!({"ok":false,"error":format!("خواندن وضعیت سرویس Mount ناموفق بود: {error}")}),
    };
    let path_mounted = is_mount_point(&path);
    let mounted = active_state == "active" && path_mounted;
    let failed = run_systemctl(&["is-failed", "--quiet", &unit]).is_ok();
    let status = if mounted { "mounted" } else if path_mounted || failed { "failed" } else if active_state == "active" { "mounting" } else { "unmounted" };
    let error = if path_mounted && !mounted {
        Some("نقطهٔ Mount وجود دارد اما سرویس مربوطه فعال نیست؛ از همین‌جا Mount را دوباره اعمال کن.")
    } else if failed {
        Some("سرویس systemd این Mount با خطا متوقف شده است؛ تنظیمات اتصال و گزارش سرویس را بررسی کن.")
    } else if status == "mounting" {
        Some("سرویس فعال است و هنوز Mount در سیستم ثبت نشده؛ چند لحظه بعد وضعیت را تازه کن.")
    } else {
        None
    };
    json!({"ok":true,"status":status,"unit_active":active_state == "active","mounted":mounted,"error":error})
}

fn start_s3_mount(request: &Value) -> Value {
    let mount_id = request["mount_id"].as_i64().unwrap_or(0);
    let connection_id = request["connection_id"].as_i64().unwrap_or(0);
    let bucket_id = request["bucket_id"].as_i64().unwrap_or(0);
    let Some(bucket) = request["bucket"].as_str().filter(|value| valid_bucket_name(value)) else {
        return json!({"ok":false,"error":"Bucket نامعتبر است."});
    };
    let Some((config_path, unit)) = mount_control_paths(mount_id, connection_id, bucket_id) else {
        return json!({"ok":false,"error":"شناسه Mount نامعتبر است."});
    };
    let Some(mount_path) = request["mount_path"].as_str().filter(|path| valid_named_mount_path(path)).map(PathBuf::from) else {
        return json!({"ok":false,"error":"نام یا مسیر Mount معتبر نیست."});
    };
    let secret = match read_secret_file(connection_id) {
        Ok(secret) => secret,
        Err(error) => return json!({"ok":false,"error":error}),
    };
    if secret.region.len() > 128 || secret.region.chars().any(char::is_control) {
        return json!({"ok":false,"error":"Signing Region در تنظیمات اتصال معتبر نیست."});
    }
    let rclone = ["/usr/bin/rclone", "/usr/local/bin/rclone"]
        .iter()
        .find(|path| PathBuf::from(path).is_file())
        .copied();
    let Some(rclone) = rclone else {
        return json!({"ok":false,"error":"Rclone نصب نیست. ابتدا بستهٔ rclone را از بخش مدیریت APT نصب کن."});
    };
    let rclone_provider = match secret.provider_type.as_str() {
        "aws" => "AWS",
        "cloudflare_r2" => "Cloudflare",
        "arvancloud" => "ArvanCloud",
        "digitalocean_spaces" => "DigitalOcean",
        "wasabi" => "Wasabi",
        "minio" => "Minio",
        _ => "Other",
    };
    let remote = format!(
        "[arg]\ntype = s3\nprovider = {rclone_provider}\naccess_key_id = {}\nsecret_access_key = {}\n",
        secret.access_key_id, secret.secret_access_key
    );
    let mut config = remote;
    if let Some(token) = secret.session_token.as_deref() {
        config.push_str(&format!("session_token = {token}\n"));
    }
    if !secret.region.is_empty() {
        config.push_str(&format!("region = {}\n", secret.region));
    }
    if !secret.endpoint.is_empty() {
        config.push_str(&format!("endpoint = {}\n", secret.endpoint));
    }
    config.push_str(&format!("force_path_style = {}\n", secret.address_style != "virtual"));
    let already_active = run_systemctl(&["is-active", "--quiet", &unit]).is_ok();
    let already_mounted = already_active && is_mount_point(&mount_path);
    if let Err(error) = ensure_mount_directory(PathBuf::from("/mnt/arg-s3").as_path()) {
        return json!({"ok":false,"error":format!("مسیر پایهٔ Mount امن نیست: {error}")});
    }
    let cache_root = PathBuf::from("/var/cache/arg-s3");
    let cache_path = cache_root.join(format!("mount-{mount_id}"));
    if ensure_mount_directory(&cache_root).is_err() || ensure_mount_directory(&cache_path).is_err() {
        return json!({"ok":false,"error":"ساخت پوشهٔ Cache محلی امن ناموفق بود."});
    }
    if let Some(parent) = mount_path.parent() {
        if let Err(error) = ensure_mount_directory(parent) {
            return json!({"ok":false,"error":format!("پوشهٔ نام اتصال امن نیست: {error}")});
        }
    }
    if !already_mounted {
        if let Err(error) = ensure_mount_directory(&mount_path) {
            return json!({"ok":false,"error":format!("پوشهٔ Mount امن نیست: {error}")});
        }
    }
    let mount_metadata = already_mounted || match fs::read_dir(&mount_path) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => false,
    };
    if !mount_metadata {
        return json!({"ok":false,"error":"مسیر Mount خالی نیست یا قابل بررسی نیست."});
    }
    let mut conf_file = match fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(&config_path) {
        Ok(file) => file,
        Err(_) => return json!({"ok":false,"error":"ساخت فایل تنظیمات امن Rclone ناموفق بود."}),
    };
    if conf_file.write_all(config.as_bytes()).is_err() || conf_file.sync_all().is_err() {
        return json!({"ok":false,"error":"ذخیرهٔ تنظیمات امن Rclone ناموفق بود."});
    }
    let _ = fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600));
    let uid = match std::process::Command::new("id").args(["-u", "arg"]).output() {
        Ok(result) if result.status.success() => String::from_utf8_lossy(&result.stdout).trim().to_owned(),
        _ => return json!({"ok":false,"error":"کاربر سرویس ARG پیدا نشد."}),
    };
    let gid = match std::process::Command::new("id").args(["-g", "arg"]).output() {
        Ok(result) if result.status.success() => String::from_utf8_lossy(&result.stdout).trim().to_owned(),
        _ => return json!({"ok":false,"error":"گروه سرویس ARG پیدا نشد."}),
    };
    if !uid.bytes().all(|byte| byte.is_ascii_digit()) || !gid.bytes().all(|byte| byte.is_ascii_digit()) {
        return json!({"ok":false,"error":"شناسهٔ کاربر سرویس ARG معتبر نیست."});
    }
    let mut args = vec!["mount".to_owned(), format!("arg:{bucket}"), mount_path.to_string_lossy().into_owned(), format!("--config={}", config_path.display()), "--allow-other".into(), "--default-permissions".into(), format!("--uid={uid}"), format!("--gid={gid}"), "--dir-perms=0700".into(), "--file-perms=0600".into(), "--vfs-cache-mode=writes".into(), format!("--cache-dir={}",cache_path.display()), "--vfs-cache-max-size=1G".into(), "--vfs-cache-max-age=24h".into()];
    if request["read_only"].as_bool().unwrap_or(true) {
        args.push("--read-only".into());
    }
    let args = args.iter().map(|arg| systemd_escape_arg(arg)).collect::<Vec<_>>().join(" ");
    // Do not isolate the service mount namespace: ARG and host processes need to see this FUSE mount.
    let unit_content = format!("[Unit]\nDescription=ARG S3 mount {mount_id}\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nUser=root\nExecStart={rclone} {args}\nRestart=on-failure\nRestartSec=5\nDeviceAllow=/dev/fuse rw\nCapabilityBoundingSet=CAP_SYS_ADMIN\nNoNewPrivileges=true\n\n[Install]\nWantedBy=multi-user.target\n");
    let unit_path = PathBuf::from("/etc/systemd/system").join(&unit);
    if fs::write(&unit_path, unit_content).is_err() || fs::set_permissions(&unit_path, fs::Permissions::from_mode(0o644)).is_err() {
        return json!({"ok":false,"error":"ثبت سرویس systemd برای Mount ناموفق بود."});
    }
    if let Err(error) = run_systemctl(&["daemon-reload"]) {
        return json!({"ok":false,"error":format!("بارگذاری تنظیمات systemd ناموفق بود: {error}")});
    }
    if request["auto_mount"].as_bool().unwrap_or(false) {
        if let Err(error) = run_systemctl(&["enable", &unit]) {
            return json!({"ok":false,"error":format!("فعال‌کردن اجرای خودکار Mount ناموفق بود: {error}")});
        }
    } else {
        let _ = run_systemctl(&["disable", &unit]);
    }
    match run_systemctl(&["restart", &unit]) {
        Ok(_) => match wait_for_mount(&unit, &mount_path) {
            Ok(()) => json!({"ok":true,"mount_path":mount_path,"unit":unit}),
            Err(error) => json!({"ok":false,"error":format!("Mount کامل نشد: {error}")}),
        },
        Err(error) => json!({"ok":false,"error":format!("Mount اجرا نشد: {error}")}),
    }
}

fn systemd_escape_arg(value: &str) -> String {
    if value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"/_:=.-".contains(&byte)) {
        value.to_owned()
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%"))
    }
}

fn ensure_mount_directory(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::DirBuilderExt;
    if !path.starts_with("/mnt/arg-s3") && !path.starts_with("/var/cache/arg-s3") {
        return Err("مسیر خارج از پوشه‌های اختصاصی S3 است.".into());
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err("پوشه باید واقعی، متعلق به root و بدون مجوز نوشتن برای دیگران باشد.".into());
            }
            if metadata.uid() != 0 || metadata.permissions().mode() & 0o022 != 0 {
                let directory = fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                    .open(path)
                    .map_err(|_| "بازکردن امن پوشهٔ اختصاصی S3 ناموفق بود.".to_owned())?;
                let fd = directory.as_raw_fd();
                if unsafe { libc::fchown(fd, 0, 0) } != 0 || unsafe { libc::fchmod(fd, 0o755) } != 0 {
                    return Err("ایمن‌سازی مالکیت و مجوز پوشهٔ اختصاصی S3 ناموفق بود.".into());
                }
                let secured = directory.metadata().map_err(|error| error.to_string())?;
                if secured.uid() != 0 || secured.permissions().mode() & 0o022 != 0 {
                    return Err("پوشه باید واقعی، متعلق به root و بدون مجوز نوشتن برای دیگران باشد.".into());
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(false).mode(0o755);
            builder.create(path).map_err(|error| error.to_string())?;
            let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() || metadata.uid() != 0 || metadata.permissions().mode() & 0o022 != 0 {
                return Err("مجوزهای پوشهٔ Mount ایمن نیست.".into());
            }
        }
        Err(error) => return Err(error.to_string()),
    }
    Ok(())
}

fn stop_s3_mount(request: &Value) -> Value {
    let mount_id = request["mount_id"].as_i64().unwrap_or(0);
    let Some((config_path, unit)) = mount_control_paths(mount_id, request["connection_id"].as_i64().unwrap_or(0), request["bucket_id"].as_i64().unwrap_or(0)) else {
        return json!({"ok":false,"error":"شناسه Mount نامعتبر است."});
    };
    match run_systemctl(&["stop", &unit]) {
        Ok(_) => json!({"ok":true,"config":config_path}),
        Err(error) => json!({"ok":false,"error":format!("توقف Mount ناموفق بود: {error}")}),
    }
}

async fn test_bucket_connection(bucket: &Bucket) -> Value {
    const VERIFY_KEY: &str = "arg-verify";
    const VERIFY_BODY: &[u8] = b"ARG S3 connection verification marker\n";

    let list_objects = bucket
        .list_page(String::new(), Some("/".into()), None, None, Some(1))
        .await
        .is_ok();

    let write_marker = match bucket
        .put_object_with_content_type(VERIFY_KEY, VERIFY_BODY, "text/plain; charset=utf-8")
        .await
    {
        Ok(response) if response.status_code() < 300 => true,
        Ok(_) => {
            return json!({
                "ok":false,"status":"failed","checks":{"endpoint":true,"list_objects":list_objects,"write_marker":false,"read_marker":false},
                "marker_key":VERIFY_KEY,"error":"سرویس‌دهنده آپلود فایل آزمایشی arg-verify را تأیید نکرد."
            });
        }
        Err(error) => {
            return json!({
                "ok":false,"status":"failed","checks":{"endpoint":false,"list_objects":list_objects,"write_marker":false,"read_marker":false},
                "marker_key":VERIFY_KEY,"error":format!("آپلود فایل آزمایشی arg-verify ناموفق بود: {}", helper_error(&error))
            });
        }
    };

    let (head_object, head_status) = match bucket.head_object(VERIFY_KEY).await {
        Ok((_headers, status)) => (true, status),
        Err(_) => (false, 0),
    };
    let mut response = match bucket.get_object_stream(VERIFY_KEY).await {
        Ok(response) => response,
        Err(error) => {
            return json!({
                "ok":true,"status":"degraded","checks":{"endpoint":true,"list_objects":list_objects,"write_marker":write_marker,"head_object":head_object,"read_marker":false},
                "marker_key":VERIFY_KEY,"marker_persisted":true,"error":format!("فایل arg-verify آپلود شد، اما مجوز خواندن برای تأیید در دسترس نیست: {}", helper_error(&error))
            });
        }
    };
    if response.status_code >= 300 {
        return json!({
            "ok":true,"status":"degraded","checks":{"endpoint":true,"list_objects":list_objects,"write_marker":write_marker,"head_object":head_object,"read_marker":false},
            "marker_key":VERIFY_KEY,"marker_persisted":true,"error":"فایل arg-verify آپلود شد، اما سرویس‌دهنده مجوز خواندن آن را نداد."
        });
    }
    let mut body = Vec::with_capacity(VERIFY_BODY.len());
    while let Some(chunk) = response.bytes.next().await {
        let Ok(chunk) = chunk else {
            return json!({
                "ok":false,"status":"failed","checks":{"endpoint":true,"list_objects":list_objects,"write_marker":write_marker,"head_object":head_object,"read_marker":false},
                "marker_key":VERIFY_KEY,"error":"پاسخ خواندن فایل arg-verify ناقص بود."
            });
        };
        if body.len().saturating_add(chunk.len()) > 1024 {
            return json!({
                "ok":false,"status":"failed","checks":{"endpoint":true,"list_objects":list_objects,"write_marker":write_marker,"head_object":head_object,"read_marker":false},
                "marker_key":VERIFY_KEY,"error":"محتوای arg-verify غیرمنتظره و بزرگ‌تر از حد مجاز است."
            });
        }
        body.extend_from_slice(&chunk);
    }
    let read_marker = body.as_slice() == VERIFY_BODY;
    if !read_marker {
        return json!({
            "ok":false,"status":"failed","checks":{"endpoint":true,"list_objects":list_objects,"write_marker":write_marker,"head_object":head_object,"read_marker":false},
            "marker_key":VERIFY_KEY,"error":"محتوای فایل arg-verify پس از آپلود با محتوای مورد انتظار یکسان نبود."
        });
    }
    let status = if list_objects && head_object && (200..300).contains(&head_status) {
        "connected"
    } else {
        "degraded"
    };
    let message = if status == "connected" {
        "اتصال، فهرست، خواندن و نوشتن S3 تأیید شد؛ فایل arg-verify در Bucket قرار گرفت."
    } else {
        "اتصال و خواندن/نوشتن تأیید شد و فایل arg-verify در Bucket قرار گرفت؛ دسترسی فهرست یا HEAD محدود است."
    };
    json!({
        "ok":true,
        "status":status,
        "message":message,
        "checks":{"endpoint":true,"list_objects":list_objects,"write_marker":write_marker,"head_object":head_object && (200..300).contains(&head_status),"read_marker":read_marker},
        "marker_key":VERIFY_KEY,
        "marker_size_bytes":VERIFY_BODY.len(),
        "marker_persisted":true
    })
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
            test_bucket_connection(&bucket).await
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
        "s3-scan-usage" => {
            let Some(name) = request["bucket"].as_str().filter(|value| valid_bucket_name(value)) else {
                return json!({"ok":false,"error":"نام Bucket معتبر نیست."});
            };
            let bucket = match make_bucket(name, secret) {
                Ok(bucket) => bucket,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            let mut token: Option<String> = None;
            let mut used_bytes: u64 = 0;
            let mut object_count: u64 = 0;
            loop {
                match bucket.list_page(String::new(), None, token, None, Some(MAX_PAGE_SIZE)).await {
                    Ok((page, _)) => {
                        for object in page.contents {
                            let Some(total) = used_bytes.checked_add(object.size) else {
                                return json!({"ok":false,"error":"حجم آبجکت‌ها از محدودهٔ قابل محاسبه بیشتر است."});
                            };
                            used_bytes = total;
                            object_count = object_count.saturating_add(1);
                        }
                        if !page.is_truncated {
                            return json!({"ok":true,"used_bytes":used_bytes,"object_count":object_count});
                        }
                        token = page.next_continuation_token;
                        if token.is_none() {
                            return json!({"ok":false,"error":"ارائه‌دهنده صفحهٔ ادامهٔ فهرست را برنگرداند؛ اسکن کامل نشد."});
                        }
                    }
                    Err(error) => return json!({"ok":false,"error":helper_error(&error)}),
                }
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
            let metadata = match validate_transfer_file(path, false) {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            if metadata.len() > 512 * 1024 * 1024 {
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
            let metadata = match validate_transfer_file(path, true) {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            if metadata.len() != 0 {
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
        "s3-upload-local-file" => {
            let (Some(name), Some(key), Some(source)) = (
                request["bucket"].as_str().filter(|value| valid_bucket_name(value)),
                request["key"].as_str().filter(|value| valid_object_key(value)),
                request["local_path"].as_str().filter(|value| value.starts_with('/')),
            ) else {
                return json!({"ok":false,"error":"مسیر فایل یا مشخصات Bucket معتبر نیست."});
            };
            let path = PathBuf::from(source);
            let metadata = match fs::symlink_metadata(&path) {
                Ok(value) if value.file_type().is_file() && value.len() <= 512 * 1024 * 1024 => value,
                Ok(_) => return json!({"ok":false,"error":"فقط فایل معمولی تا سقف 512 MiB قابل انتقال است؛ symlink پذیرفته نمی‌شود."}),
                Err(_) => return json!({"ok":false,"error":"فایل مبدأ پیدا نشد یا قابل خواندن نیست."}),
            };
            let path = match fs::canonicalize(path) {
                Ok(value) => value,
                Err(_) => return json!({"ok":false,"error":"حل مسیر فایل مبدأ ناموفق بود."}),
            };
            let file = match tokio::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path)
                .await
            {
                Ok(value) => value,
                Err(_) => return json!({"ok":false,"error":"بازکردن فایل مبدأ ناموفق بود."}),
            };
            let bucket = match make_bucket(name, secret) {
                Ok(bucket) => bucket,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            let allow_overwrite = request["overwrite"].as_bool().unwrap_or(false);
            match bucket.head_object(key).await {
                Ok((_, status)) if (200..300).contains(&status) && !allow_overwrite => {
                    return json!({"ok":false,"error":"Object مقصد از قبل وجود دارد. برای جایگزینی، تأیید بازنویسی را فعال کن."});
                }
                Ok((_, status)) if (200..300).contains(&status) || status == 404 => {}
                Ok(_) if allow_overwrite => {}
                Ok(_) => return json!({"ok":false,"error":"وجود Object مقصد قابل بررسی نیست؛ برای ادامه باید بازنویسی را صریحاً تأیید کنی."}),
                Err(_) if allow_overwrite => {}
                Err(error) if !allow_overwrite && head_error_means_object_missing(&error) => {}
                Err(error) => return json!({"ok":false,"error":format!("بررسی Object مقصد ناموفق بود؛ بازنویسی تأیید نشده: {}",helper_error(&error))}),
            }
            let content_type = request["content_type"].as_str().filter(|value| value.len() <= 128 && value.bytes().all(|byte| byte.is_ascii_graphic())).unwrap_or("application/octet-stream");
            let mut file = file;
            match bucket.put_object_stream_with_content_type(&mut file, key, content_type).await {
                Ok(response) if response.status_code() < 300 => json!({"ok":true,"size_bytes":metadata.len()}),
                Ok(_) => json!({"ok":false,"error":"ارائه‌دهنده انتقال فایل را تأیید نکرد."}),
                Err(error) => json!({"ok":false,"error":helper_error(&error)}),
            }
        }
        "s3-download-local-file" => {
            let (Some(name), Some(key), Some(target)) = (
                request["bucket"].as_str().filter(|value| valid_bucket_name(value)),
                request["key"].as_str().filter(|value| valid_object_key(value)),
                request["local_path"].as_str().filter(|value| value.starts_with('/')),
            ) else {
                return json!({"ok":false,"error":"مسیر مقصد یا مشخصات Bucket معتبر نیست."});
            };
            let target = PathBuf::from(target);
            let Some(file_name) = target.file_name().filter(|name| !name.is_empty() && *name != "." && *name != "..") else {
                return json!({"ok":false,"error":"مسیر مقصد باید نام فایل داشته باشد."});
            };
            let Some(parent) = target.parent() else {
                return json!({"ok":false,"error":"پوشهٔ مقصد معتبر نیست."});
            };
            let parent = match fs::canonicalize(parent) {
                Ok(value) if value.is_dir() => value,
                _ => return json!({"ok":false,"error":"پوشهٔ مقصد وجود ندارد یا معتبر نیست."}),
            };
            let target = parent.join(file_name);
            let bucket = match make_bucket(name, secret) {
                Ok(bucket) => bucket,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            let mut response = match bucket.get_object_stream(key).await {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":helper_error(&error)}),
            };
            if response.status_code >= 300 {
                return json!({"ok":false,"error":"دریافت Object از ارائه‌دهنده تأیید نشد."});
            }
            let mut file = match tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&target)
                .await
            {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return json!({"ok":false,"error":"فایلی با این نام از قبل وجود دارد؛ هیچ فایلی بازنویسی نشد."}),
                Err(_) => return json!({"ok":false,"error":"ساخت فایل مقصد ناموفق بود."}),
            };
            let mut size = 0u64;
            while let Some(chunk) = response.bytes.next().await {
                let Ok(chunk) = chunk else {
                    drop(file);
                    let _ = fs::remove_file(&target);
                    return json!({"ok":false,"error":"انتقال Object از ارائه‌دهنده قطع شد."});
                };
                size = size.saturating_add(chunk.len() as u64);
                if size > 2 * 1024 * 1024 * 1024 || file.write_all(&chunk).await.is_err() {
                    drop(file);
                    let _ = fs::remove_file(&target);
                    return json!({"ok":false,"error":"انتقال از سقف 2 GiB عبور کرد یا نوشتن فایل ناموفق بود."});
                }
            }
            if file.sync_all().await.is_err() {
                drop(file);
                let _ = fs::remove_file(&target);
                return json!({"ok":false,"error":"ذخیرهٔ فایل مقصد کامل نشد."});
            }
            json!({"ok":true,"size_bytes":size,"local_path":target})
        }
        _ => json!({"ok":false,"error":"Unsupported S3 network action"}),
    }
}

fn valid_object_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 1024 && !key.chars().any(char::is_control)
}
fn valid_transfer_path(path: &str) -> bool {
    path.strip_prefix(&format!("{S3_TRANSFER_DIR}/"))
        .is_some_and(|name| name.len() == 64 && name.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn prepare_transfer_directory() -> Result<(), String> {
    let directory = PathBuf::from(S3_TRANSFER_DIR);
    std::fs::create_dir_all(&directory).map_err(|_| "پوشهٔ موقت مشترک S3 ساخته نشد.".to_string())?;
    let metadata = std::fs::symlink_metadata(&directory)
        .map_err(|_| "بررسی پوشهٔ موقت مشترک S3 ناموفق بود.".to_string())?;
    if !metadata.file_type().is_dir() || metadata.uid() != unsafe { libc::geteuid() } {
        return Err("پوشهٔ موقت S3 باید یک پوشهٔ واقعی متعلق به سرویس ARG باشد.".into());
    }
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
        .map_err(|_| "تنظیم دسترسی امن پوشهٔ موقت S3 ناموفق بود.".to_string())
}

fn validate_transfer_file(path: &str, allow_empty: bool) -> Result<std::fs::Metadata, String> {
    if !valid_transfer_path(path) {
        return Err("مسیر فایل موقت S3 معتبر نیست.".into());
    }
    let directory = std::fs::symlink_metadata(S3_TRANSFER_DIR)
        .map_err(|_| "پوشهٔ مشترک انتقال S3 پیدا نشد.".to_string())?;
    if !directory.file_type().is_dir()
        || directory.uid() == 0
        || directory.permissions().mode() & 0o077 != 0
    {
        return Err("دسترسی پوشهٔ مشترک انتقال S3 ناامن است.".into());
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "فایل موقت انتقال S3 پیدا نشد؛ پوشهٔ PrivateTmp سرویس‌ها جداست.".to_string())?;
    if !metadata.file_type().is_file()
        || metadata.uid() != directory.uid()
        || metadata.permissions().mode() & 0o777 != 0o600
        || (!allow_empty && metadata.len() == 0)
    {
        return Err("فایل موقت انتقال S3 ناامن یا خالی است.".into());
    }
    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_object_head_errors_allow_new_upload_but_not_missing_bucket() {
        use ::s3::error::S3Error;
        assert!(head_error_means_object_missing(&S3Error::HttpFailWithBody(
            404,
            "<Error><Code>NoSuchKey</Code></Error>".into(),
        )));
        assert!(head_error_means_object_missing(&S3Error::HttpFailWithBody(
            404,
            String::new(),
        )));
        assert!(!head_error_means_object_missing(&S3Error::HttpFailWithBody(
            404,
            "<Error><Code>NoSuchBucket</Code></Error>".into(),
        )));
        assert!(!head_error_means_object_missing(&S3Error::HttpFailWithBody(
            403,
            "AccessDenied".into(),
        )));
    }

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
    fn cloud_metadata_addresses_remain_blocked_for_private_endpoints() {
        assert!(is_cloud_metadata_ip("169.254.169.254".parse().unwrap()));
        assert!(is_cloud_metadata_ip("100.100.100.200".parse().unwrap()));
        assert!(is_cloud_metadata_ip("fd00:ec2::254".parse().unwrap()));
        assert!(!is_cloud_metadata_ip("10.0.0.5".parse().unwrap()));
    }

    #[test]
    fn transfer_paths_are_shared_and_cannot_escape_the_transfer_directory() {
        assert!(valid_transfer_path(&format!("{S3_TRANSFER_DIR}/{}", "a1b2".repeat(16))));
        assert!(!valid_transfer_path(
            "/var/lib/arg/s3-transfer/../../etc/passwd"
        ));
        assert!(!valid_transfer_path("/etc/arg/s3/1.json"));
    }

    #[test]
    fn s3_mount_paths_are_generated_from_positive_ids_only() {
        let (config_path, unit) = mount_control_paths(12, 3, 8).unwrap();
        assert_eq!(config_path, PathBuf::from("/etc/arg/s3/mount-12.conf"));
        assert_eq!(unit, "arg-s3-mount-12.service");
        assert!(mount_control_paths(0, 3, 8).is_none());
        assert!(mount_control_paths(12, -1, 8).is_none());
        assert!(valid_named_mount_path("/mnt/arg-s3/backups/media-files"));
        assert!(!valid_named_mount_path("/mnt/arg-s3/../../etc"));
        assert!(!valid_named_mount_path("/mnt/arg-s3/backups/media/files"));
        assert_eq!(mount_slug("Production Backups", "connection"), "production-backups");
        assert_eq!(mount_slug("فضای پشتیبان", "connection-12"), "connection-12");
    }

    #[test]
    fn bucket_usage_scan_uses_a_cache_and_retries_failures_slowly() {
        let now = 1_000_000;
        assert!(usage_scan_allowed(None, None, now, false));
        assert!(!usage_scan_allowed(Some("measured"), Some(now - 23 * 3600), now, false));
        assert!(usage_scan_allowed(Some("measured"), Some(now - 24 * 3600), now, false));
        assert!(!usage_scan_allowed(Some("failed"), Some(now - 30), now, false));
        assert!(usage_scan_allowed(Some("failed"), Some(now - 3600), now, false));
        assert!(usage_scan_allowed(Some("measured"), Some(now), now, true));
        assert!(!usage_scan_allowed(Some("scanning"), Some(now - 10), now, true));
        assert!(usage_scan_allowed(Some("scanning"), Some(now - 6 * 3600), now, false));
    }
}

pub(super) async fn list_providers(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    Ok(Json(json!({"providers":providers()})))
}
#[derive(Deserialize)]
pub(super) struct MountConfigInput {
    read_only: bool,
    auto_mount: bool,
}

fn mount_public(mount_id: i64, connection_id: i64, bucket_id: i64, bucket: &str, mount_path: &str, read_only: bool, auto_mount: bool, status: &str, error: Option<&str>) -> Value {
    json!({"id":mount_id,"connection_id":connection_id,"bucket_id":bucket_id,"bucket":bucket,"mount_path":mount_path,"mount_name":format!("{bucket} · Mount شده در VPS"),"method":"rclone","read_only":read_only,"auto_mount":auto_mount,"status":status,"last_error":error,"capacity":"unknown","capacity_note":"S3 فضای ابری است؛ ظرفیت آن به دیسک محلی اضافه نمی‌شود و از statvfs قابل اتکا نیست."})
}

pub(super) async fn get_bucket_mount(State(state): State<AppState>, Path(bucket_id): Path<i64>, headers: HeaderMap) -> ApiResult {
    let (connection_id, bucket_name, mount) = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
        let bucket: (i64, String) = db.query_row("SELECT connection_id,bucket_name FROM s3_buckets WHERE id=?1", [bucket_id], |row| Ok((row.get(0)?,row.get(1)?))).map_err(|_| error(StatusCode::NOT_FOUND,"Bucket پیدا نشد."))?;
        let mount: Option<(i64,String,bool,bool,String,Option<String>,i64)> = db.query_row("SELECT id,mount_path,read_only!=0,auto_mount!=0,status,last_error,updated_at FROM s3_mounts WHERE bucket_id=?1",[bucket_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?))).optional().map_err(db_error)?;
        (bucket.0, bucket.1, mount)
    };
    let Some((id,mount_path,read_only,auto_mount,stored_status,stored_error,updated_at)) = mount else {
        return Ok(Json(json!({"mount":null})));
    };
    let runtime = privileged_request_async(json!({"action":"s3-mount-status","mount_id":id,"connection_id":connection_id,"bucket_id":bucket_id,"mount_path":mount_path})).await;
    let (status,last_error) = match runtime {
        Ok(value) if value["ok"] == true => {
            let runtime_status = value["status"].as_str().unwrap_or("unknown");
            let mut resolved_status = runtime_status.to_owned();
            let mut resolved_error = value["error"].as_str().map(str::to_owned);
            if runtime_status == "mounting" && now().saturating_sub(updated_at) > 60 {
                resolved_status = "failed".into();
                resolved_error = Some("سرویس Mount بیش از یک دقیقه فعال مانده اما نقطهٔ Mount در فایل‌سیستم دیده نمی‌شود.".into());
            } else if runtime_status == "unmounted" && stored_status == "failed" && stored_error.is_some() {
                // Keep an actionable setup failure until a later successful start clears it.
                resolved_status = stored_status.clone();
                resolved_error = stored_error.clone();
            } else if runtime_status == "unmounted" && auto_mount {
                resolved_status = "failed".into();
                resolved_error = Some("Mount برای اجرای خودکار تنظیم شده، اما سرویس آن فعال نیست.".into());
            }
            let db = state.lock().unwrap();
            db.execute("UPDATE s3_mounts SET status=?1,last_error=?2 WHERE id=?3",params![resolved_status,resolved_error,id]).map_err(db_error)?;
            (resolved_status, resolved_error)
        }
        Ok(value) => ("unknown".into(), value["error"].as_str().map(str::to_owned).or_else(|| Some("پاسخ وضعیت Mount معتبر نیست.".into()))),
        Err((_, response)) => ("unknown".into(), response["error"].as_str().map(str::to_owned).or_else(|| Some("خواندن وضعیت واقعی Mount ناموفق بود.".into()))),
    };
    let mount = mount_public(id,connection_id,bucket_id,&bucket_name,&mount_path,read_only,auto_mount,&status,last_error.as_deref());
    Ok(Json(json!({"mount":mount})))
}

pub(super) async fn configure_bucket_mount(State(state): State<AppState>, Path(bucket_id): Path<i64>, headers: HeaderMap, Json(input): Json<MountConfigInput>) -> ApiResult {
    check_origin(&headers)?;
    let (actor, connection_id, bucket_name, mount_path, mount_id) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?;
        let (connection_id,bucket_name,connection_name): (i64,String,String) = db.query_row("SELECT b.connection_id,b.bucket_name,c.display_name FROM s3_buckets b JOIN s3_connections c ON c.id=b.connection_id WHERE b.id=?1 AND b.enabled=1",[bucket_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).map_err(|_|error(StatusCode::NOT_FOUND,"Bucket فعال پیدا نشد."))?;
        let mount_path = bucket_mount_path(&db,connection_id,bucket_id,&connection_name,&bucket_name)?;
        let mount_id: Option<i64> = db.query_row("SELECT id FROM s3_mounts WHERE bucket_id=?1",[bucket_id],|row|row.get(0)).optional().map_err(db_error)?;
        let mount_id = if let Some(id) = mount_id {
            db.execute("UPDATE s3_mounts SET method='rclone',mount_path=?1,read_only=?2,auto_mount=?3,status='mounting',last_error=NULL,updated_at=?4 WHERE id=?5",params![mount_path,input.read_only as i64,input.auto_mount as i64,now(),id]).map_err(db_error)?;
            id
        } else {
            db.execute("INSERT INTO s3_mounts(bucket_id,method,mount_path,read_only,auto_mount,status,updated_at) VALUES (?1,'rclone',?2,?3,?4,'mounting',?5)",params![bucket_id,mount_path,input.read_only as i64,input.auto_mount as i64,now()]).map_err(db_error)?;
            db.last_insert_rowid()
        };
        (actor,connection_id,bucket_name,mount_path,mount_id)
    };
    let result = privileged_request_async(json!({"action":"s3-mount-start","mount_id":mount_id,"connection_id":connection_id,"bucket_id":bucket_id,"bucket":bucket_name,"mount_path":mount_path,"read_only":input.read_only,"auto_mount":input.auto_mount})).await?;
    let ok = result["ok"] == true;
    let error_message = result["error"].as_str();
    let db = state.lock().unwrap();
    db.execute("UPDATE s3_mounts SET status=?1,last_error=?2,updated_at=?3 WHERE id=?4",params![if ok {"mounted"} else {"failed"},error_message,now(),mount_id]).map_err(db_error)?;
    audit(&db,&actor.username,"s3.mount.start",&format!("mount={mount_id}; bucket_id={bucket_id}; read_only={}; auto_mount={}",input.read_only,input.auto_mount));
    if !ok { return Err(error(StatusCode::BAD_GATEWAY,error_message.unwrap_or("Mount فضای ابری ناموفق بود."))); }
    Ok(Json(json!({"ok":true,"mount":mount_public(mount_id,connection_id,bucket_id,&bucket_name,&mount_path,input.read_only,input.auto_mount,"mounted",None)})))
}

pub(super) async fn stop_bucket_mount(State(state): State<AppState>, Path(mount_id): Path<i64>, headers: HeaderMap) -> ApiResult {
    check_origin(&headers)?;
    let (actor,connection_id,bucket_id,bucket_name,mount_path): (User,i64,i64,String,String) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db,&headers)?;
        let (connection_id,bucket_id,bucket_name,mount_path): (i64,i64,String,String) = db.query_row("SELECT b.connection_id,b.id,b.bucket_name,m.mount_path FROM s3_mounts m JOIN s3_buckets b ON b.id=m.bucket_id WHERE m.id=?1",[mount_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).map_err(|_|error(StatusCode::NOT_FOUND,"Mount پیدا نشد."))?;
        (actor,connection_id,bucket_id,bucket_name,mount_path)
    };
    let result = privileged_request_async(json!({"action":"s3-mount-stop","mount_id":mount_id,"connection_id":connection_id,"bucket_id":bucket_id})).await?;
    if result["ok"] != true { return Err(error(StatusCode::BAD_GATEWAY,result["error"].as_str().unwrap_or("توقف Mount ناموفق بود."))); }
    let db = state.lock().unwrap();
    db.execute("UPDATE s3_mounts SET status='unmounted',last_error=NULL,updated_at=?1 WHERE id=?2",params![now(),mount_id]).map_err(db_error)?;
    audit(&db,&actor.username,"s3.mount.stop",&format!("mount={mount_id}; bucket_id={bucket_id}"));
    Ok(Json(json!({"ok":true,"mount":mount_public(mount_id,connection_id,bucket_id,&bucket_name,&mount_path,true,false,"unmounted",None)})))
}

pub(super) async fn summary(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db, &headers)?;
    let (connections, healthy, buckets, mounted): (i64,i64,i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM s3_connections),(SELECT COUNT(*) FROM s3_connections WHERE status='connected'),(SELECT COUNT(*) FROM s3_buckets WHERE enabled=1),(SELECT COUNT(*) FROM s3_mounts WHERE status='mounted')", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap_or((0,0,0,0));
    let due_soon: i64 = db.query_row("SELECT COUNT(*) FROM s3_contracts WHERE status IN ('active','upcoming') AND next_due_date BETWEEN date('now') AND date('now','+7 days')", [], |row| row.get(0)).unwrap_or(0);
    let (usage_sum,quota_sum,measured,min_measured,total,quota_count):(Option<i64>,Option<i64>,i64,Option<i64>,i64,i64) = db.query_row("SELECT SUM(u.used_bytes),SUM(u.quota_bytes),SUM(CASE WHEN u.scan_status IN ('measured','stale') AND u.used_bytes IS NOT NULL THEN 1 ELSE 0 END),MIN(CASE WHEN u.scan_status IN ('measured','stale') AND u.used_bytes IS NOT NULL THEN u.measured_at END),COUNT(*),SUM(CASE WHEN u.quota_bytes IS NOT NULL THEN 1 ELSE 0 END) FROM s3_buckets b LEFT JOIN s3_usage_snapshots u ON u.id=(SELECT s.id FROM s3_usage_snapshots s WHERE s.bucket_id=b.id ORDER BY s.id DESC LIMIT 1) WHERE b.enabled=1",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?))).unwrap_or((None,None,0,None,0,0));
    let usage_bytes = (total > 0 && measured == total).then_some(usage_sum).flatten();
    let quota_bytes = (total > 0 && quota_count == total).then_some(quota_sum).flatten();
    let usage_status = if measured == 0 { "not_measured" } else if min_measured.is_none_or(|value| value <= 0 || now().saturating_sub(value) >= 24 * 3600) { "stale" } else { "fresh" };
    Ok(Json(
        json!({"connections":connections,"healthy_connections":healthy,"buckets":buckets,"mounted_buckets":mounted,"contracts_due_soon":due_soon,"usage_bytes":usage_bytes,"quota_bytes":quota_bytes,"usage_status":usage_status}),
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

pub(super) async fn bucket_catalog(State(state):State<AppState>,headers:HeaderMap)->ApiResult{
    let db=state.lock().unwrap();require_admin(&db,&headers)?;
    let mut stmt=db.prepare("SELECT b.id,b.bucket_name,b.display_name,c.display_name,c.provider_type,c.status,b.enabled FROM s3_buckets b JOIN s3_connections c ON c.id=b.connection_id ORDER BY c.display_name COLLATE NOCASE,b.display_name COLLATE NOCASE").map_err(db_error)?;
    let buckets=stmt.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"bucket_name":r.get::<_,String>(1)?,"display_name":r.get::<_,String>(2)?,"connection_name":r.get::<_,String>(3)?,"provider":r.get::<_,String>(4)?,"connection_status":r.get::<_,String>(5)?,"enabled":r.get::<_,bool>(6)?}))).map_err(db_error)?.filter_map(Result::ok).collect::<Vec<_>>();Ok(Json(json!({"buckets":buckets})))
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
        let active_mounts: i64 = db.query_row("SELECT COUNT(*) FROM s3_mounts m JOIN s3_buckets b ON b.id=m.bucket_id WHERE b.connection_id=?1 AND m.status='mounted'",[id],|row|row.get(0)).map_err(db_error)?;
        if active_mounts > 0 {
            return Err(error(StatusCode::CONFLICT,"پیش از حذف اتصال، Mountهای فعال آن را Unmount کن."));
        }
        let app_bindings:i64=db.query_row("SELECT COUNT(*) FROM application_s3_bindings x JOIN s3_buckets b ON b.id=x.bucket_id WHERE b.connection_id=?1",[id],|row|row.get(0)).map_err(db_error)?;
        if app_bindings>0{return Err(error(StatusCode::CONFLICT,"این اتصال در تنظیمات اپلیکیشن‌ها استفاده می‌شود؛ ابتدا Bindingهای اپ را جدا کن."));}
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
    {
        let db = state.lock().unwrap();
        let _ = read_connection(&db, id)?;
    }
    let bucket = input.bucket_name.or_else(|| {
        let db = state.lock().ok()?;
        db.query_row(
            "SELECT bucket_name FROM s3_buckets WHERE connection_id=?1 ORDER BY id LIMIT 1",
            [id],
            |row| row.get(0),
        )
        .ok()
    });
    let Some(bucket) = bucket else {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "برای آزمایش کامل، ابتدا یک Bucket موجود را به اتصال اضافه یا نام آن را انتخاب کن.",
        ));
    };
    if !valid_bucket_name(&bucket) {
        return Err(error(StatusCode::BAD_REQUEST, "نام Bucket معتبر نیست."));
    }
    let mut response = privileged_request_async(json!({"action":"s3-test","id":id,"bucket":bucket})).await?;
    if response["ok"] == true {
        let list_buckets = privileged_request_async(json!({"action":"s3-list-buckets","id":id}))
            .await
            .map(|result| result["ok"] == true)
            .unwrap_or(false);
        if let Some(checks) = response["checks"].as_object_mut() {
            checks.insert("list_buckets".into(), json!(list_buckets));
        }
        if !list_buckets {
            if response["status"] == "connected" {
                response["status"] = json!("degraded");
            }
            let existing = response["message"].as_str().unwrap_or("بررسی Bucket انجام شد.");
            response["message"] = json!(format!("{existing} مجوز فهرست‌کردن همهٔ Bucketها در دسترس نیست."));
        }
    }
    response["bucket"] = json!(bucket);
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
        let warning = status != "connected";
        db.execute("UPDATE s3_connections SET status=?1,last_test_at=?2,last_error=?3,updated_at=?2 WHERE id=?4",params![status,now(),if warning {Some(message.to_string())} else {None::<String>},id]).map_err(db_error)?;
        if response["marker_persisted"] == true {
            let _ = db.execute(
                "UPDATE s3_buckets SET verified_at=?1,updated_at=?1 WHERE connection_id=?2 AND bucket_name=?3",
                params![now(), id, bucket],
            );
        }
        audit(
            &db,
            &actor.username,
            "s3.connection.test",
            &format!(
                "id={id}; bucket={}; outcome={status}; marker={}; marker_persisted={}; write_marker={}; list_objects={}; list_buckets={}; head_object={}; read_marker={}",
                bucket,
                response["marker_key"].as_str().unwrap_or("none"),
                response["marker_persisted"].as_bool().unwrap_or(false),
                response["checks"]["write_marker"].as_bool().unwrap_or(false),
                response["checks"]["list_objects"].as_bool().unwrap_or(false),
                response["checks"]["list_buckets"].as_bool().unwrap_or(false),
                response["checks"]["head_object"].as_bool().unwrap_or(false),
                response["checks"]["read_marker"].as_bool().unwrap_or(false),
            ),
        );
    }
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
    let local_buckets: Vec<(i64,String)> = {
        let db = state.lock().unwrap();
        let mut statement = db.prepare("SELECT id,bucket_name FROM s3_buckets WHERE connection_id=?1 AND enabled=1").map_err(db_error)?;
        let rows = statement.query_map([id], |row| Ok((row.get(0)?, row.get(1)?))).map_err(db_error)?;
        rows.filter_map(Result::ok).collect()
    };
    for (bucket_id, name) in local_buckets {
        let _ = begin_usage_scan(state.clone(), bucket_id, id, name, false).await;
    }
    let db = state.lock().unwrap();
    let mut statement=db.prepare("SELECT b.id,b.bucket_name,b.display_name,b.description,b.enabled,b.verified_at,b.created_at,u.used_bytes,u.object_count,u.quota_bytes,u.quota_source,u.usage_source,u.scan_status,u.measured_at,u.error FROM s3_buckets b LEFT JOIN s3_usage_snapshots u ON u.id=(SELECT s.id FROM s3_usage_snapshots s WHERE s.bucket_id=b.id ORDER BY s.id DESC LIMIT 1) WHERE b.connection_id=?1 ORDER BY b.display_name").map_err(db_error)?;
    let stored:Vec<Value>=statement.query_map([id],|row|Ok(json!({"id":row.get::<_,i64>(0)?,"bucket_name":row.get::<_,String>(1)?,"display_name":row.get::<_,String>(2)?,"description":row.get::<_,String>(3)?,"enabled":row.get::<_,bool>(4)?,"verified_at":row.get::<_,Option<i64>>(5)?,"created_at":row.get::<_,i64>(6)?,"used_bytes":row.get::<_,Option<i64>>(7)?,"object_count":row.get::<_,Option<i64>>(8)?,"quota_bytes":row.get::<_,Option<i64>>(9)?,"quota_source":row.get::<_,Option<String>>(10)?,"usage_source":row.get::<_,Option<String>>(11)?,"scan_status":row.get::<_,Option<String>>(12)?,"measured_at":row.get::<_,Option<i64>>(13)?,"error":row.get::<_,Option<String>>(14)?}))).map_err(db_error)?.filter_map(Result::ok).collect();
    Ok(Json(
        json!({"provider_buckets":provider_buckets,"connections":stored}),
    ))
}
pub(super) async fn set_bucket_quota(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<BucketQuotaInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?
    };
    let quota_bytes = input.quota_bytes
        .map(i64::try_from)
        .transpose()
        .map_err(|_| error(StatusCode::BAD_REQUEST, "سقف Bucket خارج از محدوده است."))?;
    if quota_bytes == Some(0) {
        return Err(error(StatusCode::BAD_REQUEST, "برای سقف نامشخص، فیلد را خالی بگذار."));
    }
    let db = state.lock().unwrap();
    let bucket: String = db.query_row("SELECT bucket_name FROM s3_buckets WHERE id=?1", [id], |row| row.get(0))
        .map_err(|_| error(StatusCode::NOT_FOUND, "Bucket در ARG پیدا نشد."))?;
    let latest_id: Option<i64> = db.query_row("SELECT id FROM s3_usage_snapshots WHERE bucket_id=?1 ORDER BY id DESC LIMIT 1", [id], |row| row.get(0)).ok();
    if let Some(snapshot_id) = latest_id {
        db.execute("UPDATE s3_usage_snapshots SET quota_bytes=?1,quota_source=?2 WHERE id=?3", params![quota_bytes,quota_bytes.map(|_| "manual"),snapshot_id]).map_err(db_error)?;
    } else {
        db.execute("INSERT INTO s3_usage_snapshots(bucket_id,quota_bytes,quota_source,usage_source,scan_status) VALUES (?1,?2,?3,'inventory','not_measured')", params![id,quota_bytes,quota_bytes.map(|_| "manual")]).map_err(db_error)?;
    }
    audit(&db, &actor.username, "s3.bucket.quota.update", &format!("bucket_id={id}; quota_bytes={}", quota_bytes.map(|value| value.to_string()).unwrap_or_else(|| "unknown".into())));
    Ok(Json(json!({"ok":true,"quota_bytes":quota_bytes,"bucket":bucket})))
}

pub(super) async fn bucket_usage(State(state): State<AppState>, Path(id): Path<i64>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_user(&db,&headers)?;
    let usage: Value = db.query_row("SELECT used_bytes,object_count,quota_bytes,quota_source,usage_source,scan_status,measured_at,error FROM s3_usage_snapshots WHERE bucket_id=?1 ORDER BY id DESC LIMIT 1", [id], |row| Ok(json!({"used_bytes":row.get::<_,Option<i64>>(0)?,"object_count":row.get::<_,Option<i64>>(1)?,"quota_bytes":row.get::<_,Option<i64>>(2)?,"quota_source":row.get::<_,Option<String>>(3)?,"usage_source":row.get::<_,Option<String>>(4)?,"scan_status":row.get::<_,Option<String>>(5)?,"measured_at":row.get::<_,Option<i64>>(6)?,"error":row.get::<_,Option<String>>(7)?}))).optional().map_err(db_error)?.unwrap_or_else(|| json!({"used_bytes":null,"object_count":null,"quota_bytes":null,"quota_source":null,"usage_source":"inventory","scan_status":"not_measured","measured_at":null,"error":null}));
    Ok(Json(json!({"usage":usage})))
}

async fn begin_usage_scan(state: AppState, bucket_id: i64, connection_id: i64, bucket_name: String, force: bool) -> Result<bool, (StatusCode, Json<Value>)> {
    let snapshot_id = {
        let db = state.lock().unwrap();
        let latest: Option<(String,Option<i64>)> = db.query_row("SELECT scan_status,measured_at FROM s3_usage_snapshots WHERE bucket_id=?1 ORDER BY id DESC LIMIT 1", [bucket_id], |row| Ok((row.get(0)?,row.get(1)?))).ok();
        if let Some((status, measured_at)) = latest {
            if !usage_scan_allowed(Some(&status),measured_at,now(),force) { return Ok(false); }
        } else if !usage_scan_allowed(None,None,now(),force) {
            return Ok(false);
        }
        let quota: Option<(i64,Option<String>)> = db.query_row("SELECT quota_bytes,quota_source FROM s3_usage_snapshots WHERE bucket_id=?1 ORDER BY id DESC LIMIT 1", [bucket_id], |row| Ok((row.get(0)?,row.get(1)?))).ok();
        db.execute("INSERT INTO s3_usage_snapshots(bucket_id,quota_bytes,quota_source,usage_source,scan_status,measured_at) VALUES (?1,?2,?3,'inventory','scanning',?4)", params![bucket_id,quota.as_ref().map(|item|item.0),quota.as_ref().and_then(|item|item.1.as_deref()),now()]).map_err(db_error)?;
        db.last_insert_rowid()
    };
    tokio::spawn(async move {
        let _scan_permit = S3_SCAN_SEMAPHORE.acquire().await.expect("S3 scan semaphore remains open");
        let result = privileged_request_async(json!({"action":"s3-scan-usage","id":connection_id,"bucket":bucket_name})).await;
        let db = state.lock().unwrap();
        match result {
            Ok(value) if value["ok"] == true => {
                let used = value["used_bytes"].as_u64().and_then(|n| i64::try_from(n).ok());
                let count = value["object_count"].as_u64().and_then(|n| i64::try_from(n).ok());
                if let (Some(used), Some(count)) = (used,count) {
                    let _ = db.execute("UPDATE s3_usage_snapshots SET used_bytes=?1,object_count=?2,usage_source='inventory',scan_status='measured',measured_at=?3,error=NULL WHERE id=?4", params![used,count,now(),snapshot_id]);
                } else {
                    let _ = db.execute("UPDATE s3_usage_snapshots SET scan_status='failed',measured_at=?1,error='مقادیر اسکن خارج از محدوده بودند.' WHERE id=?2", params![now(),snapshot_id]);
                }
            }
            Ok(value) => {
                let message = value["error"].as_str().unwrap_or("خواندن فهرست آبجکت‌ها ناموفق بود.");
                let _ = db.execute("UPDATE s3_usage_snapshots SET scan_status='failed',measured_at=?1,error=?2 WHERE id=?3", params![now(),message,snapshot_id]);
            }
            Err((_, Json(value))) => {
                let message = value["error"].as_str().unwrap_or("اسکن Bucket ناموفق بود.");
                let _ = db.execute("UPDATE s3_usage_snapshots SET scan_status='failed',measured_at=?1,error=?2 WHERE id=?3", params![now(),message,snapshot_id]);
            }
        }
    });
    Ok(true)
}

pub(super) async fn scan_bucket_usage(State(state): State<AppState>, Path(id): Path<i64>, headers: HeaderMap) -> ApiResult {
    check_origin(&headers)?;
    let (actor,connection_id,name) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db,&headers)?;
        let (connection_id,name): (i64,String) = db.query_row("SELECT connection_id,bucket_name FROM s3_buckets WHERE id=?1 AND enabled=1", [id], |row| Ok((row.get(0)?,row.get(1)?))).map_err(|_| error(StatusCode::NOT_FOUND,"Bucket فعال پیدا نشد."))?;
        (actor,connection_id,name)
    };
    let started = begin_usage_scan(state.clone(),id,connection_id,name,true).await?;
    if started {
        let db = state.lock().unwrap();
        audit(&db,&actor.username,"s3.bucket.usage.scan",&format!("bucket_id={id}"));
    }
    Ok(Json(json!({"ok":true,"started":started,"scan_status":if started {"scanning"} else {"cached_or_running"}})))
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
    let active_mount: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM s3_mounts WHERE bucket_id=?1 AND status='mounted')",[id],|row|row.get(0)).map_err(db_error)?;
    if active_mount {
        return Err(error(StatusCode::CONFLICT,"پیش از قطع Bucket از ARG، Mount فعال آن را Unmount کن."));
    }
    let app_bindings:i64=db.query_row("SELECT COUNT(*) FROM application_s3_bindings WHERE bucket_id=?1",[id],|row|row.get(0)).map_err(db_error)?;
    if app_bindings>0{return Err(error(StatusCode::CONFLICT,"این Bucket به یک یا چند اپ متصل است؛ ابتدا Bindingهای اپ را جدا کن."));}
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
    invalidate_usage_cache(&db,id,input["bucket"].as_str().unwrap_or(""));
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
    format!("{S3_TRANSFER_DIR}/{}", token())
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
    prepare_transfer_directory().map_err(|message| error(StatusCode::INTERNAL_SERVER_ERROR, &message))?;
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
    invalidate_usage_cache(&db,id,&query.bucket);
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
    prepare_transfer_directory().map_err(|message| error(StatusCode::INTERNAL_SERVER_ERROR, &message))?;
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

#[derive(Deserialize)]
pub(super) struct LocalTransferInput {
    bucket: String,
    key: String,
    local_path: String,
    #[serde(default)]
    content_type: Option<String>,
    #[serde(default)]
    overwrite: bool,
}

pub(super) async fn transfer_local_to_bucket(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<LocalTransferInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?;
        let _ = read_connection(&db, id)?;
        actor
    };
    if !valid_bucket_name(&input.bucket)
        || !valid_object_key(&input.key)
        || !input.local_path.starts_with('/')
        || input.local_path.len() > 4096
        || input.local_path.chars().any(char::is_control)
    {
        return Err(error(StatusCode::BAD_REQUEST, "مسیر فایل یا مقصد S3 معتبر نیست."));
    }
    let result = privileged_request_async(json!({"action":"s3-upload-local-file","id":id,"bucket":input.bucket,"key":input.key,"local_path":input.local_path,"content_type":input.content_type,"overwrite":input.overwrite})).await?;
    if result["ok"] != true {
        return Err(error(StatusCode::BAD_GATEWAY, result["error"].as_str().unwrap_or("انتقال فایل به فضای ابری ناموفق بود.")));
    }
    let db = state.lock().unwrap();
    invalidate_usage_cache(&db,id,&input.bucket);
    audit(&db, &actor.username, "s3.transfer.local_to_bucket", &format!("connection={id}; bucket={}; key_length={}; size_bytes={}", input.bucket, input.key.len(), result["size_bytes"].as_u64().unwrap_or(0)));
    Ok(Json(result))
}

pub(super) async fn transfer_bucket_to_local(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<LocalTransferInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?;
        let _ = read_connection(&db, id)?;
        actor
    };
    if !valid_bucket_name(&input.bucket)
        || !valid_object_key(&input.key)
        || !input.local_path.starts_with('/')
        || input.local_path.len() > 4096
        || input.local_path.chars().any(char::is_control)
    {
        return Err(error(StatusCode::BAD_REQUEST, "مسیر فایل یا مبدأ S3 معتبر نیست."));
    }
    let result = privileged_request_async(json!({"action":"s3-download-local-file","id":id,"bucket":input.bucket,"key":input.key,"local_path":input.local_path})).await?;
    if result["ok"] != true {
        return Err(error(StatusCode::BAD_GATEWAY, result["error"].as_str().unwrap_or("انتقال فایل به سرور ناموفق بود.")));
    }
    let db = state.lock().unwrap();
    audit(&db, &actor.username, "s3.transfer.bucket_to_local", &format!("connection={id}; bucket={}; key_length={}; size_bytes={}", input.bucket, input.key.len(), result["size_bytes"].as_u64().unwrap_or(0)));
    Ok(Json(result))
}
