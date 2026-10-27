use super::*;
use std::{
    fs,
    path::{Component, Path as FsPath},
};

const MAX_FILES: usize = 12_000;
const MAX_DIRECTORY_ENTRIES: usize = 20_000;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_SCAN_DEPTH: usize = 8;
const FRAMEWORKS: &[&str] = &[
    "auto", "laravel", "react", "vue", "vite", "next", "nuxt", "node", "python", "static", "custom",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AnalyzeRequest {
    #[serde(default)]
    subdirectory: String,
    #[serde(default)]
    framework_override: Option<String>,
    #[serde(default)]
    build_mode: Option<String>,
}

fn valid_relative_dir(value: &str) -> bool {
    value.len() <= 256
        && !value.contains(['\\', '\0'])
        && (value.is_empty()
            || FsPath::new(value)
                .components()
                .all(|part| matches!(part, Component::Normal(_))))
}

fn names_for(root: &FsPath) -> Result<std::collections::HashSet<String>, String> {
    let mut names = std::collections::HashSet::new();
    let mut count = 0usize;
    let mut entries_seen = 0usize;
    fn visit(
        root: &FsPath,
        directory: &FsPath,
        depth: usize,
        count: &mut usize,
        entries_seen: &mut usize,
        names: &mut std::collections::HashSet<String>,
    ) -> Result<(), String> {
        if depth > MAX_SCAN_DEPTH {
            return Ok(());
        }
        let entries = fs::read_dir(directory)
            .map_err(|_| "سورس release هنگام اسکن قابل‌خواندن نبود.".to_string())?;
        for entry in entries {
            *entries_seen += 1;
            if *entries_seen > MAX_DIRECTORY_ENTRIES {
                return Err("تعداد ورودی‌های release از حد مجاز تحلیل بیشتر است.".into());
            }
            let entry = entry.map_err(|_| "یکی از ورودی‌های سورس خوانده نشد.".to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if matches!(
                name.as_str(),
                ".git" | "node_modules" | "vendor" | ".next" | ".nuxt"
            ) {
                continue;
            }
            let kind = entry
                .file_type()
                .map_err(|_| "نوع یکی از فایل‌های سورس خوانده نشد.".to_string())?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_file() {
                *count += 1;
                if *count > MAX_FILES {
                    return Err("تعداد فایل‌های release از حد مجاز تحلیل بیشتر است.".into());
                }
                if let Ok(relative) = entry.path().strip_prefix(root) {
                    names.insert(relative.to_string_lossy().replace('\\', "/"));
                }
            } else if kind.is_dir() {
                visit(root, &entry.path(), depth + 1, count, entries_seen, names)?;
            }
        }
        Ok(())
    }
    visit(root, root, 0, &mut count, &mut entries_seen, &mut names)?;
    Ok(names)
}

fn has(names: &std::collections::HashSet<String>, file: &str) -> bool {
    names.contains(file)
}

fn json_file(path: &FsPath) -> serde_json::Value {
    fs::metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file() && metadata.len() <= MAX_MANIFEST_BYTES)
        .and_then(|_| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(serde_json::Value::Null)
}

fn detect(root: &FsPath, names: &std::collections::HashSet<String>) -> serde_json::Value {
    let package = json_file(&root.join("package.json"));
    let composer = json_file(&root.join("composer.json"));
    let dependencies = package
        .get("dependencies")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let dev_dependencies = package
        .get("devDependencies")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let package_has =
        |name: &str| dependencies.contains_key(name) || dev_dependencies.contains_key(name);
    let mut candidates: Vec<(&str, u8, &str)> = Vec::new();
    if has(names, "artisan") && composer["require"]["laravel/framework"].is_string() {
        candidates.push(("laravel", 99, "artisan و وابستگی Laravel پیدا شدند"));
    }
    if package_has("next") || names.iter().any(|name| name.starts_with("next.config.")) {
        candidates.push(("next", 97, "وابستگی یا تنظیمات Next.js پیدا شد"));
    }
    if package_has("nuxt") || names.iter().any(|name| name.starts_with("nuxt.config.")) {
        candidates.push(("nuxt", 97, "وابستگی یا تنظیمات Nuxt پیدا شد"));
    }
    if package_has("@nestjs/core") || has(names, "nest-cli.json") {
        candidates.push(("node", 92, "ساختار NestJS پیدا شد"));
    }
    if has(names, "package.json") {
        let scripts = package.get("scripts").and_then(Value::as_object);
        if package_has("vite") && package_has("react") {
            candidates.push(("react", 94, "React و Vite در manifest شناسایی شدند"));
        } else if package_has("vite") && package_has("vue") {
            candidates.push(("vue", 94, "Vue و Vite در manifest شناسایی شدند"));
        } else if package_has("vite") || names.iter().any(|name| name.starts_with("vite.config.")) {
            candidates.push(("vite", 88, "تنظیمات Vite پیدا شد"));
        } else if package_has("express")
            || package_has("fastify")
            || package_has("koa")
            || scripts.is_some_and(|scripts| scripts.contains_key("start"))
        {
            candidates.push(("node", 78, "پروژهٔ Node.js و نقطهٔ شروع احتمالی پیدا شد"));
        }
    }
    if has(names, "manage.py") || has(names, "requirements.txt") || has(names, "pyproject.toml") {
        candidates.push((
            "python",
            if has(names, "manage.py") { 92 } else { 72 },
            "فایل‌های ورودی Python پیدا شدند",
        ));
    }
    if has(names, "Dockerfile") || has(names, "compose.yaml") || has(names, "docker-compose.yml") {
        candidates.push((
            "custom",
            70,
            "تعریف Docker/Compose پیدا شد؛ بررسی دستی لازم است",
        ));
    }
    if candidates.is_empty() && has(names, "index.html") {
        candidates.push(("static", 70, "index.html پیدا شد؛ فایل استاتیک احتمالی است"));
    }
    candidates.sort_by(|left, right| right.1.cmp(&left.1));
    let selected = candidates.first().copied();
    let ambiguous = candidates
        .get(1)
        .is_some_and(|second| selected.is_some_and(|first| first.1 - second.1 < 15));
    let framework = selected.map(|item| item.0).unwrap_or("custom");
    let confidence = selected.map(|item| item.1).unwrap_or(0);
    let is_compose = has(names, "compose.yaml") || has(names, "docker-compose.yml");
    let is_docker = is_compose || has(names, "Dockerfile");
    let mode = if is_compose {
        "compose_review"
    } else if is_docker {
        "dockerfile_review"
    } else {
        "host_runner_review"
    };
    let commands = match framework {
        "laravel" => vec!["composer install --no-dev --prefer-dist --no-interaction"],
        "node" | "next" | "nuxt" | "react" | "vue" | "vite" => {
            if has(names, "pnpm-lock.yaml") {
                vec!["pnpm install --frozen-lockfile", "pnpm run build"]
            } else if has(names, "yarn.lock") {
                vec!["yarn install --immutable", "yarn build"]
            } else if has(names, "package-lock.json") {
                vec!["npm ci", "npm run build"]
            } else {
                Vec::new()
            }
        }
        "python" => {
            vec!["Review requirements.txt or pyproject.toml before selecting an isolated runner"]
        }
        _ => Vec::new(),
    };
    let mut warnings = Vec::new();
    if ambiguous {
        warnings.push("چند چارچوب نزدیک به هم شناسایی شد؛ انتخاب را دستی تأیید کن.");
    }
    if commands.is_empty() {
        warnings.push("دستور ساخت خودکار تعیین نشد؛ پیش از Deploy تنظیمات را بازبینی کن.");
    }
    if is_docker {
        warnings.push("این Plan فقط تحلیل است؛ Dockerfile و Compose تا تأیید مدیر اجرا نمی‌شوند.");
    }
    if candidates.is_empty() {
        warnings.push("ساختار شناخته‌شده‌ای پیدا نشد؛ پروژه را به‌صورت Custom بررسی کن.");
    }
    json!({
        "detector": "arg-buildplan-v1", "framework": framework, "confidence": confidence,
        "ambiguous": ambiguous, "candidates": candidates.iter().map(|(name,score,reason)| json!({"framework":name,"confidence":score,"reason":reason})).collect::<Vec<_>>(),
        "build_mode": mode, "working_directory": ".", "commands": commands,
        "artifact": if framework == "laravel" { "release source" } else if matches!(framework,"react"|"vue"|"vite") { "dist" } else { "runtime" },
        "port": if matches!(framework,"node"|"next"|"nuxt") { json!(3000) } else { Value::Null },
        "healthcheck": if matches!(framework,"node"|"next"|"nuxt") { "/" } else { "" },
        "warnings": warnings, "executes_commands": false
    })
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS app_build_plans (id TEXT PRIMARY KEY, application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE, release_id TEXT NOT NULL, source_commit TEXT, subdirectory TEXT NOT NULL DEFAULT '', plan_json TEXT NOT NULL, created_by TEXT NOT NULL, created_at INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS app_build_plans_app ON app_build_plans(application_id,created_at DESC);")
}

fn error(status: StatusCode, message: &str) -> (StatusCode, Json<Value>) {
    super::error(status, message)
}

pub(super) async fn analyze(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<AnalyzeRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_relative_dir(&input.subdirectory) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "مسیر زیرپوشه باید نسبی و بدون traversal باشد.",
        ));
    }
    if input
        .framework_override
        .as_ref()
        .is_some_and(|value| !FRAMEWORKS.contains(&value.as_str()))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Framework انتخاب‌شده پشتیبانی نمی‌شود.",
        ));
    }
    if input
        .build_mode
        .as_ref()
        .is_some_and(|value| !["auto", "host", "dockerfile", "compose"].contains(&value.as_str()))
    {
        return Err(error(StatusCode::BAD_REQUEST, "حالت ساخت نامعتبر است."));
    }
    let (actor, release_id, source_commit, release_path) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let row = db.query_row("SELECT r.id,r.source_commit,r.path FROM deployment_releases r JOIN applications a ON a.id=r.application_id WHERE r.application_id=?1 AND a.owner_server_uuid=?2 AND a.deleted_at IS NULL AND r.state='active' ORDER BY r.activated_at DESC LIMIT 1", params![app_id,owner], |row| Ok((row.get::<_,String>(0)?,row.get::<_,Option<String>>(1)?,row.get::<_,String>(2)?))).optional().map_err(db_error)?;
        let Some(row) = row else {
            return Err(error(
                StatusCode::NOT_FOUND,
                "برای این اپ release فعال پیدا نشد؛ ابتدا یک Deploy موفق لازم است.",
            ));
        };
        (actor, row.0, row.1, row.2)
    };
    let root = FsPath::new(&release_path).canonicalize().map_err(|_| {
        error(
            StatusCode::NOT_FOUND,
            "مسیر release فعال روی دیسک در دسترس نیست.",
        )
    })?;
    let app_root = deploy::deployment_root()
        .join("apps")
        .join(&app_id)
        .canonicalize()
        .map_err(|_| error(StatusCode::NOT_FOUND, "مسیر اپلیکیشن معتبر نیست."))?;
    if !root.starts_with(&app_root) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "release خارج از مسیر مالکیت اپ قرار دارد.",
        ));
    }
    let target = if input.subdirectory.is_empty() {
        root.clone()
    } else {
        root.join(&input.subdirectory)
    };
    let target = target.canonicalize().map_err(|_| {
        error(
            StatusCode::NOT_FOUND,
            "زیرپوشهٔ انتخاب‌شده در release وجود ندارد.",
        )
    })?;
    if !target.starts_with(&root) || !target.is_dir() {
        return Err(error(
            StatusCode::FORBIDDEN,
            "زیرپوشه خارج از release است یا پوشه نیست.",
        ));
    }
    let names = names_for(&target).map_err(|message| error(StatusCode::BAD_REQUEST, &message))?;
    let mut plan = detect(&target, &names);
    if let Some(framework) = input.framework_override.as_deref() {
        plan["framework"] = json!(framework);
        plan["override"] = json!(true);
    }
    if let Some(mode) = input.build_mode.as_deref().filter(|mode| *mode != "auto") {
        plan["build_mode"] = json!(mode);
        plan["mode_override"] = json!(true);
    }
    let id = format!("{:032x}", rand::random::<u128>());
    let created_at = now();
    let db = state.lock().unwrap();
    db.execute("INSERT INTO app_build_plans(id,application_id,release_id,source_commit,subdirectory,plan_json,created_by,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![id,app_id,release_id,source_commit,input.subdirectory,serde_json::to_string(&plan).unwrap_or_else(|_|"{}".into()),actor,created_at]).map_err(db_error)?;
    audit(
        &db,
        &actor,
        "app.build_plan.analyze",
        &format!("app={app_id}; plan={id}"),
    );
    Ok(Json(
        json!({"id":id,"application_id":app_id,"release_id":release_id,"source_commit":source_commit,"created_at":created_at,"plan":plan}),
    ))
}

