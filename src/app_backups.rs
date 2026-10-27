use super::*;
use axum::{body::Body, http::header::CONTENT_DISPOSITION};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Component, Path as FsPath, PathBuf},
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const MAX_ENTRIES: usize = 20_000;
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_APP_BACKUP_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_APP_ARCHIVES: usize = 50;
static APP_BACKUP_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateInput {
    confirmed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeleteInput {
    confirmed: bool,
}

#[derive(Clone)]
struct BackupSource {
    release: PathBuf,
    release_id: String,
    commit: Option<String>,
}

fn excluded(relative: &FsPath) -> bool {
    relative.components().any(|component| match component {
        Component::Normal(name) => {
            let name = name.to_string_lossy().to_ascii_lowercase();
            name.starts_with('.')
                || matches!(
                    name.as_str(),
                    "node_modules"
                        | "vendor"
                        | "storage"
                        | "secrets"
                        | "credentials"
                        | "id_rsa"
                        | "id_ed25519"
                )
                || [
                    ".pem", ".key", ".p12", ".pfx", ".sqlite", ".sqlite3", ".db", ".dump", ".sql",
                    ".log", ".bak", ".backup",
                ]
                .iter()
                .any(|suffix| name.ends_with(suffix))
        }
        _ => true,
    })
}

fn valid_archive_id(value: &str) -> bool {
    value
        .strip_suffix(".zip")
        .is_some_and(|stem| stem.len() == 32 && stem.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn backup_root(app_id: &str) -> PathBuf {
    deploy::deployment_root()
        .join("backups")
        .join("apps")
        .join(app_id)
}

fn collect_files(root: &FsPath) -> Result<Vec<PathBuf>, String> {
    fn visit(root: &FsPath, directory: &FsPath, files: &mut Vec<PathBuf>) -> Result<(), String> {
        let mut entries = fs::read_dir(directory)
            .map_err(|_| "فهرست release برای پشتیبان خوانده نشد.".to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "یکی از ورودی‌های release خوانده نشد.".to_string())?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| "مسیر فایل از release خارج شد.".to_string())?
                .to_path_buf();
            if excluded(&relative) {
                continue;
            }
            let kind = entry
                .file_type()
                .map_err(|_| "نوع یکی از فایل‌های release خوانده نشد.".to_string())?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                visit(root, &entry.path(), files)?;
            } else if kind.is_file() {
                files.push(relative);
                if files.len() > MAX_ENTRIES {
                    return Err("تعداد فایل‌های کد از سقف پشتیبان‌گیری عبور کرد.".into());
                }
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    Ok(files)
}

fn archive_usage(directory: &PathBuf) -> (usize, u64) {
    fs::read_dir(directory)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            if !valid_archive_id(name) {
                return None;
            }
            let metadata = fs::symlink_metadata(entry.path()).ok()?;
            (metadata.file_type().is_file() && !metadata.file_type().is_symlink())
                .then_some(metadata.len())
        })
        .fold((0usize, 0u64), |(count, bytes), size| {
            (count.saturating_add(1), bytes.saturating_add(size))
        })
}

fn create_archive(
    app_id: &str,
    source: BackupSource,
    max_archive_bytes: u64,
) -> Result<(String, u64, String), String> {
    let root = source
        .release
        .canonicalize()
        .map_err(|_| "release فعال روی دیسک پیدا نشد.".to_string())?;
    let expected_root = deploy::deployment_root()
        .join("apps")
        .join(app_id)
        .canonicalize()
        .map_err(|_| "پوشهٔ مالکیت اپلیکیشن پیدا نشد.".to_string())?;
    if !root.starts_with(expected_root.join("releases")) {
        return Err("مسیر release خارج از محدودهٔ اپلیکیشن است.".into());
    }
    let files = collect_files(&root)?;
    let mut total = 0u64;
    for relative in &files {
        let metadata = fs::symlink_metadata(root.join(relative))
            .map_err(|_| "بررسی اندازهٔ یکی از فایل‌ها ناموفق بود.".to_string())?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.len() > MAX_FILE_BYTES {
            return Err(format!(
                "فایل {} از سقف ۳۲ MiB پشتیبان کد بزرگ‌تر است.",
                relative.display()
            ));
        }
        total = total.saturating_add(metadata.len());
        if total > MAX_TOTAL_BYTES {
            return Err("حجم فایل‌های کد از سقف ۲۵۶ MiB عبور کرد.".into());
        }
    }
    let destination_dir = backup_root(app_id);
    fs::create_dir_all(&destination_dir).map_err(|_| "پوشهٔ آرشیو اپ ساخته نشد.".to_string())?;
    let id = format!("{:032x}.zip", rand::random::<u128>());
    let destination = destination_dir.join(&id);
    let temp = destination_dir.join(format!(".{}.tmp", &id[..32]));
    let mut temp_cleanup = TempCleanup(Some(temp.clone()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .mode(0o600)
        .open(&temp)
        .map_err(|_| "فایل موقت پشتیبان ساخته نشد.".to_string())?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o600);
    let mut manifest_entries = Vec::new();
    let mut actual_total = 0u64;
    for relative in &files {
        let input = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(root.join(relative))
            .map_err(|_| format!("خواندن فایل {} ناموفق بود.", relative.display()))?;
        let mut bytes = Vec::new();
        input
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| format!("خواندن فایل {} ناموفق بود.", relative.display()))?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(format!("فایل {} بیش از حد بزرگ شد.", relative.display()));
        }
        actual_total = actual_total.saturating_add(bytes.len() as u64);
        if actual_total > MAX_TOTAL_BYTES {
            return Err("حجم فایل‌های کد هنگام خواندن از سقف ۲۵۶ MiB عبور کرد.".into());
        }
        let path = relative.to_string_lossy().replace('\\', "/");
        writer
            .start_file(&path, options)
            .map_err(|_| "ساخت ورودی ZIP ناموفق بود.".to_string())?;
        writer
            .write_all(&bytes)
            .map_err(|_| "نوشتن داده در ZIP ناموفق بود.".to_string())?;
        manifest_entries.push(json!({"path":path,"size_bytes":bytes.len(),"sha256":hex::encode(Sha256::digest(&bytes))}));
    }
    let manifest = json!({
        "format":"arg-app-source-backup-v1",
        "application_id":app_id,
        "release_id":source.release_id,
        "source_commit":source.commit,
        "created_at":now(),
        "files":manifest_entries,
        "known_sensitive_paths_excluded":true,
        "secret_values_scanned":false,
        "database_and_runtime_paths_excluded":true,
        "restorable":false
    });
    writer
        .start_file("ARG-MANIFEST.json", options)
        .map_err(|_| "ثبت manifest پشتیبان ناموفق بود.".to_string())?;
    writer
        .write_all(manifest.to_string().as_bytes())
        .map_err(|_| "ثبت manifest پشتیبان ناموفق بود.".to_string())?;
    let file = writer
        .finish()
        .map_err(|_| "بستن آرشیو پشتیبان ناموفق بود.".to_string())?;
    file.sync_all()
        .map_err(|_| "پایدارکردن آرشیو پشتیبان ناموفق بود.".to_string())?;
    let size = file
        .metadata()
        .map_err(|_| "اندازهٔ آرشیو خوانده نشد.".to_string())?
        .len();
    if size > max_archive_bytes {
        return Err("سقف باقی‌ماندهٔ فضای آرشیوهای این اپ برای فایل جدید کافی نیست.".into());
    }
    drop(file);
    fs::rename(&temp, &destination).map_err(|_| {
        let _ = fs::remove_file(&temp);
        "ثبت آرشیو پشتیبان ناموفق بود.".to_string()
    })?;
    temp_cleanup.0 = None;
    let size = fs::metadata(&destination)
        .map(|metadata| metadata.len())
        .unwrap_or(size);
    let mut hash = Sha256::new();
    let mut reader =
        File::open(&destination).map_err(|_| "بررسی checksum آرشیو ناموفق بود.".to_string())?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|_| "خواندن checksum آرشیو ناموفق بود.".to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let digest = hex::encode(hash.finalize());
    Ok((id, size, digest))
}

