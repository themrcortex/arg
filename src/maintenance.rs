use super::*;
use axum::{body::Body, http::header::CONTENT_DISPOSITION};
use futures_util::StreamExt;
use std::path::{Path as FsPath, PathBuf};

const MAX_BACKUP_UPLOAD_BYTES: u64 = 512 * 1024 * 1024;

struct PendingBackup(PathBuf);
impl Drop for PendingBackup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn db_path() -> PathBuf {
    env::var_os("ARG_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data/arg.db"))
}
fn backup_dir() -> PathBuf {
    db_path()
        .parent()
        .unwrap_or(FsPath::new("."))
        .join("backups")
}
pub(super) async fn system_info(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let os = metrics::os_pretty_name();
    let cpu = metrics::cpu_model_name().unwrap_or_else(|| "—".to_owned());
    let kernel = std::process::Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_else(|| "—".to_owned());
    Ok(Json(
        json!({"os":os,"kernel":kernel,"cpu":cpu,"architecture":std::env::consts::ARCH,"hostname":fs::read_to_string("/etc/hostname").unwrap_or_default().trim()}),
    ))
}
fn valid_backup_id(id: &str) -> bool {
    id.starts_with("arg-")
        && id.ends_with(".db")
        && id.len() <= 96
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}
fn backup_file(id: &str) -> Result<PathBuf, (StatusCode, Json<Value>)> {
    if !valid_backup_id(id) {
        return Err(error(StatusCode::BAD_REQUEST, "Invalid backup id"));
    }
    let path = backup_dir().join(id);
    if !path.is_file() {
        return Err(error(StatusCode::NOT_FOUND, "Backup not found"));
    }
    Ok(path)
}

fn validate_backup(path: &FsPath) -> Result<(), &'static str> {
    let candidate = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| "File is not a valid ARG database backup")?;
    let integrity: String = candidate
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|_| "Could not check uploaded backup integrity")?;
    if integrity != "ok" {
        return Err("Backup failed the SQLite integrity check");
    }
    let has_users: bool = candidate
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='users')",
            [],
            |row| row.get(0),
        )
        .map_err(|_| "File does not contain ARG account data")?;
    if !has_users {
        return Err("File does not contain ARG account data");
    }
    Ok(())
}

pub(super) async fn list_backups(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let dir = backup_dir();
    let mut backups = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !valid_backup_id(name) {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                backups.push(json!({"id":name,"size_bytes":meta.len(),"modified_at":meta.modified().ok().and_then(|t|t.duration_since(UNIX_EPOCH).ok()).map(|d|d.as_secs())}));
            }
        }
    }
    backups.sort_by(|a, b| b["id"].as_str().cmp(&a["id"].as_str()));
    Ok(Json(json!({"backups":backups})))
}

pub(super) async fn create_backup(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let dir = backup_dir();
    fs::create_dir_all(&dir).map_err(|e| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Could not create backup directory: {e}"),
        )
    })?;
    let id = format!("arg-{}-{}.db", now(), &token()[..8]);
    let destination = dir.join(&id);
    backup_database(&db_path(), &destination).map_err(|e| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Could not create database backup: {e}"),
        )
    })?;
    audit(&db, &actor.username, "maintenance.backup.create", &id);
    let size = fs::metadata(&destination).map(|m| m.len()).unwrap_or(0);
    Ok(Json(
        json!({"id":id,"size_bytes":size,"message":"ARG database backup created."}),
    ))
}

pub(super) async fn upload_backup(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    let dir = backup_dir();
    fs::create_dir_all(&dir).map_err(|e| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Could not create backup directory: {e}"),
        )
    })?;
    let temporary = dir.join(format!(".arg-upload-{}.tmp", &token()[..24]));
    let pending = PendingBackup(temporary.clone());
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|e| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("Could not create temporary backup: {e}"),
                )
            })?
    };
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::from_std(file);
    let mut stream = body.into_data_stream();
    let mut size = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| error(StatusCode::BAD_REQUEST, "Backup upload was interrupted"))?;
        size = size.saturating_add(chunk.len() as u64);
        if size > MAX_BACKUP_UPLOAD_BYTES {
            return Err(error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Backup files may not exceed 512 MiB.",
            ));
        }
        file.write_all(&chunk)
            .await
            .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Could not save backup: {e}")))?;
    }
    file.flush()
        .await
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Could not save backup: {e}")))?;
    file.sync_all()
        .await
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Could not sync backup: {e}")))?;
    drop(file);

    if size < 100 {
        return Err(error(StatusCode::BAD_REQUEST, "Uploaded file is too small to be an ARG database backup."));
    }
    let mut header = [0u8; 16];
    std::io::Read::read_exact(&mut fs::File::open(&temporary).map_err(|e| {
        error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Could not inspect uploaded backup: {e}"))
    })?, &mut header)
    .map_err(|_| error(StatusCode::BAD_REQUEST, "Uploaded file is not a valid SQLite database."))?;
    if &header != b"SQLite format 3\0" {
        return Err(error(StatusCode::BAD_REQUEST, "Uploaded file is not a valid SQLite database."));
    }
    let validation_path = temporary.clone();
    tokio::task::spawn_blocking(move || validate_backup(&validation_path))
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Backup validation task failed"))?
        .map_err(|message| error(StatusCode::BAD_REQUEST, message))?;

    let id = format!("arg-{}-{}.db", now(), &token()[..8]);
    let destination = dir.join(&id);
    fs::rename(&temporary, &destination).map_err(|e| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Could not register uploaded backup: {e}"),
        )
    })?;
    let db = state.lock().unwrap();
    audit(&db, &actor, "maintenance.backup.upload", &id);
    drop(pending);
    Ok(Json(json!({"id":id,"size_bytes":size,"message":"ARG backup uploaded and verified."})))
}

