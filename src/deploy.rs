use super::*;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream;
use futures_util::StreamExt;
use std::{
    ffi::OsStr,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Component, Path as FsPath, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime},
};

const MAX_ARCHIVE_BYTES: usize = 256 * 1024 * 1024;
const MAX_RELEASE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 20_000;
const JOB_ACTIVE: &[&str] = &["queued", "validating", "running", "verifying"];

#[derive(Deserialize, Serialize, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Source {
    Git {
        repository: String,
        reference: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ssh_key_id: Option<String>,
    },
    Directory {
        path: String,
    },
    Archive {
        sha256: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeployRequest {
    source: Source,
    idempotency_key: String,
    #[serde(default)]
    healthcheck_file: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RetryRequest {
    idempotency_key: String,
}

#[derive(Deserialize, Default)]
pub(super) struct JobLogsQuery {
    after: Option<i64>,
    before: Option<i64>,
    limit: Option<i64>,
}

pub(super) fn deployment_root() -> PathBuf {
    env::var_os("ARG_DEPLOY_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/arg/deployments"))
}

fn application_tree_for_purge(app_id: &str) -> Result<Option<PathBuf>, String> {
    if !apps::valid_uuid(app_id) {
        return Err("شناسهٔ اپلیکیشن معتبر نیست.".into());
    }
    let root = deployment_root();
    let root_metadata = match fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("ریشهٔ استقرار قابل بررسی نیست: {error}")),
    };
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err("ریشهٔ استقرار یک پوشهٔ واقعی نیست؛ حذف برای ایمنی متوقف شد.".into());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("ریشهٔ استقرار قابل resolve نیست: {error}"))?;
    let apps_path = root.join("apps");
    let apps_metadata = match fs::symlink_metadata(&apps_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("پوشهٔ اپلیکیشن‌ها قابل بررسی نیست: {error}")),
    };
    if !apps_metadata.is_dir() || apps_metadata.file_type().is_symlink() {
        return Err("پوشهٔ apps یک پوشهٔ واقعی نیست؛ حذف برای ایمنی متوقف شد.".into());
    }
    let canonical_apps = apps_path
        .canonicalize()
        .map_err(|error| format!("پوشهٔ apps قابل resolve نیست: {error}"))?;
    if canonical_apps.parent() != Some(canonical_root.as_path()) {
        return Err("پوشهٔ apps از ریشهٔ استقرار خارج شده است.".into());
    }
    let app_path = canonical_apps.join(app_id);
    let metadata = match fs::symlink_metadata(&app_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("پوشهٔ اپ قابل بررسی نیست: {error}")),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("پوشهٔ اپلیکیشن واقعی نیست یا symlink است؛ حذف برای ایمنی متوقف شد.".into());
    }
    let canonical_app = app_path
        .canonicalize()
        .map_err(|error| format!("پوشهٔ اپ قابل resolve نیست: {error}"))?;
    if canonical_app != app_path || !canonical_app.starts_with(&canonical_apps) {
        return Err("مسیر واقعی اپلیکیشن با مسیر مدیریت‌شدهٔ ARG یکی نیست.".into());
    }
    let mountinfo = fs::read_to_string("/proc/self/mountinfo")
        .map_err(|error| format!("Mountهای سیستم قابل بررسی نیستند؛ حذف فایل‌ها متوقف شد: {error}"))?;
    for line in mountinfo.lines() {
        let Some(raw_mount) = line.split_whitespace().nth(4) else {
            continue;
        };
        let mount = decode_mountinfo_path(raw_mount);
        if mount == canonical_app || mount.starts_with(&canonical_app) {
            return Err(format!(
                "داخل مسیر اپلیکیشن یک Mount فعال وجود دارد ({})؛ ابتدا آن را جدا کن.",
                mount.display()
            ));
        }
    }
    Ok(Some(canonical_app))
}

fn decode_mountinfo_path(value: &str) -> PathBuf {
    let mut decoded = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() {
            let escape = &value[index + 1..index + 4];
            let character = match escape {
                "040" => Some(' '),
                "011" => Some('\t'),
                "012" => Some('\n'),
                "134" => Some('\\'),
                _ => None,
            };
            if let Some(character) = character {
                decoded.push(character);
                index += 4;
                continue;
            }
        }
        let character = value[index..].chars().next().unwrap_or_default();
        decoded.push(character);
        index += character.len_utf8();
    }
    PathBuf::from(decoded)
}

pub(super) fn preflight_application_purge(app_id: &str) -> Result<(), String> {
    let _ = application_tree_for_purge(app_id)?;
    Ok(())
}

pub(super) fn purge_application_tree(app_id: &str) -> Result<bool, String> {
    let Some(path) = application_tree_for_purge(app_id)? else {
        return Ok(false);
    };
    fs::remove_dir_all(&path).map_err(|error| format!("حذف پوشهٔ اپ ناموفق بود: {error}"))?;
    Ok(true)
}

pub(super) fn purge_application_uploads(job_ids: &[String]) -> Result<Vec<String>, String> {
    let root = deployment_root();
    let uploads = root.join("uploads");
    let metadata = match fs::symlink_metadata(&uploads) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("پوشهٔ ZIPهای استقرار بررسی نشد: {error}")),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("پوشهٔ ZIPهای استقرار واقعی نیست؛ پاک‌سازی متوقف شد.".into());
    }
    let mut removed = Vec::new();
    for job_id in job_ids {
        if !valid_key(job_id) || job_id.len() != 36 {
            return Err("شناسهٔ Job برای پاک‌کردن ZIP معتبر نیست.".into());
        }
        let path = uploads.join(format!("{job_id}.zip"));
        let file = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("فایل ZIP استقرار بررسی نشد: {error}")),
        };
        if !file.is_file() || file.file_type().is_symlink() {
            return Err("فایل ZIP استقرار معمولی نیست؛ حذف انجام نشد.".into());
        }
        fs::remove_file(&path).map_err(|error| format!("حذف ZIP استقرار ناموفق بود: {error}"))?;
        removed.push(job_id.clone());
    }
    Ok(removed)
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS deployment_jobs (
            id TEXT PRIMARY KEY, application_id TEXT NOT NULL REFERENCES applications(id),
            owner_server_uuid TEXT NOT NULL, actor TEXT NOT NULL, state TEXT NOT NULL,
            source_json TEXT NOT NULL, idempotency_key TEXT NOT NULL, healthcheck_file TEXT,
            attempt INTEGER NOT NULL DEFAULT 1, cancel_requested INTEGER NOT NULL DEFAULT 0,
            worker_id TEXT, lease_until INTEGER, heartbeat_at INTEGER,
            release_path TEXT, previous_release TEXT, source_commit TEXT, error_summary TEXT,
            created_at INTEGER NOT NULL, started_at INTEGER, finished_at INTEGER, updated_at INTEGER NOT NULL,
            UNIQUE(application_id,idempotency_key));
         CREATE INDEX IF NOT EXISTS deployment_jobs_queue ON deployment_jobs(state,created_at);
         CREATE INDEX IF NOT EXISTS deployment_jobs_app ON deployment_jobs(application_id,created_at DESC);
         CREATE TABLE IF NOT EXISTS deployment_job_steps (
            id INTEGER PRIMARY KEY, job_id TEXT NOT NULL REFERENCES deployment_jobs(id) ON DELETE CASCADE,
            sequence INTEGER NOT NULL, step TEXT NOT NULL, state TEXT NOT NULL, message TEXT NOT NULL,
            started_at INTEGER NOT NULL, finished_at INTEGER, UNIQUE(job_id,sequence));
         CREATE TABLE IF NOT EXISTS deployment_releases (
            id TEXT PRIMARY KEY, application_id TEXT NOT NULL REFERENCES applications(id),
            job_id TEXT NOT NULL UNIQUE REFERENCES deployment_jobs(id), path TEXT NOT NULL,
            state TEXT NOT NULL, source_commit TEXT, created_at INTEGER NOT NULL, activated_at INTEGER);
         ",
    )?;
    // CREATE TABLE IF NOT EXISTS does not update older deployment databases.
    // Add fields introduced after the original schema before any endpoint
    // reads/writes them, otherwise a queued deployment can surface only as a
    // generic "Database error" on upgraded installations.
    let columns = {
        let mut statement = db.prepare("PRAGMA table_info(deployment_jobs)")?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<std::collections::HashSet<_>, _>>()?;
        columns
    };
    for (name, definition) in [
        ("healthcheck_file", "TEXT"),
        ("attempt", "INTEGER NOT NULL DEFAULT 1"),
        ("cancel_requested", "INTEGER NOT NULL DEFAULT 0"),
        ("worker_id", "TEXT"),
        ("lease_until", "INTEGER"),
        ("heartbeat_at", "INTEGER"),
        ("release_path", "TEXT"),
        ("previous_release", "TEXT"),
        ("source_commit", "TEXT"),
        ("error_summary", "TEXT"),
        ("started_at", "INTEGER"),
        ("finished_at", "INTEGER"),
        ("updated_at", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !columns.contains(name) {
            db.execute(&format!("ALTER TABLE deployment_jobs ADD COLUMN {name} {definition}"), [])?;
        }
    }
    // Repair duplicate active rows left by older non-transactional enqueue
    // paths before restoring the one-active-job constraint. Keep the newest
    // row for each app; mark older orphan rows failed instead of blocking the
    // entire database migration.
    db.execute(
        "UPDATE deployment_jobs SET state='failed',error_summary='Job قدیمی به‌علت اجرای هم‌زمان تکراری در صف نگه داشته نشد.',finished_at=COALESCE(finished_at,?1),updated_at=?1 WHERE state IN ('queued','validating','running','verifying') AND id<>(SELECT newest.id FROM deployment_jobs newest WHERE newest.application_id=deployment_jobs.application_id AND newest.state IN ('queued','validating','running','verifying') ORDER BY newest.created_at DESC,newest.id DESC LIMIT 1)",
        [now()],
    )?;
    db.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS deployment_one_active_job_per_app ON deployment_jobs(application_id) WHERE state IN ('queued','validating','running','verifying');")?;
    // Older installs may already have created the step/release tables with a
    // partial schema. Bring those forward additively before any job can be
    // inserted, so a failed step write cannot strand a queued job.
    ensure_table_columns(db, "deployment_job_steps", &[
        ("job_id", "TEXT NOT NULL DEFAULT ''"),
        ("sequence", "INTEGER NOT NULL DEFAULT 0"),
        ("step", "TEXT NOT NULL DEFAULT 'unknown'"),
        ("state", "TEXT NOT NULL DEFAULT 'pending'"),
        ("message", "TEXT NOT NULL DEFAULT ''"),
        ("started_at", "INTEGER NOT NULL DEFAULT 0"),
        ("finished_at", "INTEGER"),
    ])?;
    ensure_table_columns(db, "deployment_releases", &[
        ("application_id", "TEXT NOT NULL DEFAULT ''"),
        ("job_id", "TEXT NOT NULL DEFAULT ''"),
        ("path", "TEXT NOT NULL DEFAULT ''"),
        ("state", "TEXT NOT NULL DEFAULT 'retained'"),
        ("source_commit", "TEXT"),
        ("created_at", "INTEGER NOT NULL DEFAULT 0"),
        ("activated_at", "INTEGER"),
    ])?;
    db.execute_batch("CREATE INDEX IF NOT EXISTS deployment_releases_app ON deployment_releases(application_id,created_at DESC);")?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS deployment_job_logs (
            id INTEGER PRIMARY KEY,
            job_id TEXT NOT NULL REFERENCES deployment_jobs(id) ON DELETE CASCADE,
            created_at INTEGER NOT NULL,
            step TEXT NOT NULL,
            level TEXT NOT NULL DEFAULT 'info',
            message TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS deployment_job_logs_cursor ON deployment_job_logs(job_id,id);",
    )?;
    db.execute(
        "INSERT INTO deployment_job_steps(job_id,sequence,step,state,message,started_at,finished_at) SELECT jobs.id,1,'recovered','complete','Job باقی‌مانده از نسخهٔ قبلی بازیابی شد؛ اجرای آن ادامه پیدا می‌کند.',jobs.created_at,jobs.created_at FROM deployment_jobs jobs WHERE jobs.state='queued' AND NOT EXISTS (SELECT 1 FROM deployment_job_steps steps WHERE steps.job_id=jobs.id)",
        [],
    )?;
    db.execute(
        "INSERT INTO deployment_job_logs(job_id,created_at,step,level,message) SELECT jobs.id,jobs.created_at,'recovered','warning','Job قدیمی بدون مرحلهٔ ثبت‌شده بازیابی شد.' FROM deployment_jobs jobs WHERE jobs.state='queued' AND NOT EXISTS (SELECT 1 FROM deployment_job_logs logs WHERE logs.job_id=jobs.id)",
        [],
    )?;
    Ok(())
}

fn ensure_table_columns(db: &Connection, table: &str, expected: &[(&str, &str)]) -> rusqlite::Result<()> {
    let mut statement = db.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<std::collections::HashSet<_>, _>>()?;
    drop(statement);
    for (name, definition) in expected {
        if !columns.contains(*name) {
            db.execute(&format!("ALTER TABLE {table} ADD COLUMN {name} {definition}"), [])?;
        }
    }
    Ok(())
}

fn id() -> String {
    let mut bytes = [0_u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let value = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &value[..8],
        &value[8..12],
        &value[12..16],
        &value[16..20],
        &value[20..]
    )
}

fn valid_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
}

fn valid_reference(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && !value.starts_with('-')
        && !value.contains("..")
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._/-".contains(&c))
}

fn valid_repo(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    let parts = url.path().trim_matches('/').split('/').collect::<Vec<_>>();
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && matches!(
            url.host_str(),
            Some("github.com" | "gitlab.com" | "bitbucket.org" | "codeberg.org")
        )
        && parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && *part != "."
                && *part != ".."
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
}

fn valid_relative_file(value: &str) -> bool {
    if value.is_empty() || value.len() > 512 || value.contains(['\\', '\0']) {
        return false;
    }
    let path = FsPath::new(value);
    path.components().all(|c| matches!(c, Component::Normal(_)))
}

fn api_error(status: StatusCode, message: &str) -> (StatusCode, Json<Value>) {
    error(status, message)
}

