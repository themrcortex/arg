use super::*;
use std::path::Path as FsPath;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VolumeInput {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    quota_bytes: Option<u64>,
}

fn valid_volume_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 48
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
}

fn mounted_paths() -> HashSet<PathBuf> {
    fs::read_to_string("/proc/self/mountinfo")
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_whitespace().nth(4))
        .map(|path| {
            PathBuf::from(
                path.replace("\\040", " ")
                    .replace("\\011", "\t")
                    .replace("\\012", "\n")
                    .replace("\\134", "\\"),
            )
        })
        .collect()
}

fn contains_mountpoint(root: &FsPath, mounts: &HashSet<PathBuf>) -> bool {
    mounts
        .iter()
        .any(|mount| mount == root || mount.starts_with(root))
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS application_volumes(id TEXT PRIMARY KEY,application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,name TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',relative_path TEXT NOT NULL,quota_bytes INTEGER,usage_bytes INTEGER,usage_status TEXT NOT NULL DEFAULT 'not_measured',measured_at INTEGER,created_by TEXT NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,UNIQUE(application_id,name COLLATE NOCASE)); CREATE INDEX IF NOT EXISTS application_volumes_app ON application_volumes(application_id);")?;
    for (name, declaration) in [
        ("usage_bytes", "INTEGER"),
        ("usage_status", "TEXT NOT NULL DEFAULT 'not_measured'"),
        ("measured_at", "INTEGER"),
    ] {
        let mut statement = db.prepare("PRAGMA table_info(application_volumes)")?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if !columns.iter().any(|column| column == name) {
            db.execute(
                &format!("ALTER TABLE application_volumes ADD COLUMN {name} {declaration}"),
                [],
            )?;
        }
    }
    Ok(())
}

pub(super) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let framework:Option<String>=db.query_row("SELECT framework FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",params![app_id,owner],|r|r.get(0)).optional().map_err(db_error)?;
    let Some(framework) = framework else {
        return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."));
    };
    if !apps::is_node_runtime_framework(&framework) && !apps::is_auto_framework(&framework) {
        return Err(error(
            StatusCode::CONFLICT,
            "Volume پایدار فعلاً برای سرویس‌های Node/SSR فراهم است.",
        ));
    }
    let mut stmt = db.prepare("SELECT id,name,description,relative_path,quota_bytes,created_at,usage_bytes,usage_status,measured_at FROM application_volumes WHERE application_id=?1 ORDER BY name COLLATE NOCASE").map_err(db_error)?;
    let volumes = stmt.query_map([app_id], |row| {
        Ok(json!({"id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"description":row.get::<_,String>(2)?,"relative_path":row.get::<_,String>(3)?,"quota_bytes":row.get::<_,Option<i64>>(4)?,"created_at":row.get::<_,i64>(5)?,"usage_bytes":row.get::<_,Option<i64>>(6)?,"usage_status":row.get::<_,String>(7)?,"measured_at":row.get::<_,Option<i64>>(8)?,"quota_enforced":false}))
    }).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
    Ok(Json(
        json!({"volumes":volumes,"base":"/shared/data/volumes","note":"برای Node، این مسیر در همهٔ releaseها پایدار است. سقف ثبت‌شده اطلاع‌رسانی است و quota سیستم‌فایلی اعمال نمی‌کند."}),
    ))
}

pub(super) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<VolumeInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !apps::valid_uuid(&app_id)
        || !valid_volume_name(input.name.trim())
        || input.description.chars().count() > 500
        || input
            .quota_bytes
            .is_some_and(|value| value == 0 || value > i64::MAX as u64)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام، توضیح یا سقف volume معتبر نیست.",
        ));
    }
    let (actor, relative_path, id) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let framework:Option<String>=db.query_row("SELECT framework FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",params![app_id,owner],|r|r.get(0)).optional().map_err(db_error)?;
        let Some(framework) = framework else {
            return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."));
        };
        if !apps::is_node_runtime_framework(&framework) && !apps::is_auto_framework(&framework) {
            return Err(error(
                StatusCode::CONFLICT,
                "Volume پایدار فعلاً برای سرویس‌های Node/SSR فراهم است.",
            ));
        }
        let duplicate: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM application_volumes WHERE application_id=?1 AND name=?2 COLLATE NOCASE)",
                params![app_id,input.name.trim()],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        if duplicate {
            return Err(error(
                StatusCode::CONFLICT,
                "این نام volume برای اپ تکراری است.",
            ));
        }
        (
            actor,
            format!("apps/{app_id}/shared/data/volumes/{}", input.name.trim()),
            apps::random_uuid(),
        )
    };
    let result=privileged_request_async(json!({"action":"app-volume-operation","operation":"create","application_id":app_id,"name":input.name.trim()})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("ساخت پوشهٔ پایدار ناموفق بود."),
        ));
    }
    let inserted = {
        let db = state.lock().unwrap();
        if db.execute("INSERT INTO application_volumes(id,application_id,name,description,relative_path,quota_bytes,created_by,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8)",params![id,app_id,input.name.trim(),input.description.trim(),relative_path,input.quota_bytes.map(|v|v as i64),actor,now()]).is_ok() {
            audit(
                &db,
                &actor,
                "app-volume.create",
                &format!("app={app_id}; name={}", input.name.trim()),
            );
            true
        } else {
            false
        }
    };
    if !inserted {
        let _=privileged_request_async(json!({"action":"app-volume-operation","operation":"purge","application_id":app_id,"name":input.name.trim(),"confirmed":true})).await;
        return Err(error(
            StatusCode::CONFLICT,
            "نام volume برای این اپ تکراری است یا ثبت آن انجام نشد.",
        ));
    }
    Ok(Json(
        json!({"id":id,"application_id":app_id,"name":input.name.trim(),"relative_path":relative_path,"absolute_path":result["path"],"quota_bytes":input.quota_bytes,"quota_enforced":false,"usage_status":"not_measured"}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConfirmRequest {
    confirmed: bool,
}

pub(super) async fn scan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult {
    check_origin(&headers)?;
    let (actor, app_id, name) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let row=db.query_row("SELECT v.application_id,v.name FROM application_volumes v JOIN applications a ON a.id=v.application_id WHERE v.id=?1 AND a.owner_server_uuid=?2 AND a.deleted_at IS NULL",params![id,owner],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional().map_err(db_error)?.ok_or_else(||error(StatusCode::NOT_FOUND,"Volume پیدا نشد."))?;
        (actor, row.0, row.1)
    };
    let result=privileged_request_async(json!({"action":"app-volume-operation","operation":"scan","application_id":app_id,"name":name})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("خواندن اندازهٔ volume ناموفق بود."),
        ));
    }
    let bytes = result["bytes"].as_u64().unwrap_or(0);
    let at = now();
    let status = result["complete"]
        .as_bool()
        .unwrap_or(false)
        .then_some("measured")
        .unwrap_or("partial");
    let db = state.lock().unwrap();
    db.execute(
        "UPDATE application_volumes SET usage_bytes=?1,usage_status=?2,measured_at=?3,updated_at=?3 WHERE id=?4",
        params![i64::try_from(bytes).unwrap_or(i64::MAX), status, at, id],
    )
    .map_err(db_error)?;
    audit(
        &db,
        &actor,
        "app-volume.scan",
        &format!("volume={id}; bytes={bytes}; status={status}"),
    );
    Ok(Json(
        json!({"id":id,"usage_bytes":bytes,"usage_status":status,"entries_scanned":result["entries_scanned"],"measured_at":at}),
    ))
}

pub(super) async fn detach(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let changed=db.execute("DELETE FROM application_volumes WHERE id=?1 AND application_id IN (SELECT id FROM applications WHERE owner_server_uuid=?2)",params![id,owner]).map_err(db_error)?;
    if changed == 0 {
        return Err(error(StatusCode::NOT_FOUND, "Volume پیدا نشد."));
    }
    audit(
        &db,
        &actor,
        "app-volume.detach",
        &format!("volume={id}; data=retained"),
    );
    Ok(Json(json!({"detached":true,"data_retained":true})))
}

pub(super) async fn purge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(confirm): Json<ConfirmRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if !confirm.confirmed {
        return Err(error(StatusCode::BAD_REQUEST, "تأیید حذف داده لازم است."));
    }
    let (actor, app_id, name, owner) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let row=db.query_row("SELECT v.application_id,v.name FROM application_volumes v JOIN applications a ON a.id=v.application_id WHERE v.id=?1 AND a.owner_server_uuid=?2",params![id,owner],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional().map_err(db_error)?.ok_or_else(||error(StatusCode::NOT_FOUND,"Volume پیدا نشد."))?;
        (actor, row.0, row.1, owner)
    };
    let result=privileged_request_async(json!({"action":"app-volume-operation","operation":"purge","application_id":app_id,"name":name,"confirmed":true})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("حذف دادهٔ volume ناموفق بود."),
        ));
    }
    let db = state.lock().unwrap();
    db.execute("DELETE FROM application_volumes WHERE id=?1 AND application_id IN (SELECT id FROM applications WHERE owner_server_uuid=?2)",params![id,owner]).map_err(db_error)?;
    audit(&db, &actor, "app-volume.purge", &format!("volume={id}"));
    Ok(Json(json!({"purged":true})))
}

