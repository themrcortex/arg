use super::*;
use std::collections::{HashMap, HashSet, VecDeque};

const MAX_STEPS: usize = 24;
const STEP_KINDS: &[&str] = &["deploy", "health_check", "backup", "notify", "approval"];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkflowStep {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkflowInput {
    pub schema_version: u32,
    pub name: String,
    pub trigger: String,
    pub steps: Vec<WorkflowStep>,
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS application_workflow_drafts(
            id TEXT PRIMARY KEY,
            application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
            owner_server_uuid TEXT NOT NULL,
            schema_version INTEGER NOT NULL,
            name TEXT NOT NULL,
            trigger_kind TEXT NOT NULL,
            workflow_json TEXT NOT NULL,
            created_by TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS app_workflow_drafts_owner ON application_workflow_drafts(application_id,owner_server_uuid,updated_at DESC);",
    )
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 48
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn validate(input: &WorkflowInput) -> Result<(), &'static str> {
    if input.schema_version != 1 {
        return Err("نسخهٔ طرح workflow پشتیبانی نمی‌شود.");
    }
    if input.name.trim().is_empty() || input.name.chars().count() > 80 {
        return Err("نام workflow باید بین ۱ تا ۸۰ نویسه باشد.");
    }
    if input.trigger != "manual" {
        return Err("در این نسخه فقط trigger دستی به‌صورت پیش‌نویس ذخیره می‌شود.");
    }
    if input.steps.is_empty() || input.steps.len() > MAX_STEPS {
        return Err("workflow باید بین ۱ تا ۲۴ گام داشته باشد.");
    }

    let mut ids = HashSet::with_capacity(input.steps.len());
    for step in &input.steps {
        if !valid_id(&step.id) || !ids.insert(step.id.as_str()) {
            return Err("شناسهٔ گام نامعتبر یا تکراری است.");
        }
        if !STEP_KINDS.contains(&step.kind.as_str()) {
            return Err("نوع یکی از گام‌ها پشتیبانی نمی‌شود.");
        }
        if step.depends_on.len() > MAX_STEPS {
            return Err("تعداد وابستگی‌های گام بیش از حد مجاز است.");
        }
        let mut dependencies = HashSet::new();
        for dependency in &step.depends_on {
            if !dependencies.insert(dependency.as_str()) {
                return Err("وابستگی تکراری در یک گام وجود دارد.");
            }
            if dependency == &step.id {
                return Err("گام نمی‌تواند به خودش وابسته باشد.");
            }
        }
    }
    for step in &input.steps {
        if step
            .depends_on
            .iter()
            .any(|dependency| !ids.contains(dependency.as_str()))
        {
            return Err("یکی از وابستگی‌ها به گام موجودی اشاره نمی‌کند.");
        }
    }

    let mut indegree: HashMap<&str, usize> = input
        .steps
        .iter()
        .map(|step| (step.id.as_str(), step.depends_on.len()))
        .collect();
    let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
    for step in &input.steps {
        for dependency in &step.depends_on {
            outgoing
                .entry(dependency.as_str())
                .or_default()
                .push(step.id.as_str());
        }
    }
    let mut queue: VecDeque<&str> = indegree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
        .collect();
    let mut visited = 0;
    while let Some(id) = queue.pop_front() {
        visited += 1;
        if let Some(next) = outgoing.get(id) {
            for target in next {
                if let Some(degree) = indegree.get_mut(target) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(target);
                    }
                }
            }
        }
    }
    if visited != input.steps.len() {
        return Err("گراف workflow چرخه دارد و قابل اجرا نیست.");
    }
    Ok(())
}

fn app_scope(db: &Connection, app_id: &str) -> Result<String, (StatusCode, Json<Value>)> {
    let owner = federation::local_server_uuid(db).map_err(db_error)?;
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL)",
        params![app_id, owner],
        |row| row.get(0),
    ).map_err(db_error)?;
    if !exists {
        return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."));
    }
    Ok(owner)
}

pub(super) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = app_scope(&db, &app_id)?;
    let mut statement = db.prepare("SELECT id,schema_version,name,trigger_kind,workflow_json,created_by,created_at,updated_at FROM application_workflow_drafts WHERE application_id=?1 AND owner_server_uuid=?2 ORDER BY updated_at DESC,id").map_err(db_error)?;
    let rows = statement.query_map(params![app_id, owner], |row| {
        let json_text: String = row.get(4)?;
        let workflow: Value = serde_json::from_str(&json_text).unwrap_or(Value::Null);
        Ok(json!({"id":row.get::<_,String>(0)?,"schema_version":row.get::<_,i64>(1)?,"name":row.get::<_,String>(2)?,"trigger":row.get::<_,String>(3)?,"workflow":workflow,"created_by":row.get::<_,String>(5)?,"created_at":row.get::<_,i64>(6)?,"updated_at":row.get::<_,i64>(7)?}))
    }).map_err(db_error)?;
    let workflows = rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
    Ok(Json(
        json!({"workflows":workflows,"execution_enabled":false,"note":"این‌ها پیش‌نویس اعتبارسنجی‌شده هستند و هیچ Job یا دستور سیستمی اجرا نمی‌کنند."}),
    ))
}