struct TempCleanup(Option<PathBuf>);

impl Drop for TempCleanup {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_file(path);
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
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2)",
            params![app_id, owner],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if !exists || !apps::valid_uuid(&app_id) {
        return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."));
    }
    let directory = backup_root(&app_id);
    let mut entries = Vec::new();
    if let Ok(read_dir) = fs::read_dir(directory) {
        for item in read_dir.flatten() {
            let Some(name) = item.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Ok(file_type) = item.file_type() else {
                continue;
            };
            if !valid_archive_id(&name) || !file_type.is_file() || file_type.is_symlink() {
                continue;
            }
            if let Ok(meta) = item.metadata() {
                if meta.is_file() {
                    entries.push(json!({"id":name,"size_bytes":meta.len()}));
                }
            }
        }
    }
    entries.sort_by(|a, b| b["id"].as_str().cmp(&a["id"].as_str()));
    let (archive_count, stored_bytes) = archive_usage(&backup_root(&app_id));
    Ok(Json(
        json!({"archives":entries,"archive_count":archive_count,"stored_bytes":stored_bytes,"maximum_archives":MAX_APP_ARCHIVES,"maximum_storage_bytes":MAX_APP_BACKUP_BYTES,"backup_scope":"active source code only","restore_available":false}),
    ))
}