pub(super) async fn download_backup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let path = backup_file(&id)?;
    let bytes = fs::read(path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Could not read backup"))?;
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/vnd.sqlite3"),
    );
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{id}\"")).unwrap(),
    );
    Ok(response)
}

pub(super) async fn restore_backup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Value>,
) -> ApiResult {
    check_origin(&headers)?;
    if input["confirmation"].as_str() != Some("RESTORE ARG") {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Type RESTORE ARG to confirm",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let source = backup_file(&id)?;
    validate_backup(&source).map_err(|message| error(StatusCode::BAD_REQUEST, message))?;
    let current = db_path();
    let rollback = backup_dir().join(format!("arg-before-restore-{}-{}.db", now(), &token()[..8]));
    backup_database(&current, &rollback).map_err(|e| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Could not protect current database: {e}"),
        )
    })?;
    if let Err(e) = backup_database(&source, &current) {
        let _ = backup_database(&rollback, &current);
        let _ = fs::remove_file(&rollback);
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Restore failed; current data was preserved: {e}"),
        ));
    }
    audit(&db, &actor.username, "maintenance.backup.restore", &id);
    Ok(Json(
        json!({"restored":true,"message":"Backup restored. Reloading ARG service is required for all current sessions to use restored data."}),
    ))
}

#[derive(Deserialize)]
pub(super) struct ActionInput {
    action: String,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    confirmation: String,
}
pub(super) async fn run_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ActionInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let (action, branch) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?;
        let expected = match input.action.as_str() {
            "repair" => "REPAIR ARG",
            "reboot" => "REBOOT SERVER",
            "uninstall" => "DELETE ARG",
            _ => {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "Unsupported maintenance action",
                ))
            }
        };
        if input.confirmation != expected {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "Confirmation phrase did not match",
            ));
        }
        if input.action == "repair"
            && input
                .branch
                .as_deref()
                .is_some_and(|branch| !valid_update_branch(branch))
        {
            return Err(error(StatusCode::BAD_REQUEST, "Invalid ARG repair branch"));
        }
        let action = match input.action.as_str() {
            "repair" => "maintenance-repair",
            "reboot" => "maintenance-reboot",
            _ => "maintenance-uninstall",
        };
        audit(
            &db,
            &actor.username,
            &format!("maintenance.{}", input.action),
            "administrator confirmed from the maintenance page",
        );
        (
            action.to_owned(),
            input.branch.unwrap_or_else(|| "main".to_owned()),
        )
    };
    let output = tokio::task::spawn_blocking(move || {
        privileged_request(json!({"action":action,"branch":branch}))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Maintenance task failed"))??;
    if !output["ok"].as_bool().unwrap_or(false) {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            output["error"]
                .as_str()
                .unwrap_or("Maintenance action failed"),
        ));
    }
    Ok(Json(output))
}

#[cfg(test)]
mod tests {
    use super::{valid_backup_id, validate_backup};

    #[test]
    fn backup_ids_reject_paths_and_unrelated_files() {
        assert!(valid_backup_id("arg-1790000000-acde1234.db"));
        assert!(!valid_backup_id("../arg-secret.db"));
        assert!(!valid_backup_id("arg-backup.tar.gz"));
        assert!(!valid_backup_id("arg-../../etc/passwd.db"));
    }

    #[test]
    fn uploaded_backup_must_be_integral_sqlite_with_arg_users_table() {
        let valid = std::env::temp_dir().join(format!("arg-backup-test-{}.db", super::now()));
        let _ = std::fs::remove_file(&valid);
        let db = rusqlite::Connection::open(&valid).unwrap();
        db.execute_batch("CREATE TABLE users (id INTEGER PRIMARY KEY, username TEXT NOT NULL);")
            .unwrap();
        drop(db);
        assert_eq!(validate_backup(&valid), Ok(()));
        std::fs::remove_file(&valid).unwrap();

        let invalid = std::env::temp_dir().join(format!("arg-backup-invalid-{}.db", super::now()));
        std::fs::write(&invalid, b"not a sqlite backup").unwrap();
        assert!(validate_backup(&invalid).is_err());
        std::fs::remove_file(invalid).unwrap();
    }
}
