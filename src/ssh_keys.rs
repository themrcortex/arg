use super::*;
use std::{
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path as FsPath, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

const SSH_KEY_ROOT: &str = "/var/lib/arg/ssh-keys";
const GITHUB_META_URL: &str = "https://api.github.com/meta";
const GITHUB_HOST_KEYS_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateRequest {
    name: String,
    purpose: String,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    application_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RenameRequest {
    name: String,
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS ssh_keys(
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            purpose TEXT NOT NULL CHECK(purpose IN ('personal','deploy')),
            repository TEXT,
            application_id TEXT REFERENCES applications(id) ON DELETE SET NULL,
            public_key TEXT NOT NULL,
            fingerprint TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            last_used_at INTEGER
         );
         CREATE INDEX IF NOT EXISTS ssh_keys_created ON ssh_keys(created_at DESC);",
    )
}

pub(super) fn valid_key_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_name(value: &str) -> Result<String, String> {
    let name = value.trim();
    if name.is_empty() || name.len() > 80 || name.chars().any(char::is_control) {
        return Err("نام کلید باید بین ۱ تا ۸۰ نویسه و بدون نویسهٔ کنترلی باشد.".into());
    }
    Ok(name.to_owned())
}

pub(super) fn is_github_ssh_repository(value: &str) -> bool {
    let path = if let Some(path) = value.strip_prefix("git@github.com:") {
        path.to_owned()
    } else {
        let Ok(url) = url::Url::parse(value) else {
            return false;
        };
        if url.scheme() != "ssh"
            || url.host_str() != Some("github.com")
            || url.username() != "git"
            || url.password().is_some()
            || url.port().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return false;
        }
        url.path().trim_start_matches('/').to_owned()
    };
    let path = path.strip_suffix(".git").unwrap_or(&path);
    let mut parts = path.split('/');
    let Some(owner) = parts.next() else {
        return false;
    };
    let Some(repository) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && [owner, repository].iter().all(|part| {
            !part.is_empty()
                && *part != "."
                && *part != ".."
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
}

fn key_root() -> Result<PathBuf, String> {
    let path = PathBuf::from(SSH_KEY_ROOT);
    fs::create_dir_all(&path).map_err(|_| "پوشهٔ امن کلیدهای SSH ساخته نشد.".to_owned())?;
    let metadata =
        fs::symlink_metadata(&path).map_err(|_| "پوشهٔ کلیدهای SSH قابل بررسی نیست.".to_owned())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("مسیر کلیدهای SSH باید یک پوشهٔ واقعی باشد.".into());
    }
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
        .map_err(|_| "دسترسی پوشهٔ کلیدهای SSH محدود نشد.".to_owned())?;
    let canonical = path
        .canonicalize()
        .map_err(|_| "پوشهٔ کلیدهای SSH resolve نشد.".to_owned())?;
    if canonical != path {
        return Err("مسیر واقعی کلیدهای SSH با مسیر مدیریت‌شده یکی نیست.".into());
    }
    Ok(path)
}

fn id() -> String {
    let mut bytes = [0_u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn private_path(root: &FsPath, key_id: &str) -> Result<PathBuf, String> {
    if !valid_key_id(key_id) {
        return Err("شناسهٔ کلید SSH معتبر نیست.".into());
    }
    Ok(root.join(key_id))
}

fn key_file_is_private(path: &FsPath) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.mode() & 0o777 == 0o600
            && metadata.uid() == unsafe { libc::geteuid() }
    })
}