fn ensure_app(db: &Connection, app_id: &str, owner: &str) -> Result<(), (StatusCode, Json<Value>)> {
    let active: Option<Option<i64>> = db
        .query_row(
            "SELECT deleted_at FROM applications WHERE id=?1 AND owner_server_uuid=?2",
            params![app_id, owner],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?;
    match active {
        Some(None) => Ok(()),
        Some(Some(_)) => Err(api_error(
            StatusCode::CONFLICT,
            "اپلیکیشن آرشیو شده است؛ پیش از استقرار آن را بازیابی کن.",
        )),
        None => Err(api_error(
            StatusCode::NOT_FOUND,
            "اپلیکیشن در این VPS پیدا نشد.",
        )),
    }
}

fn insert_job(
    state: &AppState,
    headers: &HeaderMap,
    app_id: &str,
    source: Source,
    key: String,
    healthcheck: Option<String>,
    archive: Option<&[u8]>,
    archive_file: Option<&FsPath>,
) -> Result<Value, (StatusCode, Json<Value>)> {
    check_origin(headers)?;
    if !valid_key(&key) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Idempotency key نامعتبر است.",
        ));
    }
    if healthcheck
        .as_deref()
        .is_some_and(|f| !valid_relative_file(f))
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "مسیر Health check باید یک مسیر نسبی امن باشد.",
        ));
    }
    if let Some(bytes) = archive {
        if bytes.is_empty() || bytes.len() > MAX_ARCHIVE_BYTES {
            return Err(api_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "ZIP باید غیرخالی و حداکثر ۲۵۶ MiB باشد.",
            ));
        }
    }
    if let Source::Git { repository, reference, ssh_key_id } = &source {
        let public_https = valid_repo(repository) && ssh_key_id.is_none();
        let private_ssh = ssh_keys::is_github_ssh_repository(repository) && ssh_key_id.as_deref().is_some_and(ssh_keys::valid_key_id);
        if !(public_https || private_ssh) || !valid_reference(reference) {
            return Err(api_error(StatusCode::BAD_REQUEST, "Git عمومی باید HTTPS معتبر باشد؛ مخزن خصوصی GitHub باید SSH و همراه کلید SSH انتخاب‌شده باشد."));
        }
    }
    if let Source::Directory { path } = &source {
        if path.len() > 1024 || path.contains('\0') {
            return Err(api_error(StatusCode::BAD_REQUEST, "مسیر پوشه معتبر نیست."));
        }
    }
    let (actor, owner) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        ensure_app(&db, app_id, &owner)?;
        if let Source::Git { repository, ssh_key_id: Some(key_id), .. } = &source {
            ssh_keys::validate_key_for_deployment(&db, app_id, repository, key_id)
                .map_err(|message| api_error(StatusCode::BAD_REQUEST, &message))?;
        }
        if let Some(existing) = db.query_row(
            "SELECT id,source_json,state,healthcheck_file FROM deployment_jobs WHERE application_id=?1 AND idempotency_key=?2",
            params![app_id, key], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?)),
        ).optional().map_err(db_error)? {
            if existing.1 != serde_json::to_string(&source).unwrap_or_default() || existing.3 != healthcheck {
                return Err(api_error(StatusCode::CONFLICT, "این Idempotency key قبلاً برای منبع دیگری مصرف شده است."));
            }
            if let Some(path)=archive_file { let _=fs::remove_file(path); }
            return read_job(&db, &existing.0).map_err(db_error);
        }
        let active: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM deployment_jobs WHERE application_id=?1 AND state IN ('queued','validating','running','verifying'))",
            [app_id], |row| row.get(0),
        ).map_err(db_error)?;
        if active {
            return Err(api_error(
                StatusCode::CONFLICT,
                "برای همین اپلیکیشن یک Job فعال است؛ ابتدا آن را بررسی یا لغو کن.",
            ));
        }
        let laravel_table_exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='laravel_operations')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if laravel_table_exists {
            let laravel_active: bool = db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM laravel_operations WHERE application_id=?1 AND state IN ('queued','running'))",
                    [app_id],
                    |row| row.get(0),
                )
                .unwrap_or(true);
            if laravel_active {
                return Err(api_error(
                    StatusCode::CONFLICT,
                    "برای این اپلیکیشن یک عملیات Laravel فعال است؛ پس از پایان آن دوباره deploy کن.",
                ));
            }
        }
        (actor, owner)
    };
    let job_id = id();
    let at = now();
    let source_json = serde_json::to_string(&source).map_err(|_| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "منبع Job قابل ذخیره نیست.",
        )
    })?;
    let archive_path = if archive.is_some() || archive_file.is_some() {
        let dir = deployment_root().join("uploads");
        fs::create_dir_all(&dir).map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "پوشهٔ دریافت آرشیو آماده نشد.",
            )
        })?;
        let path = dir.join(format!("{job_id}.zip"));
        if let Some(source_path) = archive_file {
            fs::rename(source_path, &path).map_err(|_| {
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "فایل موقت ZIP به Job منتقل نشد.",
                )
            })?;
        } else if let Some(archive_bytes) = archive {
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                let mut f = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&path)
                    .map_err(|_| {
                        api_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "فایل آرشیو موقت ساخته نشد.",
                        )
                    })?;
                f.write_all(archive_bytes).map_err(|_| {
                    api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "آرشیو به‌طور کامل ذخیره نشد.",
                    )
                })?;
                f.sync_all().ok();
            }
            #[cfg(not(unix))]
            fs::write(&path, archive_bytes)
                .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "آرشیو ذخیره نشد."))?;
        }
        Some(path.to_string_lossy().to_string())
    } else {
        None
    };
    let mut db = state.lock().unwrap();
    let response = match persist_new_job(
        &mut db,&job_id,app_id,&owner,&actor,&source_json,&key,healthcheck.as_deref(),at,source_kind(&source),
    ) {
        Ok(value) => value,
        Err(error_value) => {
        if let Some(path) = archive_path.as_deref() { let _ = fs::remove_file(path); }
        return Err(db_error(error_value));
        }
    };
    Ok(response)
}

fn persist_new_job(
    db: &mut Connection,
    job_id: &str,
    app_id: &str,
    owner: &str,
    actor: &str,
    source_json: &str,
    key: &str,
    healthcheck: Option<&str>,
    at: i64,
    source: &str,
) -> rusqlite::Result<Value> {
    let transaction = db.transaction()?;
    transaction.execute(
        "INSERT INTO deployment_jobs(id,application_id,owner_server_uuid,actor,state,source_json,idempotency_key,healthcheck_file,created_at,updated_at) VALUES (?1,?2,?3,?4,'queued',?5,?6,?7,?8,?8)",
        params![job_id,app_id,owner,actor,source_json,key,healthcheck,at],
    )?;
    add_step(&transaction,job_id,"queued","complete","در صف اجرای محدود سرور قرار گرفت.")?;
    audit(&transaction,actor,"deployment.enqueue",&format!("job={job_id}; app={app_id}; source={source}"));
    let response = read_job(&transaction,job_id)?;
    transaction.commit()?;
    Ok(response)
}

fn source_kind(source: &Source) -> &'static str {
    match source {
        Source::Git { .. } => "git",
        Source::Directory { .. } => "directory",
        Source::Archive { .. } => "archive",
    }
}

fn excluded_source_path(path: &FsPath) -> bool {
    path.components().any(|component| {
        let Component::Normal(name) = component else {
            return false;
        };
        name.to_str().is_some_and(|name| {
            name == ".git"
                || name == "node_modules"
                || name == "__MACOSX"
                || name == ".DS_Store"
                || name.starts_with("._")
                || name == ".env"
                || name.starts_with(".env.")
        })
    })
}

fn add_step(
    db: &Connection,
    job_id: &str,
    step: &str,
    state: &str,
    message: &str,
) -> rusqlite::Result<()> {
    let sequence: i64 = db.query_row(
        "SELECT COALESCE(MAX(sequence),0)+1 FROM deployment_job_steps WHERE job_id=?1",
        [job_id],
        |row| row.get(0),
    )?;
    db.execute("INSERT INTO deployment_job_steps(job_id,sequence,step,state,message,started_at,finished_at) VALUES (?1,?2,?3,?4,?5,?6,?6)",params![job_id,sequence,step,state,message,now()])?;
    append_job_log(db, job_id, step, if state == "failed" { "error" } else { "info" }, message)?;
    Ok(())
}

fn append_job_log(db: &Connection, job_id: &str, step: &str, level: &str, message: &str) -> rusqlite::Result<()> {
    let bounded = message.chars().take(8_000).collect::<String>();
    db.execute(
        "INSERT INTO deployment_job_logs(job_id,created_at,step,level,message) VALUES (?1,?2,?3,?4,?5)",
        params![job_id, now(), step.chars().take(64).collect::<String>(), level, bounded],
    )?;
    prune_job_logs(db, job_id)
}

fn prune_job_logs(db: &Connection, job_id: &str) -> rusqlite::Result<()> {
    // Retain the newest 2,000 events per job; the cursor endpoint can page
    // older retained records without loading the entire deployment history.
    db.execute(
        "DELETE FROM deployment_job_logs WHERE job_id=?1 AND id NOT IN (SELECT id FROM deployment_job_logs WHERE job_id=?1 ORDER BY id DESC LIMIT 2000)",
        [job_id],
    )?;
    Ok(())
}

fn read_job(db: &Connection, job_id: &str) -> rusqlite::Result<Value> {
    let row: (String,String,String,String,String,i64,i64,Option<i64>,Option<String>,Option<String>,Option<String>,Option<String>,Option<String>) = db.query_row(
        "SELECT id,application_id,owner_server_uuid,actor,state,attempt,created_at,started_at,finished_at,release_path,previous_release,source_commit,error_summary FROM deployment_jobs WHERE id=?1",
        [job_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?,r.get(11)?,r.get(12)?)),
    )?;
    let mut statement = db.prepare("SELECT sequence,step,state,message,started_at,finished_at FROM deployment_job_steps WHERE job_id=?1 ORDER BY sequence")?;
    let steps = statement.query_map([job_id], |r| Ok(json!({"sequence":r.get::<_,i64>(0)?,"step":r.get::<_,String>(1)?,"state":r.get::<_,String>(2)?,"message":r.get::<_,String>(3)?,"started_at":r.get::<_,i64>(4)?,"finished_at":r.get::<_,Option<i64>>(5)?})))?.collect::<Result<Vec<_>,_>>()?;
    let mut log_statement = db.prepare("SELECT id,created_at,step,level,message FROM deployment_job_logs WHERE job_id=?1 ORDER BY id DESC LIMIT 20")?;
    let mut logs = log_statement.query_map([job_id], |r| Ok(json!({"id":r.get::<_,i64>(0)?,"created_at":r.get::<_,i64>(1)?,"step":r.get::<_,String>(2)?,"level":r.get::<_,String>(3)?,"message":r.get::<_,String>(4)?})))?.collect::<Result<Vec<_>,_>>()?;
    logs.reverse();
    let log_count: i64 = db.query_row("SELECT COUNT(*) FROM deployment_job_logs WHERE job_id=?1", [job_id], |r| r.get(0))?;
    Ok(
        json!({"id":row.0,"application_id":row.1,"owner_server_uuid":row.2,"actor":row.3,"state":row.4,"attempt":row.5,"created_at":row.6,"started_at":row.7,"finished_at":row.8,"release_path":row.9,"previous_release":row.10,"source_commit":row.11,"error":row.12,"steps":steps,"logs":logs,"log_count":log_count}),
    )
}

pub(super) async fn logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(job_id): Path<String>,
    Query(query): Query<JobLogsQuery>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let job_owner: Option<String> = db.query_row("SELECT owner_server_uuid FROM deployment_jobs WHERE id=?1", [&job_id], |row| row.get(0)).optional().map_err(db_error)?;
    if job_owner.as_deref() != Some(owner.as_str()) {
        return Err(api_error(StatusCode::NOT_FOUND,"Job پیدا نشد."));
    }
    let after = query.after.unwrap_or(0).max(0);
    let limit = query.limit.unwrap_or(100).clamp(1,500);
    let (sql, cursor) = if let Some(before) = query.before {
        ("SELECT id,created_at,step,level,message FROM deployment_job_logs WHERE job_id=?1 AND id<?2 ORDER BY id DESC LIMIT ?3", before.max(0))
    } else {
        ("SELECT id,created_at,step,level,message FROM deployment_job_logs WHERE job_id=?1 AND id>?2 ORDER BY id LIMIT ?3", after)
    };
    let mut statement = db.prepare(sql).map_err(db_error)?;
    let mut values = statement.query_map(params![job_id,cursor,limit], |row| Ok(json!({"id":row.get::<_,i64>(0)?,"created_at":row.get::<_,i64>(1)?,"step":row.get::<_,String>(2)?,"level":row.get::<_,String>(3)?,"message":row.get::<_,String>(4)?}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
    if query.before.is_some() { values.reverse(); }
    let next_after = values.last().and_then(|value| value["id"].as_i64()).unwrap_or(after);
    let next_before = values.first().and_then(|value| value["id"].as_i64());
    Ok(Json(json!({"logs":values,"next_after":next_after,"next_before":next_before,"retention_per_job":2000})))
}

pub(super) fn record_output(state: &AppState, job_id: &str, step: &str, output: &str) {
    let mut db = state.lock().unwrap();
    let Ok(transaction) = db.transaction() else { return };
    for line in output.lines().take(600) {
        let line = line.trim_end();
        if !line.is_empty() {
            let bounded = line.chars().take(8_000).collect::<String>();
            if transaction.execute(
                "INSERT INTO deployment_job_logs(job_id,created_at,step,level,message) VALUES (?1,?2,?3,'output',?4)",
                params![job_id,now(),step.chars().take(64).collect::<String>(),bounded],
            ).is_err() {
                return;
            }
        }
    }
    if prune_job_logs(&transaction, job_id).is_ok() {
        let _ = transaction.commit();
    }
}

fn list_jobs(db: &Connection, app_id: &str, limit: i64) -> rusqlite::Result<Vec<Value>> {
    let mut statement = db.prepare(
        "SELECT id FROM deployment_jobs WHERE application_id=?1 ORDER BY created_at DESC LIMIT ?2",
    )?;
    let ids = statement
        .query_map(params![app_id, limit], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter().map(|job| read_job(db, job)).collect()
}

pub(super) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<DeployRequest>,
) -> ApiResult {
    let _app_execution_guard = laravel::APP_EXECUTION_LOCK.lock().await;
    Ok(Json(insert_job(
        &state,
        &headers,
        &app_id,
        input.source,
        input.idempotency_key,
        input.healthcheck_file,
        None,
        None,
    )?))
}

pub(super) async fn upload_archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    body: axum::body::Body,
) -> ApiResult {
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_none_or(|v| !v.starts_with("application/zip"))
    {
        return Err(api_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "فایل آرشیو باید با Content-Type: application/zip ارسال شود.",
        ));
    }
    check_origin(&headers)?;
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    if !valid_key(&key) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Idempotency key نامعتبر است.",
        ));
    }
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        ensure_app(&db, &app_id, &owner)?;
    }
    let dir = deployment_root().join("uploads");
    fs::create_dir_all(&dir).map_err(|_| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "پوشهٔ دریافت ZIP آماده نشد.",
        )
    })?;
    let temp = dir.join(format!("upload-{}.tmp", id()));
    #[cfg(unix)]
    let mut file = {
        tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .await
            .map_err(|_| {
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "فایل موقت ZIP ساخته نشد.",
                )
            })?
    };
    #[cfg(not(unix))]
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .await
        .map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "فایل موقت ZIP ساخته نشد.",
            )
        })?;
    let mut hasher = Sha256::new();
    let mut total = 0usize;
    let mut body_stream = body.into_data_stream();
    let upload_started = Instant::now();
    loop {
        if upload_started.elapsed() > Duration::from_secs(15 * 60) {
            drop(file);
            let _ = fs::remove_file(&temp);
            return Err(api_error(
                StatusCode::REQUEST_TIMEOUT,
                "بارگذاری ZIP از مهلت ۱۵ دقیقه عبور کرد.",
            ));
        }
        let chunk = match tokio::time::timeout(Duration::from_secs(30), body_stream.next()).await {
            Ok(Some(Ok(data))) => data,
            Ok(None) => break,
            Ok(Some(Err(_))) => {
                drop(file);
                let _ = fs::remove_file(&temp);
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    "جریان بارگذاری ZIP ناقص شد.",
                ));
            }
            Err(_) => {
                drop(file);
                let _ = fs::remove_file(&temp);
                return Err(api_error(
                    StatusCode::REQUEST_TIMEOUT,
                    "برای بارگذاری ZIP داده‌ای نرسید؛ دوباره تلاش کن.",
                ));
            }
        };
        total = total.saturating_add(chunk.len());
        if total > MAX_ARCHIVE_BYTES {
            drop(file);
            let _ = fs::remove_file(&temp);
            return Err(api_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "ZIP باید حداکثر ۲۵۶ MiB باشد.",
            ));
        }
        hasher.update(&chunk);
        if file.write_all(&chunk).await.is_err() {
            drop(file);
            let _ = fs::remove_file(&temp);
            return Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "نوشتن ZIP موقت ناموفق بود.",
            ));
        }
    }
    if total == 0 {
        drop(file);
        let _ = fs::remove_file(&temp);
        return Err(api_error(StatusCode::BAD_REQUEST, "فایل ZIP خالی است."));
    }
    if file.sync_all().await.is_err() {
        drop(file);
        let _ = fs::remove_file(&temp);
        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ثبت کامل ZIP روی دیسک ناموفق بود.",
        ));
    }
    drop(file);
    let source = Source::Archive {
        sha256: hex::encode(hasher.finalize()),
    };
    let _app_execution_guard = laravel::APP_EXECUTION_LOCK.lock().await;
    match insert_job(
        &state,
        &headers,
        &app_id,
        source,
        key,
        None,
        None,
        Some(&temp),
    ) {
        Ok(job) => Ok(Json(job)),
        Err(problem) => {
            let _ = fs::remove_file(&temp);
            Err(problem)
        }
    }
}

