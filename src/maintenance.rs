use super::*;
use axum::{body::Body, http::header::CONTENT_DISPOSITION};
use futures_util::StreamExt;
use std::path::{Path as FsPath, PathBuf};

const MAX_BACKUP_UPLOAD_BYTES: u64 = 512 * 1024 * 1024;
const BACKUP_INTERVAL_MIN_HOURS: i64 = 1;
const BACKUP_INTERVAL_MAX_HOURS: i64 = 720;
const BACKUP_RETENTION_MIN: i64 = 1;
const BACKUP_RETENTION_MAX: i64 = 100;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BackupPolicyInput {
    enabled: bool,
    interval_hours: i64,
    retention_count: i64,
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS maintenance_backup_policy(id INTEGER PRIMARY KEY CHECK(id=1),enabled INTEGER NOT NULL DEFAULT 0,interval_hours INTEGER NOT NULL DEFAULT 24,retention_count INTEGER NOT NULL DEFAULT 7,last_started_at INTEGER,last_success_at INTEGER,last_error TEXT,next_run_at INTEGER,updated_at INTEGER NOT NULL); INSERT OR IGNORE INTO maintenance_backup_policy(id,enabled,interval_hours,retention_count,updated_at) VALUES(1,0,24,7,0);")
}

pub(super) fn start_backup_scheduler(state: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            ticker.tick().await;
            if let Err(problem) = run_scheduled_backup(&state).await {
                eprintln!("ARG automatic backup scheduler: {problem}");
            }
        }
    });
}

async fn run_scheduled_backup(state: &AppState) -> Result<(), String> {
    let due = {
        let db = state.lock().map_err(|_| "Database lock is unavailable".to_owned())?;
        let (enabled, interval_hours, retention_count, next_run_at): (bool, i64, i64, Option<i64>) = db
            .query_row("SELECT enabled,interval_hours,retention_count,next_run_at FROM maintenance_backup_policy WHERE id=1", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))
            .map_err(|problem| problem.to_string())?;
        let current_time = now();
        if !enabled || next_run_at.is_some_and(|next| next > current_time) {
            None
        } else {
            let next = current_time.saturating_add(interval_hours.saturating_mul(3600));
            db.execute("UPDATE maintenance_backup_policy SET last_started_at=?1,next_run_at=?2,last_error=NULL WHERE id=1", params![current_time,next]).map_err(|problem| problem.to_string())?;
            Some((retention_count as usize, next))
        }
    };
    let Some((retention_count, _next_run)) = due else { return Ok(()) };
    let source = db_path();
    let directory = backup_dir();
    let created = tokio::task::spawn_blocking(move || create_automatic_backup(&source, &directory, retention_count).map_err(|problem| problem.to_string()))
        .await
        .map_err(|problem| problem.to_string())?;
    let db = state.lock().map_err(|_| "Database lock is unavailable".to_owned())?;
    match created {
        Ok((id, _size, pruned)) => {
            db.execute("UPDATE maintenance_backup_policy SET last_success_at=?1,last_error=NULL WHERE id=1", [now()]).map_err(|problem| problem.to_string())?;
            audit(&db, "system", "maintenance.backup.auto.create", &format!("{id}; pruned={pruned}"));
        }
        Err(problem) => {
            let safe_message = problem.chars().take(240).collect::<String>();
            db.execute("UPDATE maintenance_backup_policy SET last_error=?1,next_run_at=?2 WHERE id=1", params![safe_message, now().saturating_add(3600)]).map_err(|db_problem| db_problem.to_string())?;
            audit(&db, "system", "maintenance.backup.auto.failure", &safe_message);
        }
    }
    Ok(())
}

fn create_automatic_backup(source: &FsPath, directory: &FsPath, retention_count: usize) -> Result<(String, u64, usize), String> {
    fs::create_dir_all(directory).map_err(|problem| problem.to_string())?;
    #[cfg(unix)]
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).map_err(|problem| problem.to_string())?;
    let id = format!("arg-auto-{}-{}.db", now(), &token()[..8]);
    let destination = directory.join(&id);
    let temporary = directory.join(format!(".{id}.tmp"));
    let result = backup_database(source, &temporary)
        .map_err(|problem| problem.to_string())
        .and_then(|_| validate_backup(&temporary).map_err(str::to_owned))
        .and_then(|_| fs::rename(&temporary, &destination).map_err(|problem| problem.to_string()));
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result?;
    let size = fs::metadata(&destination).map_err(|problem| problem.to_string())?.len();
    let pruned = prune_automatic_backups(directory, retention_count).map_err(|problem| problem.to_string())?;
    Ok((id, size, pruned))
}