pub(super) fn validate_key_for_deployment(
    db: &Connection,
    application_id: &str,
    repository: &str,
    key_id: &str,
) -> Result<(), String> {
    if !valid_key_id(key_id) || !is_github_ssh_repository(repository) {
        return Err("برای Git SSH باید مخزن GitHub معتبر و کلید SSH انتخاب شود.".into());
    }
    let (purpose, bound_repo, bound_app): (String, Option<String>, Option<String>) = db
        .query_row(
            "SELECT purpose,repository,application_id FROM ssh_keys WHERE id=?1",
            [key_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|_| "فهرست کلیدهای SSH خوانده نشد.".to_owned())?
        .ok_or_else(|| "کلید SSH پیدا نشد یا حذف شده است.".to_owned())?;
    if purpose == "deploy" {
        if bound_repo.as_deref() != Some(repository)
            || bound_app
                .as_deref()
                .is_some_and(|app| app != application_id)
        {
            return Err("این Deploy Key به مخزن یا اپلیکیشن دیگری محدود شده است.".into());
        }
    } else if purpose != "personal" {
        return Err("نوع کلید SSH پشتیبانی نمی‌شود.".into());
    }
    let root = key_root()?;
    let path = private_path(&root, key_id)?;
    if !key_file_is_private(&path) {
        return Err(
            "فایل خصوصی این کلید پیدا نشد یا مجوزهای امن ندارد؛ کلید را دوباره بساز.".into(),
        );
    }
    Ok(())
}

pub(super) fn deployment_key_path(
    db: &Connection,
    application_id: &str,
    repository: &str,
    key_id: &str,
) -> Result<PathBuf, String> {
    validate_key_for_deployment(db, application_id, repository, key_id)?;
    let root = key_root()?;
    let path = private_path(&root, key_id)?;
    db.execute(
        "UPDATE ssh_keys SET last_used_at=?1 WHERE id=?2",
        params![now(), key_id],
    )
    .map_err(|_| "زمان استفاده از کلید SSH ثبت نشد.".to_owned())?;
    Ok(path)
}

pub(super) fn known_hosts_path() -> Result<PathBuf, String> {
    Ok(key_root()?.join("github_known_hosts"))
}

pub(super) async fn ensure_github_host_keys() -> Result<PathBuf, String> {
    let path = known_hosts_path()?;
    if fs::symlink_metadata(&path).is_ok_and(|metadata| {
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.mode() & 0o077 == 0
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.len() > 32
            && metadata
                .modified()
                .ok()
                .and_then(|time| time.elapsed().ok())
                .is_some_and(|age| age < GITHUB_HOST_KEYS_MAX_AGE)
    }) {
        return Ok(path);
    }
    refresh_github_host_keys().await
}

async fn fetch_github_host_keys() -> Result<(String, Vec<String>), String> {
    let client = reqwest::Client::builder()
        .https_only(true)
        .min_tls_version(reqwest::tls::Version::TLS_1_2)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(12))
        .connect_timeout(Duration::from_secs(5))
        .user_agent(concat!("ARG-Deploy-Keys/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "ساخت اتصال HTTPS برای دریافت کلید میزبان GitHub ناموفق بود.".to_owned())?;
    let response = client
        .get(GITHUB_META_URL)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| {
            "کلید میزبان GitHub دریافت نشد؛ دسترسی HTTPS به api.github.com را بررسی کن.".to_owned()
        })?;
    if !response.status().is_success() {
        return Err(format!(
            "GitHub Meta API با وضعیت {} پاسخ داد.",
            response.status()
        ));
    }
    let metadata = response
        .json::<Value>()
        .await
        .map_err(|_| "پاسخ GitHub Meta API قابل‌خواندن نیست.".to_owned())?;
    let raw_keys = metadata["ssh_keys"]
        .as_array()
        .ok_or_else(|| "GitHub Meta API فهرست کلید SSH را برنگرداند.".to_owned())?;
    let mut lines = Vec::new();
    for item in raw_keys {
        let Some(key) = item.as_str() else { continue };
        let parts = key.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 2
            || !["ssh-ed25519", "ssh-rsa", "ecdsa-sha2-nistp256"].contains(&parts[0])
            || parts[1].len() > 4096
        {
            continue;
        }
        lines.push(format!("github.com {} {}", parts[0], parts[1]));
    }
    lines.sort();
    lines.dedup();
    if lines.is_empty() || lines.len() > 16 {
        return Err("فهرست کلیدهای میزبان GitHub خالی یا نامعتبر است.".into());
    }
    let content = format!("{}\n", lines.join("\n"));
    let fingerprints = validate_known_hosts_content(&content).await?;
    Ok((content, fingerprints))
}

async fn validate_known_hosts_content(content: &str) -> Result<Vec<String>, String> {
    let root = key_root()?;
    let mut fingerprints = Vec::new();
    for line in content.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 || fields[0] != "github.com" {
            return Err("ساختار Host Key دریافتی از GitHub معتبر نیست.".into());
        }
        let public_key = format!("{} {} github.com\n", fields[1], fields[2]);
        let temp = root.join(format!(".github-host-{}.pub", id()));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        let mut file = options
            .open(&temp)
            .map_err(|_| "فایل موقت Host Key ساخته نشد.".to_owned())?;
        use std::io::Write as _;
        if file.write_all(public_key.as_bytes()).is_err() || file.sync_all().is_err() {
            let _ = fs::remove_file(&temp);
            return Err("فایل موقت Host Key ذخیره نشد.".into());
        }
        drop(file);
        let check = tokio::time::timeout(
            Duration::from_secs(8),
            tokio::process::Command::new("/usr/bin/ssh-keygen")
                .args(["-lf"])
                .arg(&temp)
                .output(),
        )
        .await;
        let _ = fs::remove_file(&temp);
        let output = check
            .map_err(|_| "اعتبارسنجی Host Key زمان‌بر شد.".to_owned())?
            .map_err(|_| "ssh-keygen برای بررسی Host Key در دسترس نیست.".to_owned())?;
        if !output.status.success() {
            return Err("یکی از کلیدهای دریافتی از GitHub از نظر OpenSSH معتبر نیست.".into());
        }
        let fingerprint = String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .nth(1)
            .map(str::to_owned)
            .ok_or_else(|| "اثر انگشت Host Key محاسبه نشد.".to_owned())?;
        fingerprints.push(fingerprint);
    }
    if fingerprints.is_empty() || fingerprints.len() != content.lines().count() {
        return Err("همهٔ Host Keyهای GitHub اعتبارسنجی نشدند.".into());
    }
    Ok(fingerprints)
}