pub(super) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    ensure_app(&db, &app_id, &owner)?;
    Ok(Json(
        json!({"jobs":list_jobs(&db,&app_id,50).map_err(db_error)?}),
    ))
}

pub(super) async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(job_id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let job = read_job(&db, &job_id).map_err(db_error)?;
    if job["owner_server_uuid"] != owner {
        return Err(api_error(StatusCode::NOT_FOUND, "Job پیدا نشد."));
    }
    Ok(Json(job))
}

pub(super) async fn cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(job_id): Path<String>,
) -> ApiResult {
    check_origin(&headers)?;
    let _app_execution_guard = laravel::APP_EXECUTION_LOCK.lock().await;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let old: Option<String> = db
        .query_row(
            "SELECT state FROM deployment_jobs WHERE id=?1 AND owner_server_uuid=?2",
            params![job_id, owner],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_error)?;
    let Some(old) = old else {
        return Err(api_error(StatusCode::NOT_FOUND, "Job پیدا نشد."));
    };
    if !JOB_ACTIVE.contains(&old.as_str()) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "این Job فعال نیست و لغو نمی‌شود.",
        ));
    }
    let at = now();
    if old == "queued" {
        db.execute("UPDATE deployment_jobs SET cancel_requested=1,state='cancelled',finished_at=?1,updated_at=?1 WHERE id=?2",params![at,job_id]).map_err(db_error)?;
        db.execute("UPDATE deployment_job_steps SET state='cancelled',finished_at=?1,message='پیش از شروع از سوی مدیر لغو شد.' WHERE job_id=?2 AND state='running'",params![at,job_id]).map_err(db_error)?;
        add_step(
            &db,
            &job_id,
            "cancelled",
            "cancelled",
            "پیش از شروع از سوی مدیر لغو شد.",
        )
        .map_err(db_error)?;
    } else {
        db.execute(
            "UPDATE deployment_jobs SET cancel_requested=1,updated_at=?1 WHERE id=?2",
            params![at, job_id],
        )
        .map_err(db_error)?;
    }
    audit(&db, &actor, "deployment.cancel", &format!("job={job_id}"));
    Ok(Json(
        json!({"cancel_requested":true,"job":read_job(&db,&job_id).map_err(db_error)?}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RollbackRequest {
    release_id: String,
}

pub(super) async fn rollback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<RollbackRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if input.release_id.len() != 36
        || !input
            .release_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "شناسهٔ release معتبر نیست.",
        ));
    }
    let _app_execution_guard = laravel::APP_EXECUTION_LOCK.lock().await;
    let (actor, runtime, static_config, row) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        ensure_app(&db, &app_id, &owner)?;
        let active: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM deployment_jobs WHERE application_id=?1 AND state IN ('queued','validating','running','verifying'))",
                [&app_id],
                |query| query.get(0),
            )
            .map_err(db_error)?;
        if active {
            return Err(api_error(
                StatusCode::CONFLICT,
                "تا پایان یا لغو Job فعال، Rollback هم‌زمان انجام نمی‌شود.",
            ));
        }
        let row: Option<(String, String)> = db
            .query_row(
                "SELECT path,state FROM deployment_releases WHERE id=?1 AND application_id=?2",
                params![input.release_id, app_id],
                |query| Ok((query.get(0)?, query.get(1)?)),
            )
            .optional()
            .map_err(db_error)?;
        let runtime = db
            .query_row(
                "SELECT a.framework,a.runtime_version,s.domain,s.enabled FROM applications a LEFT JOIN laravel_app_sites s ON s.application_id=a.id WHERE a.id=?1",
                [&app_id],
                |query| {
                    Ok((
                        query.get::<_, String>(0)?,
                        query.get::<_, String>(1)?,
                        query.get::<_, Option<String>>(2)?,
                        query.get::<_, Option<bool>>(3)?.unwrap_or(false),
                    ))
                },
            )
            .optional()
            .map_err(db_error)?;
        let static_config = db.query_row(
            "SELECT domain,static_output_path,static_spa_fallback FROM applications WHERE id=?1",
            [&app_id],
            |query| Ok((query.get::<_,Option<String>>(0)?,query.get::<_,String>(1)?,query.get::<_,bool>(2)?)),
        ).optional().map_err(db_error)?;
        (actor, runtime, static_config, row)
    };
    let Some((release_path, release_state)) = row else {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "release برای این اپلیکیشن پیدا نشد.",
        ));
    };
    if release_state != "previous" {
        return Err(api_error(
            StatusCode::CONFLICT,
            "فقط release قبلی قابل Rollback است.",
        ));
    }
    let root = deployment_root().join("apps").join(&app_id);
    let releases = root
        .join("releases")
        .canonicalize()
        .map_err(|_| api_error(StatusCode::CONFLICT, "پوشهٔ release قابل دسترسی نیست."))?;
    let selected = FsPath::new(&release_path)
        .canonicalize()
        .map_err(|_| api_error(StatusCode::CONFLICT, "فایل‌های release قبلی موجود نیستند."))?;
    if !selected.starts_with(&releases) || !selected.is_dir() {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "مسیر release خارج از پوشهٔ مجاز است.",
        ));
    }
    let current = root.join("current");
    if current.exists() && !fs::symlink_metadata(&current).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "current symlink نیست؛ Rollback برای جلوگیری از جایگزینی فایل واقعی متوقف شد.",
        ));
    }
    let old_link = fs::read_link(&current).ok();
    let old_pointer = old_link
        .as_ref()
        .map(|path| path.to_string_lossy().to_string());
    let old_id = old_link
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_string());
    let temp = root.join(format!(".rollback-{}", id()));
    std::os::unix::fs::symlink(FsPath::new("releases").join(&input.release_id), &temp)
        .and_then(|_| fs::rename(&temp, &current))
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("تغییر اتمی release ناموفق بود: {error}"),
            )
        })?;

    let framework = runtime.as_ref().map(|runtime| runtime.0.as_str()).unwrap_or_default();
    let mut nginx_snapshot: Option<(String, String)> = None;
    let static_refresh = if apps::is_static_framework(framework) || apps::is_auto_framework(framework) {
        match static_config.as_ref() {
            Some((Some(domain), output, spa)) => refresh_static_runtime(&app_id, framework, domain, output, *spa).await,
            Some((None, _, _)) if apps::is_static_framework(framework) => Err("دامنهٔ سایت Static برای انتشار ثبت نشده است.".to_owned()),
            _ => Ok(None),
        }
    } else { Ok(None) };
    let refresh_result = match static_refresh {
        Ok(Some(snapshot)) => {
            nginx_snapshot = Some(snapshot);
            Ok(())
        }
        Err(message) => Err(message),
        Ok(None) if framework == "laravel" => {
            let (_, runtime_version, domain, web_enabled) = runtime.as_ref().unwrap();
            match refresh_laravel_runtime(&app_id, runtime_version, domain.as_deref(), *web_enabled).await {
                Ok(snapshot) => { nginx_snapshot = snapshot; Ok(()) }
                Err(message) => Err(message),
            }
        }
        Ok(None) if apps::is_node_runtime_framework(framework) || apps::is_auto_framework(framework) => {
            node::refresh_after_deploy(&app_id, old_pointer.as_deref()).await.map(|_| ())
        }
        Ok(None) => Ok(()),
    };

    if let Err(message) = refresh_result {
        let restored = restore_current_pointer(&root, old_pointer.as_deref(), &id());
        let recovery = if restored.is_ok() {
            if framework == "laravel" {
                let (_, runtime_version, domain, web_enabled) = runtime.as_ref().unwrap();
                match refresh_laravel_runtime(
                    &app_id,
                    runtime_version,
                    domain.as_deref(),
                    *web_enabled,
                )
                .await
                {
                    Ok(snapshot) => {
                        if let Some((name, content)) = snapshot {
                            let db = state.lock().unwrap();
                            let _ = nginx::store_nginx_snapshot(&db, &name, &content, None);
                        }
                        "وب و سرویس‌های Laravel قبلی دوباره آماده شدند."
                    }
                    Err(error) => {
                        return Err(api_error(
                            StatusCode::BAD_GATEWAY,
                            &format!("{message} اشاره‌گر قبلی برگشت، اما بازیابی سرویس Laravel هم شکست خورد: {error}"),
                        ));
                    }
                }
            } else if apps::is_static_framework(framework) || apps::is_auto_framework(framework) {
                let static_recovery = if let Some((Some(domain), output, spa)) = static_config.as_ref() {
                    refresh_static_runtime(&app_id, framework, domain, output, *spa).await
                } else { Ok(None) };
                match static_recovery {
                    Ok(Some((name, content))) => {
                        let db = state.lock().unwrap();
                        let _ = nginx::store_nginx_snapshot(&db, &name, &content, None);
                        "کانفیگ Nginx سایت Static با release قبلی همگام شد."
                    }
                    Ok(None) if apps::is_auto_framework(framework) => match node::refresh_after_deploy(&app_id, old_pointer.as_deref()).await {
                        Ok(_) => "سرویس Node با release قبلی دوباره بررسی شد.",
                        Err(error) => return Err(api_error(StatusCode::BAD_GATEWAY, &format!("{message} اشاره‌گر قبلی برگشت، اما سرویس Node بازیابی نشد: {error}"))),
                    },
                    Ok(None) => "اشاره‌گر release قبلی بازگردانده شد.",
                    Err(error) => return Err(api_error(StatusCode::BAD_GATEWAY, &format!("{message} اشاره‌گر قبلی برگشت، اما بازیابی سایت Static ناموفق بود: {error}"))),
                }
            } else if apps::is_node_runtime_framework(framework) {
                match node::refresh_after_deploy(&app_id, old_pointer.as_deref()).await {
                    Ok(_) => "سرویس Node با release قبلی دوباره بررسی شد.",
                    Err(error) => {
                        return Err(api_error(
                            StatusCode::BAD_GATEWAY,
                            &format!("{message} اشاره‌گر قبلی برگشت، اما بازیابی سرویس Node هم شکست خورد: {error}"),
                        ));
                    }
                }
            } else {
                "اشاره‌گر release قبلی بازگردانده شد."
            }
        } else {
            return Err(api_error(
                StatusCode::BAD_GATEWAY,
                &format!("{message} بازگرداندن اشاره‌گر قبلی هم شکست خورد: {restored:?}"),
            ));
        };
        return Err(api_error(
            StatusCode::BAD_GATEWAY,
            &format!("{message} {recovery}"),
        ));
    }

    let db = state.lock().unwrap();
    let at = now();
    db.execute(
        "UPDATE deployment_releases SET state=CASE WHEN id=?1 THEN 'active' WHEN id=?2 THEN 'previous' ELSE state END,activated_at=CASE WHEN id=?1 THEN ?3 ELSE activated_at END WHERE application_id=?4 AND state IN ('active','previous')",
        params![input.release_id, old_id, at, app_id],
    )
    .map_err(db_error)?;
    let snapshot_warning = nginx_snapshot.and_then(|(name, content)| {
        nginx::store_nginx_snapshot(&db, &name, &content, None)
            .err()
            .map(|problem| problem.to_string())
    });
    audit(
        &db,
        &actor,
        "deployment.rollback",
        &format!("app={app_id}; release={}", input.release_id),
    );
    let message = snapshot_warning.map_or_else(
        || format!("release قبلی فعال شد؛ {} بررسی شد. دیتابیس rollback نشد.", if apps::is_node_runtime_framework(framework) { "سرویس Node" } else if apps::is_static_framework(framework) { "سایت Static" } else if apps::is_auto_framework(framework) { "سایت یا Runner تشخیص‌داده‌شده" } else if framework == "laravel" { "سرویس Laravel" } else { "اپلیکیشن" }),
        |warning| format!("release قبلی فعال شد، اما snapshot کانفیگ Nginx همگام نشد: {warning}"),
    );
    Ok(Json(json!({"rolled_back":true,"release_id":input.release_id,"previous_release":old_id,"message":message})))
}