pub(super) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL)",params![app_id,owner],|r|r.get(0)).map_err(db_error)?;
    if !valid {
        return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."));
    }
    let mut statement=db.prepare("SELECT id,release_id,source_commit,subdirectory,plan_json,created_by,created_at FROM app_build_plans WHERE application_id=?1 ORDER BY created_at DESC LIMIT 20").map_err(db_error)?;
    let plans=statement.query_map([app_id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"release_id":r.get::<_,String>(1)?,"source_commit":r.get::<_,Option<String>>(2)?,"subdirectory":r.get::<_,String>(3)?,"plan":serde_json::from_str::<Value>(&r.get::<_,String>(4)?).unwrap_or(Value::Null),"created_by":r.get::<_,String>(5)?,"created_at":r.get::<_,i64>(6)?}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
    Ok(Json(json!({"plans":plans})))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn names(items: &[&str]) -> std::collections::HashSet<String> {
        items.iter().map(|v| v.to_string()).collect()
    }
    #[test]
    fn detects_laravel_with_high_confidence() {
        let root =
            std::env::temp_dir().join(format!("arg-laravel-buildplan-{}", rand::random::<u64>()));
        fs::create_dir_all(&root).unwrap();
        let files = names(&["artisan", "composer.json"]);
        fs::write(
            root.join("composer.json"),
            r#"{"require":{"laravel/framework":"^12"}}"#,
        )
        .unwrap();
        let plan = detect(&root, &files);
        assert_eq!(plan["framework"], "laravel");
        assert_eq!(plan["confidence"], 99);
        assert_eq!(plan["executes_commands"], false);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn detects_vite_react_and_lockfile_command() {
        let root = std::env::temp_dir().join(format!("arg-buildplan-{}", rand::random::<u64>()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"devDependencies":{"vite":"*","react":"*"}}"#,
        )
        .unwrap();
        let files = names(&["package.json", "package-lock.json"]);
        let plan = detect(&root, &files);
        assert_eq!(plan["framework"], "react");
        assert_eq!(plan["commands"][0], "npm ci");
        let mut docker_files = files;
        docker_files.insert("Dockerfile".into());
        let docker = detect(&root, &docker_files);
        assert_eq!(docker["build_mode"], "dockerfile_review");
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn rejects_traversal_subdirectories() {
        assert!(!valid_relative_dir("../etc"));
        assert!(!valid_relative_dir("a\\b"));
        assert!(valid_relative_dir("apps/site"));
    }

    #[test]
    fn oversized_manifests_are_ignored() {
        let root =
            std::env::temp_dir().join(format!("arg-buildplan-large-{}", rand::random::<u64>()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.json"),
            vec![b' '; MAX_MANIFEST_BYTES as usize + 1],
        )
        .unwrap();
        assert!(json_file(&root.join("package.json")).is_null());
        let _ = fs::remove_dir_all(root);
    }
}