async fn refresh_github_host_keys() -> Result<PathBuf, String> {
    let (content, _) = fetch_github_host_keys().await?;
    let root = key_root()?;
    let path = root.join("github_known_hosts");
    let temp = root.join(format!(".github_known_hosts.{}.tmp", id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options
        .open(&temp)
        .map_err(|_| "فایل موقت Host Key ساخته نشد.".to_owned())?;
    use std::io::Write as _;
    file.write_all(content.as_bytes())
        .map_err(|_| "فایل Host Key نوشته نشد.".to_owned())?;
    file.sync_all()
        .map_err(|_| "فایل Host Key ذخیره نشد.".to_owned())?;
    drop(file);
    fs::rename(&temp, &path)
        .map_err(|_| "به‌روزرسانی امن Host Keyهای GitHub ناموفق بود.".to_owned())?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .map_err(|_| "مجوز Host Keyهای GitHub محدود نشد.".to_owned())?;
    Ok(path)
}

pub(super) async fn list(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let mut statement = db.prepare("SELECT id,name,purpose,repository,application_id,public_key,fingerprint,created_at,last_used_at FROM ssh_keys ORDER BY created_at DESC").map_err(db_error)?;
    let rows = statement.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_,String>(0)?, "name": row.get::<_,String>(1)?,
            "purpose": row.get::<_,String>(2)?, "repository": row.get::<_,Option<String>>(3)?,
            "application_id": row.get::<_,Option<String>>(4)?, "public_key": row.get::<_,String>(5)?,
            "fingerprint": row.get::<_,String>(6)?, "created_at": row.get::<_,i64>(7)?,
            "last_used_at": row.get::<_,Option<i64>>(8)?
        }))
    }).map_err(db_error)?;
    let keys = rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
    let host_path = PathBuf::from(SSH_KEY_ROOT).join("github_known_hosts");
    let trusted_host_keys_updated_at = fs::metadata(&host_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    Ok(Json(
        json!({"keys":keys,"github_host_keys_updated_at":trusted_host_keys_updated_at}),
    ))
}