#[cfg(test)]
#[test]
fn rollback_route_future_is_send() {
    fn assert_send<F: std::future::Future + Send>(_: F) {}
    let state = State(Arc::new(Mutex::new(Connection::open_in_memory().unwrap())));
    assert_send(rollback(
        state,
        HeaderMap::new(),
        Path(String::new()),
        Json(RollbackRequest {
            release_id: String::new(),
        }),
    ));
}

pub(super) async fn retry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(job_id): Path<String>,
    Json(input): Json<RetryRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_key(&input.idempotency_key) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Idempotency key نامعتبر است.",
        ));
    }
    let (actor, old, source) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let old: (String, String, String, Option<String>, String, Option<String>) = db
            .query_row(
                "SELECT application_id,owner_server_uuid,source_json,healthcheck_file,state,source_commit FROM deployment_jobs WHERE id=?1",
                [job_id.as_str()],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)),
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Job پیدا نشد."))?;
        if old.1 != owner {
            return Err(api_error(StatusCode::NOT_FOUND, "Job پیدا نشد."));
        }
        if JOB_ACTIVE.contains(&old.4.as_str()) {
            return Err(api_error(
                StatusCode::CONFLICT,
                "Job فعال را نمی‌توان دوباره اجرا کرد.",
            ));
        }
        let mut source: Source = serde_json::from_str(&old.2).map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "تنظیمات منبع Job قابل بازیابی نیست.",
            )
        })?;
        if let (Source::Git { repository, ssh_key_id, .. }, Some(commit)) = (&source, old.5.as_ref()) {
            source = Source::Git {
                repository: repository.clone(),
                reference: commit.clone(),
                ssh_key_id: ssh_key_id.clone(),
            };
        }
        (actor, old, source)
    };
    let archive_file = if matches!(&source, Source::Archive { .. }) {
        let path = deployment_root()
            .join("uploads")
            .join(format!("{job_id}.zip"));
        let temp = deployment_root()
            .join("uploads")
            .join(format!("retry-{}.tmp", id()));
        fs::copy(path, &temp).map_err(|_| {
            api_error(
                StatusCode::CONFLICT,
                "ZIP موقت موجود نیست؛ فایل را دوباره بارگذاری کن.",
            )
        })?;
        Some(temp)
    } else {
        None
    };
    let _app_execution_guard = laravel::APP_EXECUTION_LOCK.lock().await;
    let result = insert_job(
        &state,
        &headers,
        &old.0,
        source,
        input.idempotency_key,
        old.3,
        None,
        archive_file.as_deref(),
    );
    let mut value = match result {
        Ok(value) => value,
        Err(problem) => {
            if let Some(path) = archive_file {
                let _ = fs::remove_file(path);
            }
            return Err(problem);
        }
    };
    {
        let db = state.lock().unwrap();
        let new_id = value["id"].as_str().unwrap_or_default().to_owned();
        db.execute(
            "UPDATE deployment_jobs SET attempt=attempt+1 WHERE id=?1",
            [&new_id],
        )
        .map_err(db_error)?;
        audit(
            &db,
            &actor,
            "deployment.retry",
            &format!("from={job_id}; to={new_id}"),
        );
        value = read_job(&db, &new_id).map_err(db_error)?;
    }
    Ok(Json(value))
}

pub(super) async fn events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(job_id): Path<String>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<Event, std::convert::Infallible>>>,
    (StatusCode, Json<Value>),
> {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let job = read_job(&db, &job_id).map_err(db_error)?;
    if job["owner_server_uuid"] != owner {
        return Err(api_error(StatusCode::NOT_FOUND, "Job پیدا نشد."));
    }
    let state = state.clone();
    let job_id = job_id.clone();
    let events = stream::unfold(
        (state, job_id, headers, 0_u64, false),
        |(state, job_id, headers, revision, done)| async move {
            if done {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(900)).await;
            let result = {
                let db = state.lock().unwrap();
                if require_admin(&db, &headers).is_err() {
                    None
                } else {
                    read_job(&db, &job_id).ok()
                }
            };
            let Some(job) = result else {
                return None;
            };
            let next_revision = job["steps"]
                .as_array()
                .map_or(revision, |steps| steps.len() as u64);
            let terminal = matches!(
                job["state"].as_str(),
                Some("succeeded" | "failed" | "cancelled")
            );
            let event = if next_revision != revision || terminal {
                Event::default()
                    .event("job")
                    .json_data(&job)
                    .unwrap_or_default()
            } else {
                Event::default().comment("heartbeat")
            };
            Some((Ok(event), (state, job_id, headers, next_revision, terminal)))
        },
    );
    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}

pub(super) fn start_worker(state: AppState) {
    tokio::spawn(async move {
        recover_interrupted(&state);
        cleanup_stale_upload_temps(&state, &deployment_root());
        loop {
            if let Some(job_id) = claim_next(&state) {
                let heartbeat_state = state.clone();
                let heartbeat_job = job_id.clone();
                let heartbeat = tokio::spawn(async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(8));
                    loop {
                        interval.tick().await;
                        let db = heartbeat_state.lock().unwrap();
                        let at = now();
                        let _=db.execute("UPDATE deployment_jobs SET lease_until=?1,heartbeat_at=?2,updated_at=?2 WHERE id=?3 AND state IN ('validating','running','verifying')",params![at+30,at,heartbeat_job]);
                    }
                });
                process_job(state.clone(), job_id).await;
                heartbeat.abort();
            } else {
                tokio::time::sleep(Duration::from_millis(800)).await;
            }
        }
    });
}

fn cleanup_stale_upload_temps(state: &AppState, root: &FsPath) {
    let uploads = root.join("uploads");
    let Ok(entries) = fs::read_dir(uploads) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_file() || meta.file_type().is_symlink() {
            continue;
        }
        let age = meta
            .modified()
            .ok()
            .and_then(|time| SystemTime::now().duration_since(time).ok());
        if name.starts_with("upload-")
            && name.ends_with(".tmp")
            && age.is_some_and(|age| age > Duration::from_secs(60 * 60))
        {
            let _ = fs::remove_file(path);
            continue;
        }
        let Some(job_id) = name.strip_suffix(".zip") else {
            continue;
        };
        if !valid_key(job_id) {
            continue;
        }
        let terminal = state
            .lock()
            .unwrap()
            .query_row(
                "SELECT state IN ('failed','cancelled') FROM deployment_jobs WHERE id=?1",
                [job_id],
                |row| row.get::<_, bool>(0),
            )
            .unwrap_or(false);
        if terminal && age.is_some_and(|age| age > Duration::from_secs(7 * 24 * 60 * 60)) {
            let _ = fs::remove_file(path);
        }
    }
}

fn recover_interrupted(state: &AppState) {
    recover_interrupted_at(state, &deployment_root());
}

fn recover_interrupted_at(state: &AppState, root: &FsPath) {
    let pending = {
        let db = state.lock().unwrap();
        db.prepare("SELECT id,application_id,actor,healthcheck_file,cancel_requested FROM deployment_jobs WHERE state IN ('validating','running','verifying')").and_then(|mut s|s.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,bool>(4)?))).and_then(|rows|rows.collect::<Result<Vec<_>,_>>())).unwrap_or_default()
    };
    for (job, app, actor, healthcheck, cancel_requested) in pending {
        let app_root = root.join("apps").join(&app);
        let current = fs::read_link(app_root.join("current")).ok();
        let expected = FsPath::new("releases").join(&job);
        let release = app_root.join("releases").join(&job);
        let activated = current.as_ref() == Some(&expected)
            && verify_release(&release, healthcheck.as_deref()).is_ok();
        let db = state.lock().unwrap();
        let at = now();
        let _=db.execute("UPDATE deployment_job_steps SET state='interrupted',finished_at=?1,message=message||' (runner restart)' WHERE job_id=?2 AND state='running'",params![at,job]);
        if activated {
            let previous:Option<(String,String)>=db.query_row("SELECT id,path FROM deployment_releases WHERE application_id=?1 AND state='active' AND id<>?2 ORDER BY created_at DESC LIMIT 1",params![app,job],|r|Ok((r.get(0)?,r.get(1)?))).optional().ok().flatten();
            let _=db.execute("UPDATE deployment_releases SET state='retained' WHERE application_id=?1 AND state='previous'",[&app]);
            let _=db.execute("UPDATE deployment_releases SET state='previous' WHERE application_id=?1 AND state='active' AND id<>?2",params![app,job]);
            let source_commit: Option<String> = db
                .query_row(
                    "SELECT source_commit FROM deployment_jobs WHERE id=?1",
                    [&job],
                    |r| r.get(0),
                )
                .optional()
                .ok()
                .flatten();
            let _=db.execute("INSERT OR IGNORE INTO deployment_releases(id,application_id,job_id,path,state,source_commit,created_at,activated_at) VALUES (?1,?2,?1,?3,'active',?4,?5,?5)",params![job,app,release.to_string_lossy(),source_commit,at]);
            let _ = db.execute(
                "UPDATE deployment_releases SET state='active',activated_at=?1 WHERE id=?2",
                params![at, job],
            );
            let _=db.execute("UPDATE deployment_jobs SET state='succeeded',release_path=?1,previous_release=?2,error_summary=NULL,finished_at=?3,updated_at=?3,worker_id=NULL,lease_until=NULL WHERE id=?4",params![release.to_string_lossy(),previous.as_ref().map(|p|p.1.as_str()),at,job]);
            let _=add_step(&db,&job,"succeeded","succeeded","فعال‌سازی اتمی در لحظهٔ توقف ARG کامل شده بود؛ نتیجه پس از بررسی release بازیابی شد.");
            audit(
                &db,
                &actor,
                "deployment.recovered_success",
                &format!("job={job}; app={app}"),
            );
        } else if cancel_requested {
            let _ = fs::remove_dir_all(app_root.join(format!(".staging-{job}")));
            let _ = fs::remove_dir_all(app_root.join(format!(".work-{job}")));
            if current.as_ref() != Some(&expected) {
                let _ = fs::remove_dir_all(&release);
            }
            let _=db.execute("UPDATE deployment_jobs SET state='cancelled',finished_at=?1,updated_at=?1,worker_id=NULL,lease_until=NULL WHERE id=?2",params![at,job]);
            let _ = add_step(
                &db,
                &job,
                "cancelled",
                "cancelled",
                "Job پس از راه‌اندازی مجدد ARG لغوشده بازیابی شد.",
            );
        } else {
            let _=db.execute("UPDATE deployment_jobs SET state='queued',worker_id=NULL,lease_until=NULL,heartbeat_at=NULL,updated_at=?1 WHERE id=?2",params![at,job]);
            let _ = add_step(
                &db,
                &job,
                "recovered",
                "complete",
                "Job پس از restart به صف امن برگشت.",
            );
        }
    }
}

fn claim_next(state: &AppState) -> Option<String> {
    let db = state.lock().unwrap();
    let at = now();
    let worker = id();
    let selected:Option<String>=db.query_row("SELECT id FROM deployment_jobs WHERE state='queued' AND cancel_requested=0 ORDER BY created_at LIMIT 1",[],|r|r.get(0)).optional().ok().flatten();
    let job = selected?;
    let changed=db.execute("UPDATE deployment_jobs SET state='validating',worker_id=?1,lease_until=?2,heartbeat_at=?3,started_at=COALESCE(started_at,?3),updated_at=?3 WHERE id=?4 AND state='queued'",params![worker,at+30,at,job]).ok()?;
    if changed == 0 {
        return None;
    }
    let _ = add_step(
        &db,
        &job,
        "validating",
        "running",
        "بررسی مالکیت برنامه، فضای مقصد و منبع آغاز شد.",
    );
    Some(job)
}

pub(super) fn progress(state: &AppState, job_id: &str, job_state: &str, step: &str, message: &str) {
    let db = state.lock().unwrap();
    let at = now();
    let _=db.execute("UPDATE deployment_jobs SET state=?1,lease_until=?2,heartbeat_at=?3,updated_at=?3 WHERE id=?4",params![job_state,at+30,at,job_id]);
    let _=db.execute("UPDATE deployment_job_steps SET state='complete',finished_at=?1 WHERE job_id=?2 AND state='running'",params![at,job_id]);
    let _ = add_step(&db, job_id, step, "running", message);
}

fn cancelled(state: &AppState, job_id: &str) -> bool {
    state
        .lock()
        .unwrap()
        .query_row(
            "SELECT cancel_requested FROM deployment_jobs WHERE id=?1",
            [job_id],
            |r| r.get::<_, bool>(0),
        )
        .unwrap_or(true)
}

fn finish(state: &AppState, job_id: &str, final_state: &str, message: &str) {
    let db = state.lock().unwrap();
    let at = now();
    let _=db.execute("UPDATE deployment_job_steps SET state=?1,message=?2,finished_at=?3 WHERE job_id=?4 AND state='running'",params![final_state,message,at,job_id]);
    let _=db.execute("UPDATE deployment_jobs SET state=?1,error_summary=CASE WHEN ?1='failed' THEN ?2 ELSE NULL END,finished_at=?3,updated_at=?3,lease_until=NULL,worker_id=NULL WHERE id=?4",params![final_state,if final_state=="failed"{Some(message)}else{None},at,job_id]);
    let _ = add_step(&db, job_id, final_state, final_state, message);
}

async fn refresh_laravel_runtime(
    app_id: &str,
    runtime_version: &str,
    domain: Option<&str>,
    web_enabled: bool,
) -> Result<Option<(String, String)>, String> {
    let mut nginx_snapshot = None;
    if web_enabled {
        let domain = domain.ok_or("دامنهٔ سایت Laravel پس از فعال‌سازی پیدا نشد.")?;
        let response = privileged_request_async(json!({
            "action":"laravel-web", "app_id":app_id, "domain":domain,
            "runtime_version":runtime_version, "enabled":true
        }))
        .await
        .map_err(|_| "ارتباط با helper برای بازآماده‌سازی Nginx/PHP-FPM برقرار نشد.".to_owned())?;
        if response["ok"] != true {
            return Err(response["error"].as_str().unwrap_or("Nginx و PHP-FPM پس از تغییر release آماده نشدند.").to_owned());
        }
        let data = &response["data"];
        if let (Some(name), Some(content)) = (data["site_name"].as_str(), data["nginx_content"].as_str()) {
            nginx_snapshot = Some((name.to_owned(), content.to_owned()));
        }
    }
    let response = privileged_request_async(json!({"action":"laravel-refresh-services","app_id":app_id}))
        .await
        .map_err(|_| "ارتباط با helper برای راه‌اندازی مجدد سرویس‌های Laravel برقرار نشد.".to_owned())?;
    if response["ok"] != true {
        return Err(response["error"].as_str().unwrap_or("سرویس‌های Laravel پس از تغییر release راه‌اندازی نشدند.").to_owned());
    }
    Ok(nginx_snapshot)
}