pub(super) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<WorkflowInput>,
) -> ApiResult {
    check_origin(&headers)?;
    validate(&input).map_err(|message| error(StatusCode::BAD_REQUEST, message))?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let owner = app_scope(&db, &app_id)?;
    let id = token();
    let document = serde_json::to_string(&input)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "ساختار workflow قابل ذخیره نیست."))?;
    let current = now();
    db.execute("INSERT INTO application_workflow_drafts(id,application_id,owner_server_uuid,schema_version,name,trigger_kind,workflow_json,created_by,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)", params![id, app_id, owner, input.schema_version, input.name.trim(), input.trigger, document, actor.username, current]).map_err(db_error)?;
    audit(
        &db,
        &actor.username,
        "app.workflow.draft.create",
        &format!("app={app_id};workflow={id}"),
    );
    Ok(Json(
        json!({"id":id,"workflow":input,"state":"draft","execution_enabled":false,"created_at":current}),
    ))
}

pub(super) async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((app_id, workflow_id)): Path<(String, String)>,
) -> ApiResult {
    check_origin(&headers)?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?;
    let owner = app_scope(&db, &app_id)?;
    let removed = db.execute("DELETE FROM application_workflow_drafts WHERE id=?1 AND application_id=?2 AND owner_server_uuid=?3", params![workflow_id, app_id, owner]).map_err(db_error)?;
    if removed == 0 {
        return Err(error(StatusCode::NOT_FOUND, "پیش‌نویس workflow پیدا نشد."));
    }
    audit(
        &db,
        &actor.username,
        "app.workflow.draft.delete",
        &format!("app={app_id};workflow={workflow_id}"),
    );
    Ok(Json(json!({"removed":true,"execution_enabled":false})))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(steps: Vec<WorkflowStep>) -> WorkflowInput {
        WorkflowInput {
            schema_version: 1,
            name: "Deploy preview".into(),
            trigger: "manual".into(),
            steps,
        }
    }
    fn step(id: &str, kind: &str, depends_on: &[&str]) -> WorkflowStep {
        WorkflowStep {
            id: id.into(),
            kind: kind.into(),
            depends_on: depends_on.iter().map(|item| (*item).into()).collect(),
        }
    }

    #[test]
    fn accepts_a_linear_deploy_health_plan() {
        assert!(validate(&sample(vec![
            step("deploy", "deploy", &[]),
            step("health", "health_check", &["deploy"])
        ]))
        .is_ok());
    }
    #[test]
    fn rejects_cycles_duplicate_ids_and_unknown_dependencies() {
        assert!(validate(&sample(vec![
            step("a", "deploy", &["b"]),
            step("b", "health_check", &["a"])
        ]))
        .is_err());
        assert!(validate(&sample(vec![
            step("a", "deploy", &[]),
            step("a", "notify", &[])
        ]))
        .is_err());
        assert!(validate(&sample(vec![step("a", "deploy", &["missing"])])).is_err());
    }
    #[test]
    fn rejects_unsupported_triggers_steps_and_oversized_graphs() {
        let mut input = sample(vec![step("a", "shell", &[])]);
        assert!(validate(&input).is_err());
        input = sample(vec![step("a", "deploy", &[])]);
        input.trigger = "webhook".into();
        assert!(validate(&input).is_err());
        input = sample(
            (0..=MAX_STEPS)
                .map(|index| step(&format!("s{index}"), "notify", &[]))
                .collect(),
        );
        assert!(validate(&input).is_err());
    }

    #[test]
    fn draft_schema_migration_is_idempotent_and_foreign_key_scoped() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE applications(id TEXT PRIMARY KEY); INSERT INTO applications VALUES('app-a');").unwrap();
        migrate(&db).unwrap();
        db.execute("INSERT INTO application_workflow_drafts(id,application_id,owner_server_uuid,schema_version,name,trigger_kind,workflow_json,created_by,created_at,updated_at) VALUES('draft-a','app-a','server-a',1,'Draft','manual','{}','admin',1,1)", []).unwrap();
        migrate(&db).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM application_workflow_drafts WHERE application_id='app-a'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert!(db.execute("INSERT INTO application_workflow_drafts(id,application_id,owner_server_uuid,schema_version,name,trigger_kind,workflow_json,created_by,created_at,updated_at) VALUES('draft-b','missing-app','server-a',1,'Draft','manual','{}','admin',1,1)", []).is_err());
    }
}