pub(super) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let name =
        valid_name(&input.name).map_err(|message| error(StatusCode::BAD_REQUEST, &message))?;
    let purpose = input.purpose.as_str();
    if !["personal", "deploy"].contains(&purpose) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نوع کلید باید personal یا deploy باشد.",
        ));
    }
    let repository = input
        .repository
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if purpose == "deploy" && !repository.is_some_and(is_github_ssh_repository) {
        return Err(error(StatusCode::BAD_REQUEST, "Deploy Key باید به URL مخزن خصوصی GitHub مانند git@github.com:owner/repo.git محدود شود."));
    }
    if purpose == "personal" && (repository.is_some() || input.application_id.is_some()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "کلید شخصی به مخزن یا اپلیکیشن خاصی محدود نمی‌شود.",
        ));
    }
    if input
        .application_id
        .as_deref()
        .is_some_and(|app| !apps::valid_uuid(app))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "شناسهٔ اپلیکیشن نامعتبر است.",
        ));
    }
    let actor = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        if let Some(app_id) = input.application_id.as_deref() {
            let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL)", params![app_id,owner], |row| row.get(0)).map_err(db_error)?;
            if !exists {
                return Err(error(
                    StatusCode::NOT_FOUND,
                    "اپلیکیشن برای این VPS پیدا نشد.",
                ));
            }
        }
        actor
    };
    let root = key_root().map_err(|message| error(StatusCode::INTERNAL_SERVER_ERROR, &message))?;
    let key_id = id();
    let temporary = root.join(format!(".{key_id}.tmp"));
    let temporary_public = root.join(format!(".{key_id}.tmp.pub"));
    let generated = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new("/usr/bin/ssh-keygen")
            .args([
                "-q",
                "-t",
                "ed25519",
                "-N",
                "",
                "-C",
                &format!("arg:{key_id}"),
                "-f",
            ])
            .arg(&temporary)
            .output(),
    )
    .await;
    let generated = match generated {
        Ok(Ok(output)) if output.status.success() => Ok(()),
        Ok(Ok(_)) => Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ssh-keygen نتوانست کلید Ed25519 بسازد؛ پیش‌نیاز OpenSSH Client را بررسی کن.",
        )),
        Ok(Err(_)) => Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "OpenSSH Client نصب نیست یا ssh-keygen اجرا نشد.",
        )),
        Err(_) => Err(error(
            StatusCode::GATEWAY_TIMEOUT,
            "ساخت کلید SSH از زمان مجاز بیشتر طول کشید.",
        )),
    };
    if let Err(failure) = generated {
        let _ = fs::remove_file(&temporary);
        let _ = fs::remove_file(&temporary_public);
        return Err(failure);
    }
    let public = match fs::read_to_string(&temporary_public) {
        Ok(public) => public,
        Err(_) => {
            let _ = fs::remove_file(&temporary);
            let _ = fs::remove_file(&temporary_public);
            return Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "کلید عمومی تولید نشد.",
            ));
        }
    };
    if public.len() > 2048
        || !public.starts_with("ssh-ed25519 ")
        || public.chars().any(char::is_control)
    {
        let _ = fs::remove_file(&temporary);
        let _ = fs::remove_file(&temporary_public);
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "خروجی کلید عمومی OpenSSH معتبر نیست.",
        ));
    }
    if fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600)).is_err()
        || fs::set_permissions(&temporary_public, fs::Permissions::from_mode(0o644)).is_err()
    {
        let _ = fs::remove_file(&temporary);
        let _ = fs::remove_file(&temporary_public);
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "مجوز امن فایل‌های کلید تنظیم نشد.",
        ));
    }
    let fingerprint_result = tokio::process::Command::new("/usr/bin/ssh-keygen")
        .args(["-lf"])
        .arg(&temporary_public)
        .output()
        .await;
    let fingerprint_output = match fingerprint_result {
        Ok(output) => output,
        Err(_) => {
            let _ = fs::remove_file(&temporary);
            let _ = fs::remove_file(&temporary_public);
            return Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "اثر انگشت کلید محاسبه نشد.",
            ));
        }
    };
    let fingerprint = String::from_utf8_lossy(&fingerprint_output.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    if !fingerprint.starts_with("SHA256:") {
        let _ = fs::remove_file(&temporary);
        let _ = fs::remove_file(&temporary_public);
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "اثر انگشت کلید SSH معتبر نیست.",
        ));
    }
    let private = root.join(&key_id);
    let public_path = root.join(format!("{key_id}.pub"));
    if fs::rename(&temporary, &private).is_err() {
        let _ = fs::remove_file(&temporary);
        let _ = fs::remove_file(&temporary_public);
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ذخیرهٔ خصوصی کلید SSH ناموفق بود.",
        ));
    }
    if fs::rename(&temporary_public, &public_path).is_err() {
        let _ = fs::remove_file(&private);
        let _ = fs::remove_file(&temporary_public);
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ذخیرهٔ کلید عمومی ناموفق بود.",
        ));
    }
    let at = now();
    let db = state.lock().unwrap();
    if let Err(problem) = db.execute("INSERT INTO ssh_keys(id,name,purpose,repository,application_id,public_key,fingerprint,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![key_id,name,purpose,repository,input.application_id,public.trim(),fingerprint,at]) {
        let _=fs::remove_file(&private); let _=fs::remove_file(&public_path);
        return Err(db_error(problem));
    }
    audit(
        &db,
        &actor,
        "ssh_key.create",
        &format!(
            "id={key_id}; purpose={purpose}; application={}; repository={}",
            input.application_id.as_deref().unwrap_or("global"),
            repository.unwrap_or("none")
        ),
    );
    Ok(Json(
        json!({"id":key_id,"name":name,"purpose":purpose,"repository":repository,"application_id":input.application_id,"public_key":public.trim(),"fingerprint":fingerprint,"created_at":at,"private_key_saved_on_server":true}),
    ))
}

