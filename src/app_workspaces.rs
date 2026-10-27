use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NamedResourceInput {
    name: String,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnvironmentInput {
    name: String,
    kind: String,
    #[serde(default)]
    protected: bool,
}

fn valid_name(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.chars().count() <= 80 && !value.chars().any(char::is_control)
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_workspaces(
            id TEXT PRIMARY KEY,
            owner_server_uuid TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            is_default INTEGER NOT NULL DEFAULT 0,
            created_by TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            UNIQUE(owner_server_uuid,name COLLATE NOCASE)
         );
         CREATE UNIQUE INDEX IF NOT EXISTS app_one_default_workspace
            ON app_workspaces(owner_server_uuid) WHERE is_default=1;
         CREATE TABLE IF NOT EXISTS app_projects(
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL REFERENCES app_workspaces(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            created_by TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            UNIQUE(workspace_id,name COLLATE NOCASE)
         );
         CREATE INDEX IF NOT EXISTS app_projects_workspace ON app_projects(workspace_id);
         CREATE TABLE IF NOT EXISTS app_environments(
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES app_projects(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            kind TEXT NOT NULL CHECK(kind IN ('development','staging','production','custom')),
            protected INTEGER NOT NULL DEFAULT 0,
            created_by TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            UNIQUE(project_id,name COLLATE NOCASE)
         );
         CREATE INDEX IF NOT EXISTS app_environments_project ON app_environments(project_id);
         CREATE TABLE IF NOT EXISTS application_context(
            application_id TEXT PRIMARY KEY REFERENCES applications(id) ON DELETE CASCADE,
            environment_id TEXT NOT NULL REFERENCES app_environments(id) ON DELETE RESTRICT,
            updated_by TEXT NOT NULL,
            updated_at INTEGER NOT NULL
         );",
    )?;
    let workspace_columns = {
        let mut statement = db.prepare("PRAGMA table_info(app_workspaces)")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if !workspace_columns
        .iter()
        .any(|column| column == "description")
    {
        db.execute(
            "ALTER TABLE app_workspaces ADD COLUMN description TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }

    // Existing applications are retained and assigned to an explicit default
    // project/environment. Seed is idempotent so startup can safely resume.
    let owners = {
        let mut statement = db.prepare("SELECT DISTINCT owner_server_uuid FROM applications")?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let mut owners = owners;
    let current_owner = federation::local_server_uuid(db)?;
    if !owners.contains(&current_owner) {
        owners.push(current_owner);
    }
    for owner in owners {
        ensure_defaults(db, &owner, "system", true)?;
    }
    Ok(())
}

fn ensure_defaults(
    db: &Connection,
    owner: &str,
    actor: &str,
    backfill_existing: bool,
) -> rusqlite::Result<String> {
    let workspace: Option<String> = db
        .query_row(
            "SELECT id FROM app_workspaces WHERE owner_server_uuid=?1 AND is_default=1",
            [owner],
            |row| row.get(0),
        )
        .optional()?;
    let workspace = if let Some(id) = workspace {
        id
    } else {
        let id = apps::random_uuid();
        db.execute(
        "INSERT INTO app_workspaces(id,owner_server_uuid,name,description,is_default,created_by,created_at,updated_at) VALUES(?1,?2,'پیش‌فرض','',1,?3,?4,?4)",
            params![id, owner, actor, now()],
        )?;
        id
    };

    let project: Option<String> = db
        .query_row(
            "SELECT id FROM app_projects WHERE workspace_id=?1 AND name='پیش‌فرض' COLLATE NOCASE",
            [&workspace],
            |row| row.get(0),
        )
        .optional()?;
    let project = if let Some(id) = project {
        id
    } else {
        let id = apps::random_uuid();
        db.execute(
            "INSERT INTO app_projects(id,workspace_id,name,description,created_by,created_at,updated_at) VALUES(?1,?2,'پیش‌فرض','محیط انتقالی برای اپلیکیشن‌های موجود',?3,?4,?4)",
            params![id, workspace, actor, now()],
        )?;
        id
    };

    let environment: Option<String> = db
        .query_row(
            "SELECT id FROM app_environments WHERE project_id=?1 AND kind='production' ORDER BY created_at,id LIMIT 1",
            [&project],
            |row| row.get(0),
        )
        .optional()?;
    let environment = if let Some(id) = environment {
        id
    } else {
        let id = apps::random_uuid();
        db.execute(
            "INSERT INTO app_environments(id,project_id,name,kind,protected,created_by,created_at,updated_at) VALUES(?1,?2,'Production','production',1,?3,?4,?4)",
            params![id, project, actor, now()],
        )?;
        id
    };
    if backfill_existing {
        db.execute(
            "INSERT OR IGNORE INTO application_context(application_id,environment_id,updated_by,updated_at)
             SELECT id,?1,?2,?3 FROM applications WHERE owner_server_uuid=?4",
            params![environment, actor, now(), owner],
        )?;
    }
    Ok(workspace)
}

pub(super) fn assign_default(
    db: &Connection,
    app_id: &str,
    owner: &str,
    actor: &str,
) -> rusqlite::Result<()> {
    let workspace = ensure_defaults(db, owner, actor, false)?;
    let environment: String = db.query_row(
        "SELECT e.id FROM app_environments e JOIN app_projects p ON p.id=e.project_id WHERE p.workspace_id=?1 AND p.name='پیش‌فرض' COLLATE NOCASE AND e.kind='production' ORDER BY e.created_at,e.id LIMIT 1",
        [&workspace], |row| row.get(0),
    )?;
    db.execute(
        "INSERT OR IGNORE INTO application_context(application_id,environment_id,updated_by,updated_at) VALUES(?1,?2,?3,?4)",
        params![app_id, environment, actor, now()],
    )?;
    Ok(())
}

pub(super) fn environment_is_owned_by(
    db: &Connection,
    environment_id: &str,
    owner: &str,
) -> rusqlite::Result<bool> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM app_environments e JOIN app_projects p ON p.id=e.project_id JOIN app_workspaces w ON w.id=p.workspace_id WHERE e.id=?1 AND w.owner_server_uuid=?2)",
        params![environment_id, owner],
        |row| row.get(0),
    )
}

pub(super) fn set_application_environment(
    db: &Connection,
    app_id: &str,
    environment_id: &str,
    actor: &str,
) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO application_context(application_id,environment_id,updated_by,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(application_id) DO UPDATE SET environment_id=excluded.environment_id,updated_by=excluded.updated_by,updated_at=excluded.updated_at",
        params![app_id, environment_id, actor, now()],
    )?;
    Ok(())
}

pub(super) fn add_context(db: &Connection, apps: &mut [Value]) -> rusqlite::Result<()> {
    let mut statement = db.prepare(
        "SELECT c.application_id,e.id,e.name,e.kind,e.protected,p.id,p.name,w.id,w.name
         FROM application_context c
         JOIN app_environments e ON e.id=c.environment_id
         JOIN app_projects p ON p.id=e.project_id
         JOIN app_workspaces w ON w.id=p.workspace_id",
    )?;
    let contexts = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                json!({
                    "environment_id": row.get::<_,String>(1)?,
                    "environment_name": row.get::<_,String>(2)?,
                    "environment_kind": row.get::<_,String>(3)?,
                    "environment_protected": row.get::<_,bool>(4)?,
                    "project_id": row.get::<_,String>(5)?,
                    "project_name": row.get::<_,String>(6)?,
                    "workspace_id": row.get::<_,String>(7)?,
                    "workspace_name": row.get::<_,String>(8)?
                }),
            ))
        })?
        .collect::<Result<std::collections::HashMap<_, _>, _>>()?;
    for app in apps {
        if let Some(context) = app["id"].as_str().and_then(|id| contexts.get(id)) {
            if let (Some(target), Some(source)) = (app.as_object_mut(), context.as_object()) {
                target.extend(source.clone());
            }
        }
    }
    Ok(())
}