pub(super) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<CreateInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !input.confirmed {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "ساخت پشتیبان نیازمند تأیید صریح است.",
        ));
    }
    let _backup_guard = APP_BACKUP_LOCK.lock().await;
    let (actor, source, max_archive_bytes) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let source = db.query_row("SELECT r.id,r.source_commit,r.path FROM deployment_releases r JOIN applications a ON a.id=r.application_id WHERE r.application_id=?1 AND a.owner_server_uuid=?2 AND a.deleted_at IS NULL AND r.state='active' ORDER BY r.activated_at DESC LIMIT 1",params![app_id,owner],|row|Ok(BackupSource{release:PathBuf::from(row.get::<_,String>(2)?),release_id:row.get(0)?,commit:row.get(1)?})).optional().map_err(db_error)?.ok_or_else(||error(StatusCode::NOT_FOUND,"release فعال پیدا نشد."))?;
        if !apps::valid_uuid(&app_id) {
            return Err(error(StatusCode::BAD_REQUEST, "شناسهٔ اپلیکیشن معتبر نیست."));
        }
        let (count, stored_bytes) = archive_usage(&backup_root(&app_id));
        if count >= MAX_APP_ARCHIVES {
            return Err(error(
                StatusCode::CONFLICT,
                "برای این اپ ۵۰ آرشیو نگه داشته شده؛ ابتدا پشتیبان‌های قبلی را بررسی کن.",
            ));
        }
        if stored_bytes >= MAX_APP_BACKUP_BYTES {
            return Err(error(
                StatusCode::INSUFFICIENT_STORAGE,
                "سقف ۱ GiB آرشیوهای محلی این اپ پر شده است؛ ابتدا یک فایل قدیمی را حذف کن.",
            ));
        }
        (actor, source, MAX_APP_BACKUP_BYTES - stored_bytes)
    };
    let task_app = app_id.clone();
    let (id, size, digest) =
        tokio::task::spawn_blocking(move || create_archive(&task_app, source, max_archive_bytes))
            .await
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "وظیفهٔ پشتیبان متوقف شد."))?
            .map_err(|message| error(StatusCode::BAD_REQUEST, &message))?;
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor,
        "app.backup.create",
        &format!("app={app_id}; archive={id}; sha256={digest}"),
    );
    Ok(Json(
        json!({"id":id,"application_id":app_id,"size_bytes":size,"sha256":digest,"checksum_computed":true,"scope":"active source code only","known_sensitive_paths_excluded":true,"secret_values_scanned":false,"database_and_runtime_paths_excluded":true,"restorable":false}),
    ))
}

pub(super) async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((app_id, id)): Path<(String, String)>,
    Json(input): Json<DeleteInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !input.confirmed {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "حذف آرشیو نیازمند تأیید صریح است.",
        ));
    }
    let _backup_guard = APP_BACKUP_LOCK.lock().await;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2)",
            params![app_id, owner],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if !exists || !apps::valid_uuid(&app_id) || !valid_archive_id(&id) {
        return Err(error(StatusCode::NOT_FOUND, "آرشیو پشتیبان پیدا نشد."));
    }
    let path = backup_root(&app_id).join(&id);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| error(StatusCode::NOT_FOUND, "آرشیو پشتیبان پیدا نشد."))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(error(StatusCode::NOT_FOUND, "آرشیو پشتیبان پیدا نشد."));
    }
    fs::remove_file(&path).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "حذف آرشیو از دیسک ناموفق بود.",
        )
    })?;
    audit(
        &db,
        &actor,
        "app.backup.delete",
        &format!("app={app_id}; archive={id}; size_bytes={}", metadata.len()),
    );
    Ok(Json(json!({"deleted":true,"size_bytes":metadata.len()})))
}

pub(super) async fn download(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((app_id, id)): Path<(String, String)>,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2)",
            params![app_id, owner],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if !exists || !apps::valid_uuid(&app_id) || !valid_archive_id(&id) {
        return Err(error(StatusCode::NOT_FOUND, "آرشیو پشتیبان پیدا نشد."));
    }
    let path = backup_root(&app_id).join(&id);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| error(StatusCode::NOT_FOUND, "آرشیو پشتیبان پیدا نشد."))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(error(StatusCode::NOT_FOUND, "آرشیو پشتیبان پیدا نشد."));
    }
    drop(db);
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "خواندن آرشیو پشتیبان ناموفق بود.",
            )
        })?;
    let file = tokio::fs::File::from_std(file);
    let mut response = Response::new(Body::from_stream(tokio_util::io::ReaderStream::new(file)));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{id}\"")).unwrap(),
    );
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&metadata.len().to_string()).unwrap(),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excludes_secrets_and_runtime_data() {
        assert!(excluded(FsPath::new(".env")));
        assert!(excluded(FsPath::new("config/.env.production")));
        assert!(excluded(FsPath::new("storage/app/data.sqlite")));
        assert!(excluded(FsPath::new("exports/customer.sql")));
        assert!(excluded(FsPath::new("tls/site.key")));
        assert!(!excluded(FsPath::new("src/main.rs")));
    }
    #[test]
    fn archive_ids_are_fixed_hex_and_not_paths() {
        assert!(valid_archive_id("0123456789abcdef0123456789abcdef.zip"));
        assert!(!valid_archive_id("../0123456789abcdef0123456789abcdef.zip"));
    }
}