fn persist_laravel_nginx_snapshot(state: &AppState, snapshot: &(String, String)) -> Result<(), String> {
    let db = state.lock().unwrap();
    nginx::store_nginx_snapshot(&db, &snapshot.0, &snapshot.1, None)
        .map_err(|error| error.to_string())
}

async fn process_job(state: AppState, job_id: String) {
    let record = {
        let db = state.lock().unwrap();
        db.query_row("SELECT application_id,owner_server_uuid,source_json,healthcheck_file,cancel_requested FROM deployment_jobs WHERE id=?1",[&job_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,bool>(4)?))).optional().ok().flatten()
    };
    let Some((app_id, _owner, source_json, healthcheck, was_cancelled)) = record else {
        return;
    };
    if was_cancelled || cancelled(&state, &job_id) {
        finish(&state, &job_id, "cancelled", "پیش از شروع لغو شد.");
        return;
    }
    let source: Source = match serde_json::from_str(&source_json) {
        Ok(s) => s,
        Err(_) => {
            finish(&state, &job_id, "failed", "تنظیمات منبع Job خوانده نشد.");
            return;
        }
    };
    let root = deployment_root();
    let app_root = root.join("apps").join(&app_id);
    let releases = app_root.join("releases");
    let stage = app_root.join(format!(".staging-{job_id}"));
    let work = app_root.join(format!(".work-{job_id}"));
    if let Err(e) = fs::create_dir_all(&releases).and_then(|_| fs::create_dir_all(&app_root)) {
        finish(
            &state,
            &job_id,
            "failed",
            &format!("فضای استقرار آماده نشد: {e}"),
        );
        return;
    }
    if ensure_space(&app_root, 64 * 1024 * 1024).is_err() {
        finish(
            &state,
            &job_id,
            "failed",
            "فضای آزاد برای استقرار کمتر از 64 MiB است.",
        );
        return;
    }
    let final_release = releases.join(&job_id);
    let current = app_root.join("current");
    let current_link = fs::read_link(&current).ok();
    if current_link.as_ref() == Some(&FsPath::new("releases").join(&job_id))
        && verify_release(&final_release, healthcheck.as_deref()).is_ok()
    {
        let db = state.lock().unwrap();
        let at = now();
        let previous:Option<String>=db.query_row("SELECT path FROM deployment_releases WHERE application_id=?1 AND state='active' AND id<>?2 ORDER BY created_at DESC LIMIT 1",params![app_id,job_id],|r|r.get(0)).optional().ok().flatten();
        let source_commit: Option<String> = db
            .query_row(
                "SELECT source_commit FROM deployment_jobs WHERE id=?1",
                [&job_id],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        let _=db.execute("UPDATE deployment_releases SET state='retained' WHERE application_id=?1 AND state='previous'",[&app_id]);
        let _=db.execute("UPDATE deployment_releases SET state='previous' WHERE application_id=?1 AND state='active' AND id<>?2",params![app_id,job_id]);
        let _=db.execute("INSERT OR IGNORE INTO deployment_releases(id,application_id,job_id,path,state,source_commit,created_at,activated_at) VALUES (?1,?2,?1,?3,'active',?4,?5,?5)",params![job_id,app_id,final_release.to_string_lossy(),source_commit,at]);
        let _ = db.execute(
            "UPDATE deployment_releases SET state='active',activated_at=?1 WHERE id=?2",
            params![at, job_id],
        );
        let _ = db.execute(
            "UPDATE deployment_jobs SET release_path=?1,previous_release=?2 WHERE id=?3",
            params![final_release.to_string_lossy(), previous, job_id],
        );
        drop(db);
        finish(
            &state,
            &job_id,
            "succeeded",
            "فعال‌سازی قبلی پس از بررسی release بازیابی شد.",
        );
        return;
    }
    // A crash after staging rename but before pointer activation leaves an orphan directory.
    // It is safe to discard because `current` has not selected it.
    if final_release.exists() {
        let _ = fs::remove_dir_all(&final_release);
    }
    let _ = fs::remove_dir_all(&stage);
    let _ = fs::remove_dir_all(&work);
    if let Err(e) = fs::create_dir_all(&stage).and_then(|_| fs::create_dir_all(&work)) {
        finish(
            &state,
            &job_id,
            "failed",
            &format!("مسیر staging ساخته نشد: {e}"),
        );
        return;
    }
    progress(
        &state,
        &job_id,
        "running",
        "acquiring",
        "منبع در فضای staging مجزا دریافت یا کپی می‌شود؛ هیچ اسکریپت پروژه اجرا نمی‌شود.",
    );
    let source_result = match &source {
        Source::Git { repository, reference, ssh_key_id } => {
            let key_path = match ssh_key_id.as_deref() {
                Some(key_id) => {
                    let db = state.lock().unwrap();
                    match ssh_keys::deployment_key_path(&db, &app_id, repository, key_id) {
                        Ok(path) => Some(path),
                        Err(message) => {
                            drop(db);
                            finish(&state, &job_id, "failed", &message);
                            let _ = fs::remove_dir_all(&stage);
                            let _ = fs::remove_dir_all(&work);
                            return;
                        }
                    }
                }
                None => None,
            };
            match acquire_git(repository, reference, key_path.as_deref(), &work).await {
            Ok(revision) => {
                let pinned = Source::Git {
                    repository: repository.clone(),
                    reference: revision.clone(),
                    ssh_key_id: ssh_key_id.clone(),
                };
                if let Ok(source_json) = serde_json::to_string(&pinned) {
                    let db = state.lock().unwrap();
                    let _=db.execute("UPDATE deployment_jobs SET source_json=?1,source_commit=?2,updated_at=?3 WHERE id=?4",params![source_json,revision,now(),job_id]);
                }
                copy_tree(&work, &stage).map(|_| Some(revision))
            }
            Err(message) => Err(message),
            }
        }
        Source::Directory { path } => acquire_directory(path, &stage).map(|_| None),
        Source::Archive { sha256 } => {
            let archive_path = root.join("uploads").join(format!("{job_id}.zip"));
            verify_archive_digest(&archive_path, &sha256)
                .and_then(|_| extract_archive(&archive_path, &stage))
                .map(|_| None)
        }
    };
    let commit = match source_result {
        Ok(value) => value,
        Err(message) => {
            let _ = fs::remove_dir_all(&stage);
            let _ = fs::remove_dir_all(&work);
            finish(&state, &job_id, "failed", &message);
            return;
        }
    };
    let _ = fs::remove_dir_all(&work);
    if cancelled(&state, &job_id) {
        let _ = fs::remove_dir_all(&stage);
        finish(
            &state,
            &job_id,
            "cancelled",
            "منبع دریافت شد اما پیش از بررسی نهایی لغو شد.",
        );
        return;
    }
    let (framework, build_runtime_version, domain, static_output_path, static_spa_fallback, build_environment_json, node_build_script, node_start_script) = {
        let db = state.lock().unwrap();
        db.query_row(
            "SELECT framework,build_runtime_version,domain,static_output_path,static_spa_fallback,build_environment_json,node_build_script,node_start_script FROM applications WHERE id=?1",
            [&app_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, String>(3)?, row.get::<_, bool>(4)?, row.get::<_, String>(5)?, row.get::<_,String>(6)?, row.get::<_,String>(7)?)),
        )
        .optional()
        .ok()
        .flatten()
        .unwrap_or_else(|| ("custom".to_owned(), "system".to_owned(), None, "auto".to_owned(), true, "{}".to_owned(), "auto".to_owned(), "auto".to_owned()))
    };
    let build_environment = match node::parse_public_build_environment(&build_environment_json) {
        Ok(variables) => variables,
        Err(message) => {
            let _ = fs::remove_dir_all(&stage);
            finish(&state, &job_id, "failed", &message);
            return;
        }
    };
    let static_site = match node::is_static_deployment(&framework, &stage, &static_output_path) {
        Ok(value) => value,
        Err(message) => {
            let _ = fs::remove_dir_all(&stage);
            finish(&state, &job_id, "failed", &message);
            return;
        }
    };
    if static_site && domain.as_deref().map_or(true, str::is_empty) {
        let _ = fs::remove_dir_all(&stage);
        finish(&state, &job_id, "failed", "برای انتشار سایت استاتیک، دامنه را در مشخصات اپ ثبت کن تا Nginx همان سایت را بسازد.");
        return;
    }
    let node_project = apps::is_node_runtime_framework(&framework)
        || apps::is_static_framework(&framework)
        || apps::is_auto_framework(&framework);
    if framework == "laravel" {
        progress(
            &state,
            &job_id,
            "verifying",
            "laravel-persistent-data",
            "پوشهٔ storage و .env را برای ماندگاری میان releaseها آماده می‌کند؛ هیچ دستور پروژه‌ای اجرا نمی‌شود.",
        );
        if let Err(message) = laravel::prepare_release(&app_id, &stage) {
            let _ = fs::remove_dir_all(&stage);
            finish(&state, &job_id, "failed", &message);
            return;
        }
    }
    if node_project {
        let has_manifest = node::has_package_manifest(&stage);
        progress(
            &state,
            &job_id,
            "running",
            "node-build",
            if static_site { "lockfile و خروجی React/Vue/Vite بررسی می‌شوند؛ build در محیط موقت systemd اجرا می‌شود." } else { "lockfile و scripts.start بررسی می‌شوند؛ وابستگی‌ها و build در محیط موقت systemd اجرا می‌شوند." },
        );
        if has_manifest {
            let should_build = if static_site {
                match node::static_build_available(&stage) {
                    Ok(available) => available,
                    Err(message) => {
                        let _ = fs::remove_dir_all(&stage);
                        finish(&state, &job_id, "failed", &message);
                        return;
                    }
                }
            } else {
                true
            };
            if should_build {
                if let Err(message) = node::prepare_release(&app_id, &stage) {
                    let _ = fs::remove_dir_all(&stage);
                    finish(&state, &job_id, "failed", &message);
                    return;
                }
                let build = if static_site {
                    node::prepare_static_build(&state, &app_id, &job_id, &stage, &build_runtime_version, &build_environment, &node_build_script).await
                } else {
                    node::prepare_build(&state, &app_id, &job_id, &stage, &build_runtime_version, &build_environment, &node_build_script, &node_start_script).await
                };
                if let Err(message) = build {
                    let _ = fs::remove_dir_all(&stage);
                    finish(&state, &job_id, "failed", &message);
                    return;
                }
            }
        } else if !static_site {
            let _ = fs::remove_dir_all(&stage);
            finish(&state, &job_id, "failed", "برای اجرای Node.js/SSR باید package.json و lockfile پشتیبانی‌شده وجود داشته باشد.");
            return;
        }
    }
    let resolved_static_output = if static_site {
        match node::resolve_static_output_path(&stage, &static_output_path) {
            Ok(path) => Some(path),
            Err(message) => {
                let _ = fs::remove_dir_all(&stage);
                finish(&state, &job_id, "failed", &message);
                return;
            }
        }
    } else { None };
    progress(
        &state,
        &job_id,
        "verifying",
        "verifying",
        "ساختار release، فایل Health check و symlinkها بررسی می‌شوند.",
    );
    if let Err(message) = verify_release(&stage, if static_site { None } else { healthcheck.as_deref() }) {
        let _ = fs::remove_dir_all(&stage);
        finish(&state, &job_id, "failed", &message);
        return;
    }
    if cancelled(&state, &job_id) {
        let _ = fs::remove_dir_all(&stage);
        finish(
            &state,
            &job_id,
            "cancelled",
            "پیش از فعال‌سازی لغو شد؛ نسخهٔ جاری تغییری نکرد.",
        );
        return;
    }
    if let Err(e) = fs::rename(&stage, &final_release) {
        finish(
            &state,
            &job_id,
            "failed",
            &format!("انتقال release به مسیر نهایی ناموفق بود: {e}"),
        );
        return;
    }
    if node_project && !static_site && node::has_package_manifest(&final_release) {
        if let Err(message) = node::activate_release(&final_release) {
            let _ = fs::remove_dir_all(&final_release);
            finish(&state, &job_id, "failed", &format!("فایل محیط پایدار به release متصل نشد: {message}"));
            return;
        }
    }
    if current.exists() && !fs::symlink_metadata(&current).is_ok_and(|m| m.file_type().is_symlink())
    {
        finish(
            &state,
            &job_id,
            "failed",
            "مسیر current symlink نیست؛ فعال‌سازی برای جلوگیری از جایگزینی فایل واقعی متوقف شد.",
        );
        let _ = fs::remove_dir_all(&final_release);
        return;
    }
    let previous = fs::read_link(&current)
        .ok()
        .map(|p| p.to_string_lossy().to_string());
    progress(
        &state,
        &job_id,
        "verifying",
        "activating",
        "release آماده است؛ اشاره‌گر نسخه با rename اتمی فعال می‌شود.",
    );
    let link_tmp = app_root.join(format!(".current-{job_id}"));
    let _ = fs::remove_file(&link_tmp);
    if let Err(e) = std::os::unix::fs::symlink(FsPath::new("releases").join(&job_id), &link_tmp)
        .and_then(|_| fs::rename(&link_tmp, &current))
    {
        let _ = fs::remove_file(&link_tmp);
        finish(
            &state,
            &job_id,
            "failed",
            &format!("فعال‌سازی اتمی انجام نشد: {e}"),
        );
        let _ = fs::remove_dir_all(&final_release);
        return;
    }
    let mut nginx_snapshot = None;
    if static_site {
        let output_path = resolved_static_output.as_deref().unwrap_or(".");
        let configured = privileged_request_async(json!({"action":"node-nginx-static","application_id":app_id,"domain":domain.as_deref().unwrap_or_default(),"output_path":output_path,"spa_fallback":static_spa_fallback})).await;
        let config_result = match configured {
            Ok(value) if value["ok"] == true => {
                let name = value["data"]["site_name"].as_str();
                let content = value["data"]["content"].as_str();
                name.zip(content).map(|(name, content)| (name.to_owned(), content.to_owned()))
                    .ok_or_else(|| "اطلاعات سایت ساخته‌شده از Nginx کامل نیست.".to_owned())
            }
            Ok(value) => Err(value["error"].as_str().unwrap_or("پیکربندی Static در Nginx انجام نشد.").to_owned()),
            Err((_, body)) => Err(body["error"].as_str().unwrap_or("ارتباط helper برای پیکربندی Static برقرار نشد.").to_owned()),
        };
        match config_result {
            Ok(snapshot) => nginx_snapshot = Some(snapshot),
            Err(message) => {
                let recovery = restore_current_pointer(&app_root, previous.as_deref(), &job_id);
                let detail = recovery.map(|_| format!("{message}؛ release قبلی حفظ شد.")).unwrap_or_else(|error| format!("{message}؛ بازگرداندن release قبلی هم ناموفق بود: {error}"));
                finish(&state, &job_id, "failed", &detail);
                return;
            }
        }
    }
    if framework == "laravel" {
        let laravel_runtime = {
            let db = state.lock().unwrap();
            db.query_row(
                "SELECT runtime_version,domain,enabled FROM applications a LEFT JOIN laravel_app_sites s ON s.application_id=a.id WHERE a.id=?1",
                [&app_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, Option<bool>>(2)?.unwrap_or(false))),
            ).optional().ok().flatten()
        };
        if let Some((runtime_version, domain, web_enabled)) = laravel_runtime {
            match refresh_laravel_runtime(&app_id, &runtime_version, domain.as_deref(), web_enabled).await {
                Ok(snapshot) => nginx_snapshot = snapshot,
                Err(message) => {
                    let recovery = restore_current_pointer(&app_root, previous.as_deref(), &job_id);
                    let detail = match recovery {
                        Ok(()) => match refresh_laravel_runtime(&app_id, &runtime_version, domain.as_deref(), web_enabled).await {
                            Ok(snapshot) => {
                                if let Some(snapshot) = snapshot.as_ref() {
                                    let _ = persist_laravel_nginx_snapshot(&state, snapshot);
                                }
                                format!("{message} اشاره‌گر به release قبلی برگشت و سرویس‌های قبلی دوباره بررسی شدند؛ دیتابیس تغییر نکرد.")
                            }
                            Err(recovery_error) => format!("{message} کد به release قبلی برگشت، اما آماده‌سازی دوبارهٔ سرویس هم شکست خورد: {recovery_error}. وضعیت Nginx و systemd را بررسی کن."),
                        },
                        Err(rollback_error) => format!("{message} بازگرداندن اشاره‌گر هم شکست خورد: {rollback_error}. مسیر current را فوراً بررسی کن."),
                    };
                    finish(&state, &job_id, "failed", &detail);
                    return;
                }
            }
        }
    }
    if apps::is_node_runtime_framework(&framework) || (apps::is_auto_framework(&framework) && !static_site) {
        if let Err(message) = node::refresh_after_deploy(&app_id, previous.as_deref()).await {
            finish(&state, &job_id, "failed", &message);
            return;
        }
    }
    let db = state.lock().unwrap();
    let at = now();
    let _=db.execute("INSERT OR REPLACE INTO deployment_releases(id,application_id,job_id,path,state,source_commit,created_at,activated_at) VALUES (?1,?2,?1,?3,'active',?4,?5,?5)",params![job_id,app_id,final_release.to_string_lossy(),commit,at]);
    let _=db.execute("UPDATE deployment_releases SET state='retained' WHERE application_id=?1 AND state='previous'",[&app_id]);
    let _=db.execute("UPDATE deployment_releases SET state='previous' WHERE application_id=?1 AND id<>?2 AND state='active'",params![app_id,job_id]);
    let _ = db.execute(
        "UPDATE deployment_jobs SET release_path=?1,previous_release=?2 WHERE id=?3",
        params![final_release.to_string_lossy(), previous, job_id],
    );
    let snapshot_warning = nginx_snapshot.as_ref().and_then(|snapshot| {
        nginx::store_nginx_snapshot(&db, &snapshot.0, &snapshot.1, None)
            .err()
            .map(|problem| problem.to_string())
    });
    drop(db);
    let success_message = snapshot_warning.map_or_else(
        || match framework.as_str() {
            "laravel" => "release فعال شد؛ کانفیگ وب و سرویس‌های Laravel بررسی شدند. Migration دیتابیس خودکار اجرا نمی‌شود.".to_owned(),
            "node" | "next" | "nuxt" => "release SSR فعال شد؛ سرویس Node در صورت فعال‌بودن restart و سلامت loopback آن بررسی شد.".to_owned(),
            "static" | "react" | "vue" | "vite" => "خروجی سایت Static ساخته و از طریق Nginx منتشر شد؛ برای این سایت process جداگانه اجرا نمی‌شود.".to_owned(),
            "auto" if static_site => "چارچوب از manifest تشخیص داده شد؛ خروجی Static از طریق Nginx منتشر شد و process جداگانه‌ای اجرا نمی‌شود.".to_owned(),
            "auto" => "چارچوب از manifest تشخیص داده شد؛ release SSR فعال و سرویس Node بررسی شد.".to_owned(),
            _ => "release فعال شد و اشاره‌گر جاری به‌صورت اتمی جابه‌جا شد.".to_owned(),
        },
        |warning| format!("release فعال شد، اما snapshot کانفیگ Nginx همگام نشد: {warning}"),
    );
    finish(&state, &job_id, "succeeded", &success_message);
    prune_releases(&state, &app_id);
    let _ = fs::remove_file(
        deployment_root()
            .join("uploads")
            .join(format!("{job_id}.zip")),
    );
}