pub(super) async fn rename(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key_id): Path<String>,
    Json(input): Json<RenameRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let name =
        valid_name(&input.name).map_err(|message| error(StatusCode::BAD_REQUEST, &message))?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let changed = db
        .execute(
            "UPDATE ssh_keys SET name=?1 WHERE id=?2",
            params![name, key_id],
        )
        .map_err(db_error)?;
    if changed == 0 {
        return Err(error(StatusCode::NOT_FOUND, "کلید SSH پیدا نشد."));
    }
    audit(
        &db,
        &actor.username,
        "ssh_key.rename",
        &format!("id={key_id}; name={name}"),
    );
    Ok(Json(json!({"id":key_id,"name":name})))
}

pub(super) async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key_id): Path<String>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_key_id(&key_id) {
        return Err(error(StatusCode::BAD_REQUEST, "شناسهٔ کلید SSH معتبر نیست."));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM ssh_keys WHERE id=?1)",
            [&key_id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if !exists {
        return Err(error(StatusCode::NOT_FOUND, "کلید SSH پیدا نشد."));
    }
    let mut statement = db.prepare("SELECT source_json FROM deployment_jobs WHERE state IN ('queued','validating','running','verifying')").map_err(db_error)?;
    let sources = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    let active = sources.iter().any(|source| {
        serde_json::from_str::<Value>(source)
            .ok()
            .and_then(|value| {
                value
                    .get("ssh_key_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .as_deref()
            == Some(key_id.as_str())
    });
    if active {
        return Err(error(
            StatusCode::CONFLICT,
            "این کلید در Job استقرار فعال استفاده می‌شود؛ پس از پایان یا لغو Job دوباره تلاش کن.",
        ));
    }
    let root = key_root().map_err(|message| error(StatusCode::INTERNAL_SERVER_ERROR, &message))?;
    let private =
        private_path(&root, &key_id).map_err(|message| error(StatusCode::BAD_REQUEST, &message))?;
    let public = root.join(format!("{key_id}.pub"));
    for path in [&private, &public] {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                fs::remove_file(path).map_err(|_| {
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "حذف یکی از فایل‌های کلید SSH ناموفق بود؛ رکورد نگه داشته شد.",
                    )
                })?
            }
            Ok(_) => {
                return Err(error(
                    StatusCode::CONFLICT,
                    "فایل کلید SSH معمولی نیست؛ حذف برای ایمنی متوقف شد.",
                ))
            }
            Err(problem) if problem.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "فایل کلید SSH بررسی نشد.",
                ))
            }
        }
    }
    db.execute("DELETE FROM ssh_keys WHERE id=?1", [&key_id])
        .map_err(db_error)?;
    audit(&db, &actor, "ssh_key.delete", &format!("id={key_id}"));
    Ok(Json(
        json!({"deleted":true,"message":"کلید از ARG حذف شد؛ برای لغو دسترسی، کلید عمومی را از تنظیمات Git provider هم حذف کن."}),
    ))
}