pub(super) fn execute_privileged_request(request: &Value) -> Value {
    let operation = request["operation"].as_str().unwrap_or_default();
    let Some(app_id) = request["application_id"]
        .as_str()
        .filter(|value| apps::valid_uuid(value))
    else {
        return json!({"ok":false,"error":"Application ID is invalid"});
    };
    let Some(name) = request["name"]
        .as_str()
        .filter(|value| valid_volume_name(value))
    else {
        return json!({"ok":false,"error":"Volume name is invalid"});
    };
    let root = env::var_os("ARG_DEPLOY_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/arg/deployments"));
    let app = root.join("apps").join(app_id);
    let shared = app.join("shared");
    let data = shared.join("data");
    let volume = data.join("volumes").join(name);
    if operation == "create" {
        for path in [
            &root,
            &root.join("apps"),
            &app,
            &shared,
            &data,
            &data.join("volumes"),
        ] {
            if let Ok(meta) = fs::symlink_metadata(path) {
                if !meta.is_dir() || meta.file_type().is_symlink() {
                    return json!({"ok":false,"error":"Volume parent path is not a normal directory"});
                }
            } else if let Err(err) = fs::create_dir(path) {
                if err.kind() != std::io::ErrorKind::AlreadyExists {
                    return json!({"ok":false,"error":err.to_string()});
                }
            }
        }
        let group = format!("argapp-{}", &app_id[..12]);
        if let Err(err) = node::ensure_group(&group) {
            return json!({"ok":false,"error":err});
        }
        let mut shared_chown = std::process::Command::new("/usr/bin/chown");
        shared_chown.args([
            "--no-dereference",
            &format!("root:{group}"),
            shared.to_string_lossy().as_ref(),
        ]);
        if let Err(err) =
            command_result(&mut shared_chown)
                .and_then(|(ok, out)| if ok { Ok((ok, out)) } else { Err(out) })
        {
            return json!({"ok":false,"error":err});
        }
        if let Err(err) = fs::set_permissions(&shared, fs::Permissions::from_mode(0o1770)) {
            return json!({"ok":false,"error":err.to_string()});
        }
        let mut data_chown = std::process::Command::new("/usr/bin/chown");
        data_chown.args([
            "--no-dereference",
            &format!("root:{group}"),
            data.to_string_lossy().as_ref(),
        ]);
        if let Err(err) =
            command_result(&mut data_chown)
                .and_then(|(ok, out)| if ok { Ok((ok, out)) } else { Err(out) })
        {
            return json!({"ok":false,"error":err});
        }
        if let Err(err) = fs::set_permissions(&data, fs::Permissions::from_mode(0o1770)) {
            return json!({"ok":false,"error":err.to_string()});
        }
        let parent = data.join("volumes");
        let mut parent_chown = std::process::Command::new("/usr/bin/chown");
        parent_chown.args([
            "--no-dereference",
            &format!("root:{group}"),
            parent.to_string_lossy().as_ref(),
        ]);
        if let Err(err) =
            command_result(&mut parent_chown)
                .and_then(|(ok, out)| if ok { Ok((ok, out)) } else { Err(out) })
        {
            return json!({"ok":false,"error":err});
        }
        if let Err(err) = fs::set_permissions(&parent, fs::Permissions::from_mode(0o1770)) {
            return json!({"ok":false,"error":err.to_string()});
        }
        if fs::symlink_metadata(&volume).is_ok() {
            return json!({"ok":false,"error":"Volume path already exists; refusing to adopt an existing directory"});
        }
        if let Err(err) = fs::create_dir(&volume) {
            return json!({"ok":false,"error":err.to_string()});
        }
        let Ok(base) = data.canonicalize() else {
            return json!({"ok":false,"error":"Application data path could not be verified"});
        };
        let Ok(canonical) = volume.canonicalize() else {
            return json!({"ok":false,"error":"New volume path could not be verified"});
        };
        if !canonical.starts_with(base)
            || fs::symlink_metadata(&volume).is_ok_and(|meta| meta.file_type().is_symlink())
        {
            return json!({"ok":false,"error":"Volume path escaped its managed data root"});
        }
        let mut chown = std::process::Command::new("/usr/bin/chown");
        chown.args([
            format!("root:{group}"),
            volume.to_string_lossy().into_owned(),
        ]);
        if let Err(err) =
            command_result(&mut chown)
                .and_then(|(ok, out)| if ok { Ok((ok, out)) } else { Err(out) })
        {
            let _ = fs::remove_dir(&volume);
            return json!({"ok":false,"error":err});
        }
        if let Err(err) = fs::set_permissions(&volume, fs::Permissions::from_mode(0o2770)) {
            let _ = fs::remove_dir(&volume);
            return json!({"ok":false,"error":err.to_string()});
        }
        return json!({"ok":true,"path":volume.to_string_lossy()});
    }
    let Ok(canonical_base) = data.canonicalize() else {
        return json!({"ok":false,"error":"Application data directory not found"});
    };
    let Ok(canonical_volume) = volume.canonicalize() else {
        return json!({"ok":false,"error":"Volume directory not found"});
    };
    if !canonical_volume.starts_with(&canonical_base)
        || fs::symlink_metadata(&volume).is_ok_and(|m| m.file_type().is_symlink())
    {
        return json!({"ok":false,"error":"Volume path escaped the managed application data root"});
    }
    if operation == "purge" {
        if request["confirmed"] != true {
            return json!({"ok":false,"error":"Confirmation required"});
        }
        let Ok(base_meta) = fs::metadata(&canonical_base) else {
            return json!({"ok":false,"error":"Volume base could not be checked"});
        };
        let Ok(volume_meta) = fs::metadata(&canonical_volume) else {
            return json!({"ok":false,"error":"Volume could not be checked"});
        };
        if volume_meta.dev() != base_meta.dev()
            || contains_mountpoint(&canonical_volume, &mounted_paths())
        {
            return json!({"ok":false,"error":"Refusing to purge a volume that is a separate mounted filesystem"});
        }
        return match fs::remove_dir_all(&canonical_volume) {
            Ok(()) => json!({"ok":true}),
            Err(err) => json!({"ok":false,"error":err.to_string()}),
        };
    }
    if operation == "scan" {
        let mounts = mounted_paths();
        if mounts.contains(&canonical_volume) {
            return json!({"ok":false,"error":"Refusing to scan a volume root that is itself a mounted filesystem"});
        }
        let mut stack = vec![canonical_volume];
        let mut bytes = 0_u64;
        let mut count = 0_u64;
        let mut complete = true;
        while let Some(path) = stack.pop() {
            let Ok(entries) = fs::read_dir(path) else {
                complete = false;
                continue;
            };
            for entry in entries.flatten() {
                count += 1;
                if count > 100_000 {
                    complete = false;
                    break;
                }
                let p = entry.path();
                if mounts.contains(&p) {
                    continue;
                }
                let Ok(meta) = fs::symlink_metadata(&p) else {
                    complete = false;
                    continue;
                };
                if meta.file_type().is_symlink() {
                    continue;
                }
                if meta.is_dir() {
                    stack.push(p);
                } else if meta.is_file() {
                    bytes = bytes.saturating_add(meta.len());
                }
            }
            if count > 100_000 {
                break;
            }
        }
        return json!({"ok":true,"bytes":bytes,"entries_scanned":count,"complete":complete});
    }
    json!({"ok":false,"error":"Volume operation is not allowlisted"})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn volume_scan_rejects_missing_and_cross_origin_requests() {
        let state = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));

        let missing_origin = scan(
            State(state.clone()),
            HeaderMap::new(),
            Path("volume-1".to_owned()),
        )
        .await;
        assert_eq!(missing_origin.unwrap_err().0, StatusCode::FORBIDDEN);

        let mut cross_origin = HeaderMap::new();
        cross_origin.insert(header::HOST, HeaderValue::from_static("panel.example.test"));
        cross_origin.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://attacker.example.test"),
        );
        let rejected = scan(
            State(state.clone()),
            cross_origin,
            Path("volume-1".to_owned()),
        )
        .await;
        assert_eq!(rejected.unwrap_err().0, StatusCode::FORBIDDEN);

        let mut same_origin = HeaderMap::new();
        same_origin.insert(header::HOST, HeaderValue::from_static("panel.example.test"));
        same_origin.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://panel.example.test"),
        );
        let authenticated = scan(
            State(state),
            same_origin,
            Path("volume-1".to_owned()),
        )
        .await;
        assert_eq!(authenticated.unwrap_err().0, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn volume_names_reject_paths() {
        assert!(valid_volume_name("uploads_1"));
        assert!(!valid_volume_name("../etc"));
        assert!(!valid_volume_name("a/b"));
    }
    #[test]
    fn migration_is_additive_and_idempotent() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE applications(id TEXT PRIMARY KEY); CREATE TABLE application_volumes(id TEXT PRIMARY KEY,application_id TEXT NOT NULL REFERENCES applications(id),name TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',relative_path TEXT NOT NULL,quota_bytes INTEGER,created_by TEXT NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,UNIQUE(application_id,name COLLATE NOCASE)); PRAGMA foreign_keys=ON;")
            .unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        let count:i64=db.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='application_volumes'",[],|r|r.get(0)).unwrap();
        assert_eq!(count, 1);
        let columns: Vec<String> = db
            .prepare("PRAGMA table_info(application_volumes)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(columns.contains(&"usage_bytes".to_owned()));
        assert!(columns.contains(&"measured_at".to_owned()));
    }
}