async fn refresh_static_runtime(
    app_id: &str,
    framework: &str,
    domain: &str,
    configured_output: &str,
    spa_fallback: bool,
) -> Result<Option<(String, String)>, String> {
    let app_root = deployment_root().join("apps").join(app_id);
    let releases = app_root.join("releases").canonicalize().map_err(|error| format!("پوشهٔ releaseها پیدا نشد: {error}"))?;
    let release = app_root.join("current").canonicalize().map_err(|error| format!("release جاری پیدا نشد: {error}"))?;
    if !release.starts_with(&releases) || !release.is_dir() {
        return Err("release جاری خارج از مسیر مدیریت‌شده است.".to_owned());
    }
    if !node::is_static_deployment(framework, &release, configured_output)? {
        return Ok(None);
    }
    let output = node::resolve_static_output_path(&release, configured_output)?;
    let response = privileged_request_async(json!({"action":"node-nginx-static","application_id":app_id,"domain":domain,"output_path":output,"spa_fallback":spa_fallback}))
        .await
        .map_err(|(_, body)| body["error"].as_str().unwrap_or("ارتباط helper برای تنظیم Nginx برقرار نشد.").to_owned())?;
    if response["ok"] != true {
        return Err(response["error"].as_str().unwrap_or("تنظیم Nginx سایت Static ناموفق بود.").to_owned());
    }
    let name = response["data"]["site_name"].as_str().ok_or("نام سایت Nginx از helper دریافت نشد.")?;
    let content = response["data"]["content"].as_str().ok_or("متن کانفیگ Nginx از helper دریافت نشد.")?;
    Ok(Some((name.to_owned(), content.to_owned())))
}

fn restore_current_pointer(
    app_root: &FsPath,
    previous: Option<&str>,
    job_id: &str,
) -> Result<(), String> {
    let current = app_root.join("current");
    if !fs::symlink_metadata(&current)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("current بعد از فعال‌سازی symlink معتبر نیست.".into());
    }
    let Some(previous) = previous else {
        fs::remove_file(current)
            .map_err(|error| format!("حذف اشاره‌گر نسخهٔ ناموفق نشد: {error}"))?;
        return Ok(());
    };
    let releases = app_root
        .join("releases")
        .canonicalize()
        .map_err(|error| format!("پوشهٔ releaseها قابل بررسی نیست: {error}"))?;
    let candidate = app_root
        .join(previous)
        .canonicalize()
        .map_err(|error| format!("release قبلی پیدا نشد: {error}"))?;
    if !candidate.starts_with(&releases) || !candidate.is_dir() {
        return Err("release قبلی خارج از محدودهٔ مجاز است.".into());
    }
    let temp = app_root.join(format!(".restore-{job_id}"));
    let _ = fs::remove_file(&temp);
    std::os::unix::fs::symlink(previous, &temp)
        .map_err(|error| format!("ساخت symlink بازگردانی ناموفق بود: {error}"))?;
    fs::rename(&temp, current)
        .map_err(|error| format!("جابه‌جایی اتمی به release قبلی ناموفق بود: {error}"))
}

fn prune_releases(state: &AppState, app_id: &str) {
    prune_releases_at(state, &deployment_root(), app_id);
}

fn prune_releases_at(state: &AppState, root: &FsPath, app_id: &str) {
    let app_root = root.join("apps").join(app_id);
    let release_root = app_root.join("releases");
    let Ok(canonical_root) = release_root.canonicalize() else {
        return;
    };
    let current = fs::read_link(app_root.join("current"))
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
        });
    let entries = {
        let db = state.lock().unwrap();
        let Ok(mut statement)=db.prepare("SELECT id,path,state FROM deployment_releases WHERE application_id=?1 ORDER BY created_at DESC,id DESC")else{return};
        statement
            .query_map([app_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
            .unwrap_or_default()
    };
    for (index, (release_id, path, state_name)) in entries.into_iter().enumerate() {
        if index < 5
            || state_name == "active"
            || state_name == "previous"
            || current.as_deref() == Some(release_id.as_str())
        {
            continue;
        }
        let candidate = FsPath::new(&path);
        let Ok(canonical) = candidate.canonicalize() else {
            continue;
        };
        if canonical == canonical_root
            || !canonical.starts_with(&canonical_root)
            || fs::symlink_metadata(candidate).is_ok_and(|meta| meta.file_type().is_symlink())
        {
            continue;
        }
        if fs::remove_dir_all(&canonical).is_ok() {
            let db = state.lock().unwrap();
            let _=db.execute("UPDATE deployment_releases SET state='expired' WHERE id=?1 AND application_id=?2 AND state NOT IN ('active','previous')",params![release_id,app_id]);
            audit(
                &db,
                "arg",
                "deployment.release_prune",
                &format!("app={app_id}; release={release_id}"),
            );
        }
    }
}

fn ensure_space(path: &FsPath, needed: u64) -> io::Result<()> {
    use std::ffi::CString;
    let c = CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| io::Error::other("invalid path"))?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::statvfs(c.as_ptr(), &mut stat) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    let available = (stat.f_bavail as u128)
        .saturating_mul(stat.f_frsize as u128)
        .min(u64::MAX as u128) as u64;
    if available < needed {
        Err(io::Error::new(
            io::ErrorKind::StorageFull,
            "insufficient disk space",
        ))
    } else {
        Ok(())
    }
}

async fn acquire_git(
    repository: &str,
    reference: &str,
    private_key: Option<&FsPath>,
    destination: &FsPath,
) -> Result<String, String> {
    let is_public = valid_repo(repository) && private_key.is_none();
    let is_private = ssh_keys::is_github_ssh_repository(repository) && private_key.is_some();
    if !(is_public || is_private) || !valid_reference(reference) {
        return Err("URL مخزن یا reference مجاز نیست.".into());
    }
    let ssh_command = if let Some(private_key) = private_key {
        if !private_key.is_absolute()
            || private_key.file_name().and_then(OsStr::to_str).map_or(true, |name| !ssh_keys::valid_key_id(name))
            || !private_key.parent().is_some_and(|parent| parent == FsPath::new("/var/lib/arg/ssh-keys"))
        {
            return Err("مسیر کلید SSH با ریشهٔ امن ARG تطبیق ندارد.".into());
        }
        let known_hosts = ssh_keys::ensure_github_host_keys().await?;
        if known_hosts != FsPath::new("/var/lib/arg/ssh-keys/github_known_hosts") {
            return Err("مسیر Host Keyهای GitHub با مسیر امن ARG تطبیق ندارد.".into());
        }
        Some(format!(
            "ssh -F /dev/null -i /var/lib/arg/ssh-keys/{} -o IdentitiesOnly=yes -o BatchMode=yes -o StrictHostKeyChecking=yes -o ConnectTimeout=15 -o UserKnownHostsFile=/var/lib/arg/ssh-keys/github_known_hosts -o GlobalKnownHostsFile=/dev/null",
            private_key.file_name().and_then(OsStr::to_str).unwrap_or_default()
        ))
    } else {
        None
    };
    fs::create_dir_all(destination).map_err(|e| format!("پوشهٔ Git ساخته نشد: {e}"))?;
    let run = tokio::process::Command::new("git")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "protocol.file.allow=never",
            "init",
            "--quiet",
        ])
        .current_dir(destination)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();
    command_output(run, Duration::from_secs(15), "git init").await?;
    let remote = tokio::process::Command::new("git")
        .args(["remote", "add", "origin", repository])
        .current_dir(destination)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output();
    command_output(remote, Duration::from_secs(15), "ثبت مخزن").await?;
    let fetch = tokio::process::Command::new("git")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "protocol.file.allow=never",
            "-c",
            "http.followRedirects=false",
            "fetch",
            "--depth=1",
            "--no-tags",
            "origin",
            reference,
        ])
        .current_dir(destination)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_SSH_COMMAND", ssh_command.as_deref().unwrap_or("ssh -o BatchMode=yes -o StrictHostKeyChecking=yes"))
        .kill_on_drop(true)
        .output();
    command_output_bounded(
        fetch,
        Duration::from_secs(180),
        "دریافت Git",
        destination,
        MAX_RELEASE_BYTES,
    )
    .await?;
    let checkout = tokio::process::Command::new("git")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "checkout",
            "--quiet",
            "--detach",
            "FETCH_HEAD",
        ])
        .current_dir(destination)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .kill_on_drop(true)
        .output();
    command_output_bounded(
        checkout,
        Duration::from_secs(30),
        "انتخاب revision",
        destination,
        MAX_RELEASE_BYTES,
    )
    .await?;
    let rev = tokio::process::Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(destination)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .await
        .map_err(|e| format!("خواندن revision ناموفق بود: {e}"))?;
    if !rev.status.success() {
        return Err("commit دریافت‌شده قابل شناسایی نیست.".into());
    }
    Ok(String::from_utf8_lossy(&rev.stdout)
        .trim()
        .chars()
        .take(64)
        .collect())
}