pub(super) async fn list_workspaces(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let mut statement = db
        .prepare("SELECT id,name,description,is_default,created_at FROM app_workspaces WHERE owner_server_uuid=?1 ORDER BY is_default DESC,name COLLATE NOCASE")
        .map_err(db_error)?;
    let rows = statement
        .query_map([owner], |row| {
            Ok(json!({"id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"description":row.get::<_,String>(2)?,"is_default":row.get::<_,bool>(3)?,"created_at":row.get::<_,i64>(4)?}))
        })
        .map_err(db_error)?;
    let workspaces = rows.filter_map(Result::ok).collect::<Vec<_>>();
    Ok(Json(json!({"workspaces":workspaces})))
}

pub(super) async fn create_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<NamedResourceInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let name = input.name.trim();
    if !valid_name(name) || input.description.chars().count() > 1000 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام یا توضیحات فضای‌کار معتبر نیست.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let transaction = db.unchecked_transaction().map_err(db_error)?;
    let id = apps::random_uuid();
    transaction.execute(
        "INSERT INTO app_workspaces(id,owner_server_uuid,name,description,is_default,created_by,created_at,updated_at) VALUES(?1,?2,?3,?4,0,?5,?6,?6)",
        params![id, owner, name, input.description.trim(), actor, now()],
    ).map_err(|error_value| if error_value.sqlite_error_code()==Some(rusqlite::ErrorCode::ConstraintViolation) { error(StatusCode::CONFLICT,"فضای‌کاری با این نام از قبل وجود دارد.") } else { db_error(error_value) })?;
    let project_id = apps::random_uuid();
    transaction.execute(
        "INSERT INTO app_projects(id,workspace_id,name,description,created_by,created_at,updated_at) VALUES(?1,?2,'پیش‌فرض','',?3,?4,?4)",
        params![project_id, id, actor, now()],
    ).map_err(db_error)?;
    let environment_id = apps::random_uuid();
    transaction.execute(
        "INSERT INTO app_environments(id,project_id,name,kind,protected,created_by,created_at,updated_at) VALUES(?1,?2,'Production','production',1,?3,?4,?4)",
        params![environment_id, project_id, actor, now()],
    ).map_err(db_error)?;
    transaction.commit().map_err(db_error)?;
    audit(
        &db,
        &actor,
        "app.workspace.create",
        &format!("workspace={id}"),
    );
    Ok(Json(
        json!({"id":id,"name":name,"description":input.description.trim(),"project_id":project_id,"default_environment_id":environment_id}),
    ))
}

pub(super) async fn list_projects(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM app_workspaces WHERE id=?1 AND owner_server_uuid=?2)",
            params![workspace_id, owner],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if !exists {
        return Err(error(StatusCode::NOT_FOUND, "فضای‌کار پیدا نشد."));
    }
    let mut statement = db.prepare(
        "SELECT id,name,description,created_at,(SELECT COUNT(*) FROM app_environments e WHERE e.project_id=p.id) FROM app_projects p WHERE workspace_id=?1 ORDER BY name COLLATE NOCASE",
    ).map_err(db_error)?;
    let rows = statement.query_map([workspace_id], |row| Ok(json!({"id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"description":row.get::<_,String>(2)?,"created_at":row.get::<_,i64>(3)?,"environment_count":row.get::<_,i64>(4)?}))).map_err(db_error)?;
    Ok(Json(
        json!({"projects":rows.filter_map(Result::ok).collect::<Vec<_>>()}),
    ))
}

pub(super) async fn create_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
    Json(input): Json<NamedResourceInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let name = input.name.trim();
    if !valid_name(name) || input.description.chars().count() > 1000 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام یا توضیحات پروژه معتبر نیست.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let valid: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM app_workspaces WHERE id=?1 AND owner_server_uuid=?2)",
            params![workspace_id, owner],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if !valid {
        return Err(error(StatusCode::NOT_FOUND, "فضای‌کار پیدا نشد."));
    }
    let transaction = db.unchecked_transaction().map_err(db_error)?;
    let id = apps::random_uuid();
    let at = now();
    transaction.execute("INSERT INTO app_projects(id,workspace_id,name,description,created_by,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6)",params![id,workspace_id,name,input.description.trim(),actor,at]).map_err(|error_value| if error_value.sqlite_error_code()==Some(rusqlite::ErrorCode::ConstraintViolation) { error(StatusCode::CONFLICT,"پروژه‌ای با این نام از قبل وجود دارد.") } else { db_error(error_value) })?;
    let mut created = Vec::new();
    for (name, kind, protected) in [
        ("Development", "development", false),
        ("Staging", "staging", false),
        ("Production", "production", true),
    ] {
        let environment_id = apps::random_uuid();
        transaction.execute("INSERT INTO app_environments(id,project_id,name,kind,protected,created_by,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)",params![environment_id,id,name,kind,protected,actor,at]).map_err(db_error)?;
        created.push(json!({"id":environment_id,"name":name,"kind":kind,"protected":protected}));
    }
    transaction.commit().map_err(db_error)?;
    audit(
        &db,
        &actor,
        "app.project.create",
        &format!("project={id}; workspace={workspace_id}"),
    );
    Ok(Json(
        json!({"id":id,"workspace_id":workspace_id,"name":name,"environments":created}),
    ))
}

pub(super) async fn list_environments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project_id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM app_projects p JOIN app_workspaces w ON w.id=p.workspace_id WHERE p.id=?1 AND w.owner_server_uuid=?2)",params![project_id,owner],|row|row.get(0)).map_err(db_error)?;
    if !valid {
        return Err(error(StatusCode::NOT_FOUND, "پروژه پیدا نشد."));
    }
    let mut statement = db.prepare("SELECT id,name,kind,protected,created_at FROM app_environments WHERE project_id=?1 ORDER BY CASE kind WHEN 'development' THEN 1 WHEN 'staging' THEN 2 WHEN 'production' THEN 3 ELSE 4 END,name COLLATE NOCASE").map_err(db_error)?;
    let rows = statement.query_map([project_id],|row|Ok(json!({"id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"kind":row.get::<_,String>(2)?,"protected":row.get::<_,bool>(3)?,"created_at":row.get::<_,i64>(4)?}))).map_err(db_error)?;
    Ok(Json(
        json!({"environments":rows.filter_map(Result::ok).collect::<Vec<_>>()}),
    ))
}

pub(super) async fn create_environment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project_id): Path<String>,
    Json(input): Json<EnvironmentInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let name = input.name.trim();
    if !valid_name(name)
        || !["development", "staging", "production", "custom"].contains(&input.kind.as_str())
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام یا نوع محیط معتبر نیست.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM app_projects p JOIN app_workspaces w ON w.id=p.workspace_id WHERE p.id=?1 AND w.owner_server_uuid=?2)",params![project_id,owner],|row|row.get(0)).map_err(db_error)?;
    if !valid {
        return Err(error(StatusCode::NOT_FOUND, "پروژه پیدا نشد."));
    }
    let id = apps::random_uuid();
    let at = now();
    db.execute("INSERT INTO app_environments(id,project_id,name,kind,protected,created_by,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)",params![id,project_id,name,input.kind,input.protected,actor,at]).map_err(|error_value| if error_value.sqlite_error_code()==Some(rusqlite::ErrorCode::ConstraintViolation) { error(StatusCode::CONFLICT,"محیطی با این نام از قبل وجود دارد.") } else { db_error(error_value) })?;
    audit(
        &db,
        &actor,
        "app.environment.create",
        &format!(
            "environment={id}; project={project_id}; kind={}",
            input.kind
        ),
    );
    Ok(Json(
        json!({"id":id,"project_id":project_id,"name":name,"kind":input.kind,"protected":input.protected}),
    ))
}

pub(super) async fn assign_environment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<serde_json::Value>,
) -> ApiResult {
    check_origin(&headers)?;
    let environment_id = input["environment_id"].as_str().unwrap_or_default();
    if !apps::valid_uuid(&app_id) || !apps::valid_uuid(environment_id) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "شناسهٔ اپ یا محیط معتبر نیست.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let app_valid: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL)",
        params![app_id,owner],|row|row.get(0),
    ).map_err(db_error)?;
    let env_valid = environment_is_owned_by(&db, environment_id, &owner).map_err(db_error)?;
    if !app_valid || !env_valid {
        return Err(error(
            StatusCode::NOT_FOUND,
            "اپلیکیشن یا محیط متعلق به این VPS پیدا نشد.",
        ));
    }
    let protected: bool = db
        .query_row(
            "SELECT protected FROM app_environments WHERE id=?1",
            [environment_id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    set_application_environment(&db, &app_id, environment_id, &actor).map_err(db_error)?;
    audit(
        &db,
        &actor,
        "app.environment.assign",
        &format!("app={app_id}; environment={environment_id}"),
    );
    Ok(Json(
        json!({"application_id":app_id,"environment_id":environment_id,"environment_protected":protected,"config_or_secret_copied":false}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_preserves_apps_and_assigns_a_default_context_idempotently() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL); INSERT INTO settings VALUES('federation_server_uuid','server-a'); CREATE TABLE applications(id TEXT PRIMARY KEY,owner_server_uuid TEXT NOT NULL,name TEXT NOT NULL,created_by TEXT NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL); INSERT INTO applications VALUES('app-a','server-a','Existing','admin',1,1);").unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        let (contexts, projects, envs): (i64,i64,i64) = db.query_row(
            "SELECT (SELECT COUNT(*) FROM application_context),(SELECT COUNT(*) FROM app_projects),(SELECT COUNT(*) FROM app_environments)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        ).unwrap();
        assert_eq!((contexts, projects, envs), (1, 1, 1));
        let env_kind: String = db.query_row("SELECT e.kind FROM application_context c JOIN app_environments e ON e.id=c.environment_id WHERE c.application_id='app-a'",[],|row|row.get(0)).unwrap();
        assert_eq!(env_kind, "production");
        let environment_id: String = db
            .query_row(
                "SELECT environment_id FROM application_context WHERE application_id='app-a'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(environment_is_owned_by(&db, &environment_id, "server-a").unwrap());
        assert!(!environment_is_owned_by(&db, &environment_id, "server-b").unwrap());
    }

    #[test]
    fn names_reject_controls_and_empty_values() {
        assert!(valid_name("Customer workspace"));
        assert!(!valid_name("  "));
        assert!(!valid_name("a\nroot"));
    }
}