pub(super) async fn host_keys(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let path = PathBuf::from(SSH_KEY_ROOT).join("github_known_hosts");
    let updated_at = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    Ok(Json(
        json!({"provider":"github.com","updated_at":updated_at,"fresh_for_seconds":GITHUB_HOST_KEYS_MAX_AGE.as_secs()}),
    ))
}

pub(super) async fn refresh_host_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    let path = refresh_github_host_keys()
        .await
        .map_err(|message| error(StatusCode::BAD_GATEWAY, &message))?;
    let updated_at = fs::metadata(&path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor,
        "ssh_key.github_host_keys.refresh",
        "loaded and validated SSH host keys from GitHub Meta API over HTTPS",
    );
    Ok(Json(
        json!({"provider":"github.com","updated_at":updated_at,"fresh_for_seconds":GITHUB_HOST_KEYS_MAX_AGE.as_secs()}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_github_ssh_repository_forms() {
        assert!(is_github_ssh_repository("git@github.com:org/project.git"));
        assert!(is_github_ssh_repository("git@github.com:org/project"));
        assert!(is_github_ssh_repository(
            "ssh://git@github.com/org/project.git"
        ));
        assert!(!is_github_ssh_repository(
            "git@github.com.attacker.invalid:org/project.git"
        ));
        assert!(!is_github_ssh_repository(
            "ssh://git@github.com:2222/org/project.git"
        ));
        assert!(!is_github_ssh_repository(
            "ssh://git@github.com/org/../secret.git"
        ));
        assert!(!is_github_ssh_repository(
            "https://github.com/org/project.git"
        ));
    }

    #[test]
    fn key_ids_are_fixed_lowercase_hex_identifiers() {
        assert!(valid_key_id("0123456789abcdef0123456789abcdef"));
        assert!(!valid_key_id("0123456789abcdef0123456789abcdeg"));
        assert!(!valid_key_id("../known_hosts"));
        assert!(!valid_key_id("0123456789abcdef"));
    }

    #[test]
    fn migration_is_idempotent_and_unlinks_deleted_applications() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE applications(id TEXT PRIMARY KEY);")
            .unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        db.execute("INSERT INTO applications(id) VALUES('app')", [])
            .unwrap();
        db.execute(
            "INSERT INTO ssh_keys(id,name,purpose,repository,application_id,public_key,fingerprint,created_at) VALUES('0123456789abcdef0123456789abcdef','deploy','deploy','git@github.com:org/repo.git','app','ssh-ed25519 AAAA test','SHA256:test',1)",
            [],
        )
        .unwrap();
        db.execute("DELETE FROM applications WHERE id='app'", [])
            .unwrap();
        let app_id: Option<String> = db
            .query_row(
                "SELECT application_id FROM ssh_keys WHERE id='0123456789abcdef0123456789abcdef'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(app_id, None);
    }
}