async fn command_output<F>(future: F, timeout: Duration, label: &str) -> Result<(), String>
where
    F: std::future::Future<Output = io::Result<std::process::Output>>,
{
    let output = tokio::time::timeout(timeout, future)
        .await
        .map_err(|_| format!("{label} از مهلت زمانی گذشت."))?
        .map_err(|e| format!("{label} اجرا نشد: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr)
        .chars()
        .take(700)
        .collect::<String>();
    Err(format!(
        "{label} ناموفق بود: {}",
        stderr.trim().replace('\0', "")
    ))
}

async fn command_output_bounded<F>(
    future: F,
    timeout: Duration,
    label: &str,
    directory: &FsPath,
    max_bytes: u64,
) -> Result<(), String>
where
    F: std::future::Future<Output = io::Result<std::process::Output>>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    tokio::pin!(future);
    loop {
        tokio::select! {
            output = &mut future => {
                let output = output.map_err(|e| format!("{label} اجرا نشد: {e}"))?;
                if output.status.success() {
                    return Ok(());
                }
                let stderr = String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(700)
                    .collect::<String>();
                return Err(format!("{label} ناموفق بود: {}", stderr.trim().replace('\0', "")));
            }
            _ = tokio::time::sleep_until(deadline) => {
                return Err(format!("{label} از مهلت زمانی گذشت."));
            }
            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                match bounded_tree_size(directory, max_bytes) {
                    Ok(size) if size > max_bytes => {
                        return Err(format!("حجم مخزن Git از سقف {} GiB عبور کرد؛ دریافت متوقف شد.", max_bytes / (1024 * 1024 * 1024)));
                    }
                    Ok(_) => {}
                    Err(error) => return Err(format!("اندازهٔ دریافت Git کنترل نشد: {error}")),
                }
                ensure_space(directory, 64 * 1024 * 1024)
                    .map_err(|_| "فضای خالی دیسک هنگام دریافت Git به کمتر از 64 MiB رسید.".to_owned())?;
            }
        }
    }
}

fn bounded_tree_size(root: &FsPath, max_bytes: u64) -> io::Result<u64> {
    let mut pending = vec![root.to_path_buf()];
    let mut total = 0u64;
    let mut entries = 0usize;
    while let Some(directory) = pending.pop() {
        for item in fs::read_dir(directory)? {
            let item = item?;
            entries += 1;
            if entries > MAX_ARCHIVE_ENTRIES {
                return Ok(max_bytes.saturating_add(1));
            }
            let metadata = fs::symlink_metadata(item.path())?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(item.path());
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
                if total > max_bytes {
                    return Ok(total);
                }
            }
        }
    }
    Ok(total)
}

fn acquire_directory(path: &str, dest: &FsPath) -> Result<(), String> {
    let source = PathBuf::from(path)
        .canonicalize()
        .map_err(|e| format!("پوشهٔ منبع خوانده نشد: {e}"))?;
    let allowed = [FsPath::new("/var/www"), FsPath::new("/opt/arg/apps")];
    if !allowed.iter().any(|root| source.starts_with(root)) {
        return Err("پوشهٔ موجود باید زیر /var/www یا /opt/arg/apps باشد.".into());
    }
    let meta = fs::metadata(&source).map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err("منبع موجود باید پوشه باشد.".into());
    }
    copy_tree(&source, dest)
}

fn copy_tree(source: &FsPath, dest: &FsPath) -> Result<(), String> {
    let mut count = 0usize;
    let mut bytes = 0u64;
    fn walk(src: &FsPath, dst: &FsPath, count: &mut usize, bytes: &mut u64) -> Result<(), String> {
        for entry in fs::read_dir(src).map_err(|e| format!("پوشهٔ منبع فهرست نشد: {e}"))?
        {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            if excluded_source_path(FsPath::new(&name)) {
                continue;
            }
            let meta = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                return Err(format!(
                    "منبع دارای symlink است و برای جلوگیری از خروج از پوشه پذیرفته نشد: {}",
                    name.to_string_lossy()
                ));
            }
            *count += 1;
            if *count > MAX_ARCHIVE_ENTRIES {
                return Err("تعداد فایل‌های منبع از ۲۰٬۰۰۰ بیشتر است.".into());
            }
            let target = dst.join(&name);
            if meta.is_dir() {
                fs::create_dir(&target).map_err(|e| e.to_string())?;
                walk(&entry.path(), &target, count, bytes)?
            } else if meta.is_file() {
                *bytes = bytes.saturating_add(meta.len());
                if *bytes > MAX_RELEASE_BYTES {
                    return Err("حجم فایل‌های release از ۲ GiB بیشتر است.".into());
                }
                ensure_space(dst, meta.len().saturating_add(64 * 1024 * 1024))
                    .map_err(|_| "فضای خالی دیسک برای کپی release کافی نیست.".to_owned())?;
                fs::copy(entry.path(), &target).map_err(|e| format!("کپی فایل ناموفق بود: {e}"))?;
                let mode = if meta.mode() & 0o111 != 0 {
                    0o755
                } else {
                    0o644
                };
                fs::set_permissions(&target, fs::Permissions::from_mode(mode)).ok();
            } else {
                return Err("فایل خاص یا device در منبع پذیرفته نمی‌شود.".into());
            }
        }
        Ok(())
    }
    walk(source, dest, &mut count, &mut bytes)
}

fn extract_archive(archive_path: &FsPath, dest: &FsPath) -> Result<(), String> {
    let file = fs::File::open(archive_path).map_err(|e| format!("فایل ZIP موقت پیدا نشد: {e}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("ZIP معتبر نیست: {e}"))?;
    if zip.len() > MAX_ARCHIVE_ENTRIES {
        return Err("ZIP بیش از ۲۰٬۰۰۰ ورودی دارد.".into());
    }
    // Browser-generated ZIPs commonly wrap the project in one directory.
    // Strip that directory only when every file is below the same top-level
    // folder; archives with files at their root keep their original layout.
    let wrapper = common_archive_wrapper(&mut zip)?;
    let mut expanded = 0u64;
    for i in 0..zip.len() {
        let mut item = zip
            .by_index(i)
            .map_err(|e| format!("ورودی ZIP خوانده نشد: {e}"))?;
        let Some(relative) = item.enclosed_name() else {
            return Err("مسیر ZIP شامل traversal یا مسیر مطلق است.".into());
        };
        if !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        {
            return Err("مسیر ZIP معتبر نیست.".into());
        }
        if let Some(mode) = item.unix_mode() {
            if mode & 0o170000 == 0o120000 {
                return Err("symlink داخل ZIP پذیرفته نمی‌شود.".into());
            }
            if ![0o100000, 0o040000].contains(&(mode & 0o170000)) {
                return Err("فایل خاص داخل ZIP پذیرفته نمی‌شود.".into());
            }
        }
        // Ignore known archive metadata before stripping the common wrapper;
        // macOS ZIPs often contain __MACOSX beside the actual project folder.
        if excluded_source_path(&relative) {
            continue;
        }
        let relative = match wrapper.as_ref() {
            Some(wrapper) => match relative.strip_prefix(wrapper) {
                Ok(path) => path.to_path_buf(),
                Err(_) => return Err("ساختار پوشهٔ ریشهٔ ZIP ناسازگار است.".into()),
            },
            None => relative,
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        if excluded_source_path(&relative) {
            continue;
        }
        let size = item.size();
        expanded = expanded.saturating_add(size);
        if expanded > MAX_RELEASE_BYTES
            || (size > 10 * 1024 * 1024 && item.compressed_size().saturating_mul(100) < size)
        {
            return Err("ZIP از حد حجم بازشده یا نسبت فشرده‌سازی امن عبور می‌کند.".into());
        }
        ensure_space(dest, size.saturating_add(64 * 1024 * 1024))
            .map_err(|_| "فضای خالی دیسک برای استخراج release کافی نیست.".to_owned())?;
        let target = dest.join(relative);
        if item.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?
        }
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|e| format!("ساخت فایل ZIP ناموفق بود: {e}"))?;
        let copied =
            io::copy(&mut item, &mut out).map_err(|e| format!("بازکردن ZIP ناموفق بود: {e}"))?;
        if copied != size {
            return Err("اندازهٔ واقعی فایل ZIP با مقدار ثبت‌شده برابر نیست.".into());
        }
        #[cfg(unix)]
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).ok();
    }
    Ok(())
}

fn common_archive_wrapper(zip: &mut zip::ZipArchive<fs::File>) -> Result<Option<PathBuf>, String> {
    let mut wrapper: Option<PathBuf> = None;
    let mut files = 0usize;
    for index in 0..zip.len() {
        let item = zip.by_index(index).map_err(|error| format!("ورودی ZIP خوانده نشد: {error}"))?;
        let Some(path) = item.enclosed_name() else {
            return Err("مسیر ZIP شامل traversal یا مسیر مطلق است.".into());
        };
        if !path.components().all(|component| matches!(component, Component::Normal(_))) {
            return Err("مسیر ZIP معتبر نیست.".into());
        }
        if item.is_dir() { continue; }
        if excluded_source_path(&path) { continue; }
        files += 1;
        let mut components = path.components();
        let Some(Component::Normal(first)) = components.next() else { return Ok(None) };
        if components.next().is_none() { return Ok(None); }
        let candidate = PathBuf::from(first);
        if wrapper.as_ref().is_some_and(|existing| existing != &candidate) {
            return Ok(None);
        }
        wrapper = Some(candidate);
    }
    Ok((files > 0).then_some(wrapper).flatten())
}

fn verify_archive_digest(path: &FsPath, expected: &str) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|e| format!("فایل ZIP موقت پیدا نشد: {e}"))?;
    if metadata.len() > MAX_ARCHIVE_BYTES as u64 {
        return Err("آرشیو موقت از سقف اندازه بیشتر است.".into());
    }
    let bytes = fs::read(path).map_err(|e| format!("آرشیو موقت خوانده نشد: {e}"))?;
    if hex::encode(Sha256::digest(&bytes)) != expected {
        return Err("بررسی صحت آرشیو موقت شکست خورد؛ ZIP را دوباره بارگذاری کن.".into());
    }
    Ok(())
}