fn prune_automatic_backups(directory: &FsPath, retention_count: usize) -> std::io::Result<usize> {
    let mut entries = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("arg-auto-") || !valid_backup_id(&name) || !entry.file_type().ok()?.is_file() { return None; }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((entry.path(), modified))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| right.1.cmp(&left.1));
    let mut removed = 0;
    for (path, _) in entries.into_iter().skip(retention_count) {
        fs::remove_file(path)?;
        removed += 1;
    }
    Ok(removed)
}

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
                backups.push(json!({"id":name,"size_bytes":meta.len(),"automatic":name.starts_with("arg-auto-"),"modified_at":meta.modified().ok().and_then(|t|t.duration_since(UNIX_EPOCH).ok()).map(|d|d.as_secs())}));
            }
        }
    }
    backups.sort_by(|a, b| b["modified_at"].as_u64().cmp(&a["modified_at"].as_u64()));
    Ok(Json(json!({"backups":backups})))
}

pub(super) async fn get_backup_policy(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    backup_policy_response(&db)
}

fn backup_policy_response(db: &Connection) -> ApiResult {
    let (enabled, interval_hours, retention_count, last_started_at, last_success_at, last_error, next_run_at): (bool, i64, i64, Option<i64>, Option<i64>, Option<String>, Option<i64>) = db.query_row(
        "SELECT enabled,interval_hours,retention_count,last_started_at,last_success_at,last_error,next_run_at FROM maintenance_backup_policy WHERE id=1",
        [],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)),
    ).map_err(db_error)?;
    let automatic_count = fs::read_dir(backup_dir()).ok().into_iter().flatten().filter_map(Result::ok).filter(|entry| {
        entry.file_name().to_string_lossy().starts_with("arg-auto-") && entry.file_type().is_ok_and(|kind| kind.is_file())
    }).count();
    Ok(Json(json!({"enabled":enabled,"interval_hours":interval_hours,"retention_count":retention_count,"last_started_at":last_started_at,"last_success_at":last_success_at,"last_error":last_error,"next_run_at":next_run_at,"automatic_count":automatic_count})))
}

pub(super) async fn save_backup_policy(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<BackupPolicyInput>) -> ApiResult {
    check_origin(&headers)?;
    if !(BACKUP_INTERVAL_MIN_HOURS..=BACKUP_INTERVAL_MAX_HOURS).contains(&input.interval_hours)
        || !(BACKUP_RETENTION_MIN..=BACKUP_RETENTION_MAX).contains(&input.retention_count)
    {
        return Err(error(StatusCode::BAD_REQUEST, "بازهٔ بکاپ باید بین ۱ تا ۷۲۰ ساعت و نگه‌داری بین ۱ تا ۱۰۰ نسخه باشد."));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let current_time = now();
    let next_run = if input.enabled { Some(current_time.saturating_add(input.interval_hours.saturating_mul(3600))) } else { None };
    db.execute("UPDATE maintenance_backup_policy SET enabled=?1,interval_hours=?2,retention_count=?3,next_run_at=?4,last_error=NULL,updated_at=?5 WHERE id=1", params![input.enabled,input.interval_hours,input.retention_count,next_run,current_time]).map_err(db_error)?;
    audit(&db, &actor, "maintenance.backup.policy.update", &format!("enabled={}; interval_hours={}; retention_count={}", input.enabled, input.interval_hours, input.retention_count));
    backup_policy_response(&db)
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
    use super::{create_automatic_backup, prune_automatic_backups, valid_backup_id, validate_backup};

    #[test]
    fn backup_ids_reject_paths_and_unrelated_files() {
        assert!(valid_backup_id("arg-1790000000-acde1234.db"));
        assert!(valid_backup_id("arg-auto-1790000000-acde1234.db"));
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

    #[test]
    fn automatic_backup_is_valid_and_retention_preserves_manual_files() {
        let base = std::env::temp_dir().join(format!("arg-auto-backup-test-{}", &super::token()[..12]));
        let source = base.join("source.db");
        let dir = base.join("backups");
        std::fs::create_dir_all(&dir).unwrap();
        let db = rusqlite::Connection::open(&source).unwrap();
        db.execute_batch("CREATE TABLE users(id INTEGER PRIMARY KEY,username TEXT NOT NULL);").unwrap();
        drop(db);

        let (created, size, _) = create_automatic_backup(&source, &dir, 10).unwrap();
        assert!(created.starts_with("arg-auto-"));
        assert!(size > 0);
        assert_eq!(validate_backup(&dir.join(&created)), Ok(()));

        let manual = dir.join("arg-1790000000-manual123.db");
        std::fs::write(&manual, b"manual backup marker").unwrap();
        for index in 0..3 {
            std::fs::write(dir.join(format!("arg-auto-{index}-retention.db")), b"automatic backup marker").unwrap();
        }
        let removed = prune_automatic_backups(&dir, 2).unwrap();
        assert_eq!(removed, 2);
        assert!(manual.is_file());
        let remaining_auto = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("arg-auto-")).count();
        assert_eq!(remaining_auto, 2);
        std::fs::remove_dir_all(base).unwrap();
    }
}