fn verify_release(path: &FsPath, healthcheck: Option<&str>) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("مسیر release وجود ندارد: {e}"))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("release یک پوشهٔ عادی نیست.".into());
    }
    if fs::read_dir(path)
        .map_err(|e| e.to_string())?
        .next()
        .is_none()
    {
        return Err("منبع خالی است و release فعال نشد.".into());
    }
    if let Some(relative) = healthcheck {
        if !valid_relative_file(relative) {
            return Err("Health check path ناامن است.".into());
        }
        let file = path.join(relative);
        let meta = fs::symlink_metadata(&file)
            .map_err(|_| format!("فایل Health check پیدا نشد: {relative}"))?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("Health check باید فایل معمولی باشد.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repo_urls_are_https_public_and_credential_free() {
        assert!(valid_repo("https://github.com/themrcortex/arg.git"));
        assert!(!valid_repo("http://github.com/a/b"));
        assert!(!valid_repo("https://user:secret@github.com/a/b"));
        assert!(!valid_repo("https://127.0.0.1/a/b"));
        assert!(!valid_repo("https://github.com/a/b?token=x"));
    }
    #[test]
    fn existing_git_deploy_sources_default_to_no_ssh_key() {
        let source: Source = serde_json::from_str(
            r#"{"kind":"git","repository":"https://github.com/acme/site.git","reference":"main"}"#,
        )
        .unwrap();
        assert!(matches!(source, Source::Git { ssh_key_id: None, .. }));
    }
    #[test]
    fn references_and_health_paths_reject_option_and_traversal() {
        assert!(valid_reference("release/v1.2"));
        assert!(!valid_reference("--upload-pack=x"));
        assert!(!valid_reference("../main"));
        assert!(valid_relative_file("dist/index.html"));
        assert!(!valid_relative_file("../../etc/passwd"));
        assert!(!valid_relative_file("/tmp/a"));
    }
    #[test]
    fn deployment_schema_is_additive_and_idempotent() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE applications(id TEXT PRIMARY KEY,owner_server_uuid TEXT NOT NULL);",
        )
        .unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        let tables:i64=db.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name LIKE 'deployment_%'",[],|r|r.get(0)).unwrap();
        assert_eq!(tables, 4);
    }

    #[test]
    fn legacy_partial_step_and_release_tables_are_migrated_before_use() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE applications(id TEXT PRIMARY KEY,owner_server_uuid TEXT NOT NULL); CREATE TABLE audit(at INTEGER,actor TEXT,action TEXT,detail TEXT); CREATE TABLE deployment_job_steps(id INTEGER PRIMARY KEY,job_id TEXT); CREATE TABLE deployment_releases(id TEXT PRIMARY KEY);").unwrap();
        migrate(&db).unwrap();
        db.execute("INSERT INTO applications(id,owner_server_uuid) VALUES('app','server')",[]).unwrap();
        persist_new_job(&mut db,"job","app","server","admin","{}","key",None,1,"git").unwrap();
        let message: String = db.query_row("SELECT message FROM deployment_job_steps WHERE job_id='job'", [], |row| row.get(0)).unwrap();
        assert_eq!(message,"در صف اجرای محدود سرور قرار گرفت.");
        let log_count: i64 = db.query_row("SELECT COUNT(*) FROM deployment_job_logs WHERE job_id='job'", [], |row| row.get(0)).unwrap();
        assert_eq!(log_count,1);
    }

    #[test]
    fn deployment_output_retains_a_bounded_recent_window() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE applications(id TEXT PRIMARY KEY,owner_server_uuid TEXT NOT NULL); CREATE TABLE audit(at INTEGER,actor TEXT,action TEXT,detail TEXT);").unwrap();
        migrate(&db).unwrap();
        db.execute("INSERT INTO applications(id,owner_server_uuid) VALUES('app','server')",[]).unwrap();
        persist_new_job(&mut db,"job","app","server","admin","{}","key",None,1,"git").unwrap();
        for line in 0..2_001 {
            append_job_log(&db,"job","build","output",&format!("line {line}")).unwrap();
        }
        let count: i64 = db.query_row("SELECT COUNT(*) FROM deployment_job_logs WHERE job_id='job'",[],|row|row.get(0)).unwrap();
        let first_message: String = db.query_row("SELECT message FROM deployment_job_logs WHERE job_id='job' ORDER BY id LIMIT 1",[],|row|row.get(0)).unwrap();
        let latest: String = db.query_row("SELECT message FROM deployment_job_logs WHERE job_id='job' ORDER BY id DESC LIMIT 1",[],|row|row.get(0)).unwrap();
        let job = read_job(&db,"job").unwrap();
        assert_eq!(count,2_000);
        assert_eq!(first_message,"line 2");
        assert_eq!(latest,"line 2000");
        assert_eq!(job["logs"].as_array().unwrap().len(),20);
        assert_eq!(job["log_count"],2_000);
    }

    #[test]
    fn enqueue_job_rolls_back_parent_when_initial_step_insert_fails() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE applications(id TEXT PRIMARY KEY,owner_server_uuid TEXT NOT NULL); CREATE TABLE audit(at INTEGER,actor TEXT,action TEXT,detail TEXT);").unwrap();
        migrate(&db).unwrap();
        db.execute("INSERT INTO applications(id,owner_server_uuid) VALUES('app','server')",[]).unwrap();
        db.execute_batch("CREATE TRIGGER reject_initial_step BEFORE INSERT ON deployment_job_steps BEGIN SELECT RAISE(ABORT,'step insert rejected'); END;").unwrap();
        let failed = persist_new_job(&mut db,"job","app","server","admin","{}","key",None,1,"archive");
        assert!(failed.is_err());
        let count: i64 = db.query_row("SELECT COUNT(*) FROM deployment_jobs WHERE id='job'",[],|row|row.get(0)).unwrap();
        assert_eq!(count,0,"a failed first-stage write must not leave a queued deployment");
        db.execute_batch("DROP TRIGGER reject_initial_step;").unwrap();
        let created = persist_new_job(&mut db,"job","app","server","admin","{}","key",None,2,"archive").unwrap();
        assert_eq!(created["state"],"queued");
        assert_eq!(created["steps"].as_array().unwrap().len(),1);
    }
    #[test]
    fn legacy_deployment_jobs_gain_runtime_columns_without_data_loss() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE applications(id TEXT PRIMARY KEY,owner_server_uuid TEXT NOT NULL);
             CREATE TABLE deployment_jobs (
                id TEXT PRIMARY KEY, application_id TEXT NOT NULL, owner_server_uuid TEXT NOT NULL,
                actor TEXT NOT NULL, state TEXT NOT NULL, source_json TEXT NOT NULL,
                idempotency_key TEXT NOT NULL, created_at INTEGER NOT NULL
             );
             INSERT INTO deployment_jobs(id,application_id,owner_server_uuid,actor,state,source_json,idempotency_key,created_at)
             VALUES ('old-job','app','server','admin','queued','{}','key',10);",
        ).unwrap();
        migrate(&db).unwrap();
        let columns = {
            let mut statement = db.prepare("PRAGMA table_info(deployment_jobs)").unwrap();
            statement.query_map([], |row| row.get::<_, String>(1)).unwrap()
                .collect::<Result<std::collections::HashSet<_>, _>>().unwrap()
        };
        for column in ["healthcheck_file", "attempt", "cancel_requested", "source_commit", "updated_at"] {
            assert!(columns.contains(column), "missing migrated column {column}");
        }
        let state: String = db.query_row("SELECT state FROM deployment_jobs WHERE id='old-job'", [], |row| row.get(0)).unwrap();
        assert_eq!(state, "queued");
        let step: String = db.query_row("SELECT step FROM deployment_job_steps WHERE job_id='old-job'", [], |row| row.get(0)).unwrap();
        assert_eq!(step,"recovered");
    }

    #[test]
    fn failed_runtime_activation_restores_only_a_release_inside_the_app_root() {
        let root = std::env::temp_dir().join(format!("arg-deploy-restore-{}", id()));
        let releases = root.join("releases");
        fs::create_dir_all(releases.join("old")).unwrap();
        fs::create_dir_all(releases.join("new")).unwrap();
        std::os::unix::fs::symlink("releases/new", root.join("current")).unwrap();
        restore_current_pointer(&root, Some("releases/old"), "job-safe").unwrap();
        assert_eq!(fs::read_link(root.join("current")).unwrap(), FsPath::new("releases/old"));

        std::os::unix::fs::symlink("releases/new", root.join("current")).unwrap_or_else(|_| {
            fs::remove_file(root.join("current")).unwrap();
            std::os::unix::fs::symlink("releases/new", root.join("current")).unwrap();
        });
        assert!(restore_current_pointer(&root, Some("../../etc"), "job-unsafe").is_err());
        assert_eq!(fs::read_link(root.join("current")).unwrap(), FsPath::new("releases/new"));
        restore_current_pointer(&root, None, "job-first").unwrap();
        assert!(!root.join("current").exists());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn zip_extraction_rejects_traversal_and_tree_copy_excludes_secrets() {
        let root = std::env::temp_dir().join(format!("arg-deploy-test-{}", id()));
        let src = root.join("src");
        let dst = root.join("dst");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::write(src.join("index.html"), "ok").unwrap();
        fs::write(src.join(".env"), "secret").unwrap();
        copy_tree(&src, &dst).unwrap();
        assert!(dst.join("index.html").is_file());
        assert!(!dst.join(".env").exists());
        let archive_path = root.join("bad.zip");
        let file = fs::File::create(&archive_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("../../escape.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"bad").unwrap();
        zip.finish().unwrap();
        let result = extract_archive(&archive_path, &root.join("outside"));
        assert!(result.unwrap_err().contains("traversal"));
        assert!(!root.parent().unwrap().join("escape.txt").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn zip_with_single_project_directory_extracts_manifest_at_release_root() {
        let root = std::env::temp_dir().join(format!("arg-deploy-wrapped-zip-{}", id()));
        fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("project.zip");
        let file = fs::File::create(&archive_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.add_directory("arg-nodejs-test-site/", options).unwrap();
        zip.start_file("arg-nodejs-test-site/package.json", options).unwrap();
        zip.write_all(br#"{"name":"arg-nodejs-test-site","scripts":{"build":"node scripts/build.mjs","start":"node server.mjs"}}"#).unwrap();
        zip.start_file("arg-nodejs-test-site/package-lock.json", options).unwrap();
        zip.write_all(br#"{"name":"arg-nodejs-test-site","lockfileVersion":3}"#).unwrap();
        zip.start_file("arg-nodejs-test-site/.env.example", options).unwrap();
        zip.write_all(b"SHOULD_NOT_SHIP=1").unwrap();
        zip.start_file("__MACOSX/arg-nodejs-test-site/._package.json", options).unwrap();
        zip.write_all(b"macOS metadata").unwrap();
        zip.start_file("arg-nodejs-test-site/server.mjs", options).unwrap();
        zip.write_all(b"export {};").unwrap();
        zip.finish().unwrap();

        let release = root.join("release");
        fs::create_dir_all(&release).unwrap();
        extract_archive(&archive_path, &release).unwrap();
        assert!(release.join("package.json").is_file());
        assert!(release.join("package-lock.json").is_file());
        assert!(release.join("server.mjs").is_file());
        assert!(!release.join("arg-nodejs-test-site/package.json").exists());
        assert!(!release.join(".env.example").exists());
        assert!(!release.join("__MACOSX").exists());
        let manifest = serde_json::from_slice::<Value>(&fs::read(release.join("package.json")).unwrap()).unwrap();
        assert_eq!(manifest["scripts"]["start"],"node server.mjs");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn git_worktree_size_guard_counts_regular_files_and_skips_symlinks() {
        let root = std::env::temp_dir().join(format!("arg-deploy-size-{}", id()));
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested/payload"), [0u8; 32]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/dev/zero", root.join("outside")).unwrap();

        assert_eq!(bounded_tree_size(&root, 64).unwrap(), 32);
        assert!(bounded_tree_size(&root, 16).unwrap() > 16);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_requeues_interrupted_job_and_marks_running_step() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE applications(id TEXT PRIMARY KEY,owner_server_uuid TEXT NOT NULL);",
        )
        .unwrap();
        migrate(&db).unwrap();
        let at = now();
        db.execute(
            "INSERT INTO applications(id,owner_server_uuid) VALUES('app','server')",
            [],
        )
        .unwrap();
        db.execute("INSERT INTO deployment_jobs(id,application_id,owner_server_uuid,actor,state,source_json,idempotency_key,created_at,updated_at) VALUES('job','app','server','admin','running','{}','key',?1,?1)",[at]).unwrap();
        db.execute("INSERT INTO deployment_job_steps(job_id,sequence,step,state,message,started_at) VALUES('job',1,'acquiring','running','fetching',?1)",[at]).unwrap();
        let state = Arc::new(Mutex::new(db));
        recover_interrupted(&state);
        let db = state.lock().unwrap();
        let status: String = db
            .query_row(
                "SELECT state FROM deployment_jobs WHERE id='job'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let step: String = db
            .query_row(
                "SELECT state FROM deployment_job_steps WHERE job_id='job'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "queued");
        assert_eq!(step, "interrupted");
    }

    #[test]
    fn startup_recognizes_atomic_activation_before_process_crash() {
        let root = std::env::temp_dir().join(format!("arg-deploy-recover-{}", id()));
        let app_root = root.join("apps/app");
        let release = app_root.join("releases/job");
        fs::create_dir_all(&release).unwrap();
        fs::write(release.join("index.html"), "ready").unwrap();
        std::os::unix::fs::symlink(FsPath::new("releases/job"), app_root.join("current")).unwrap();
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE applications(id TEXT PRIMARY KEY,owner_server_uuid TEXT NOT NULL);",
        )
        .unwrap();
        migrate(&db).unwrap();
        let at = now();
        db.execute(
            "INSERT INTO applications(id,owner_server_uuid) VALUES('app','server')",
            [],
        )
        .unwrap();
        db.execute("INSERT INTO deployment_jobs(id,application_id,owner_server_uuid,actor,state,source_json,idempotency_key,created_at,updated_at) VALUES('job','app','server','admin','verifying','{}','key',?1,?1)",[at]).unwrap();
        let state = Arc::new(Mutex::new(db));
        recover_interrupted_at(&state, &root);
        let db = state.lock().unwrap();
        let status: String = db
            .query_row(
                "SELECT state FROM deployment_jobs WHERE id='job'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let releases: i64 = db
            .query_row(
                "SELECT count(*) FROM deployment_releases WHERE id='job' AND state='active'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "succeeded");
        assert_eq!(releases, 1);
        drop(db);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn job_enqueue_is_admin_only_idempotent_and_serialized_per_application() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE applications(id TEXT PRIMARY KEY,owner_server_uuid TEXT NOT NULL,deleted_at INTEGER); CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE users(id INTEGER PRIMARY KEY,username TEXT NOT NULL,role TEXT NOT NULL,enabled INTEGER NOT NULL); CREATE TABLE sessions(token_hash TEXT PRIMARY KEY,user_id INTEGER NOT NULL,expires_at INTEGER NOT NULL,last_seen INTEGER); CREATE TABLE audit(id INTEGER PRIMARY KEY,at INTEGER NOT NULL,actor TEXT NOT NULL,action TEXT NOT NULL,detail TEXT NOT NULL);").unwrap();
        migrate(&db).unwrap();
        let admin_token = "deploy-admin-session";
        let viewer_token = "deploy-viewer-session";
        let at = now();
        db.execute(
            "INSERT INTO applications(id,owner_server_uuid) VALUES('app','server')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO settings(key,value) VALUES('federation_server_uuid','server')",
            [],
        )
        .unwrap();
        db.execute("INSERT INTO users(id,username,role,enabled) VALUES(1,'admin','admin',1),(2,'viewer','viewer',1)",[]).unwrap();
        db.execute(
            "INSERT INTO sessions(token_hash,user_id,expires_at) VALUES(?1,1,?3),(?2,2,?3)",
            params![
                session_hash(admin_token),
                session_hash(viewer_token),
                at + 3600
            ],
        )
        .unwrap();
        let state = Arc::new(Mutex::new(db));
        let headers_for = |token: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::HOST, "arg.example".parse().unwrap());
            headers.insert(header::ORIGIN, "https://arg.example".parse().unwrap());
            headers.insert(
                header::COOKIE,
                format!("arg_session={token}").parse().unwrap(),
            );
            headers
        };
        let source = Source::Directory {
            path: "/var/www/demo".into(),
        };
        let first = insert_job(
            &state,
            &headers_for(admin_token),
            "app",
            source.clone(),
            "stable-key".into(),
            None,
            None,
            None,
        )
        .unwrap();
        let second = insert_job(
            &state,
            &headers_for(admin_token),
            "app",
            source.clone(),
            "stable-key".into(),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(first["id"], second["id"]);
        let conflict = insert_job(
            &state,
            &headers_for(admin_token),
            "app",
            Source::Git {
                repository: "https://github.com/acme/site".into(),
                reference: "main".into(),
                ssh_key_id: None,
            },
            "stable-key".into(),
            None,
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(conflict.0, StatusCode::CONFLICT);
        let concurrent = insert_job(
            &state,
            &headers_for(admin_token),
            "app",
            source.clone(),
            "second-key".into(),
            None,
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(concurrent.0, StatusCode::CONFLICT);
        let unauthorized = insert_job(
            &state,
            &headers_for(viewer_token),
            "app",
            source,
            "viewer-key".into(),
            None,
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(unauthorized.0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn release_retention_keeps_five_recent_and_active_previous() {
        let root = std::env::temp_dir().join(format!("arg-release-retention-{}", id()));
        let app_root = root.join("apps/app");
        let release_root = app_root.join("releases");
        fs::create_dir_all(&release_root).unwrap();
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE applications(id TEXT PRIMARY KEY,owner_server_uuid TEXT NOT NULL);",
        )
        .unwrap();
        migrate(&db).unwrap();
        db.execute(
            "INSERT INTO applications(id,owner_server_uuid) VALUES('app','server')",
            [],
        )
        .unwrap();
        let mut ids = Vec::new();
        for index in 0..8 {
            let job = id();
            ids.push(job.clone());
            let release = release_root.join(&job);
            fs::create_dir_all(&release).unwrap();
            fs::write(release.join("index.html"), "ok").unwrap();
            db.execute("INSERT INTO deployment_jobs(id,application_id,owner_server_uuid,actor,state,source_json,idempotency_key,created_at,updated_at) VALUES(?1,'app','server','admin','succeeded','{}',?1,?2,?2)",params![job,index+1]).unwrap();
            let status = if index == 7 {
                "active"
            } else if index == 6 {
                "previous"
            } else {
                "retained"
            };
            db.execute("INSERT INTO deployment_releases(id,application_id,job_id,path,state,created_at) VALUES(?1,'app',?1,?2,?3,?4)",params![job,release.to_string_lossy(),status,index+1]).unwrap();
        }
        std::os::unix::fs::symlink(
            FsPath::new("releases").join(ids[7].as_str()),
            app_root.join("current"),
        )
        .unwrap();
        let state = Arc::new(Mutex::new(db));
        prune_releases_at(&state, &root, "app");
        let db = state.lock().unwrap();
        let expired: i64 = db
            .query_row(
                "SELECT count(*) FROM deployment_releases WHERE state='expired'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(expired, 3);
        assert!(!release_root.join(&ids[0]).exists());
        assert!(!release_root.join(&ids[1]).exists());
        assert!(release_root.join(&ids[3]).exists());
        assert!(release_root.join(&ids[6]).exists());
        assert!(release_root.join(&ids[7]).exists());
        drop(db);
        let _ = fs::remove_dir_all(root);
    }
}
