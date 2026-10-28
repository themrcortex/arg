use super::*;
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path as FsPath, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const OUTPUT_LIMIT: usize = 48 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnvironmentRequest {
    variables: HashMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BuildEnvironmentRequest {
    variables: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NodeRuntimeSettingsRequest {
    build_script: String,
    start_script: String,
    healthcheck_path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ServiceRequest {
    enabled: bool,
}

#[derive(Deserialize, Default)]
pub(super) struct NodeLogsQuery {
    limit: Option<usize>,
    after: Option<String>,
}

fn valid_journal_cursor(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value.len() <= 512
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b":=;_-".contains(&byte))
}

fn deployment_root() -> PathBuf {
    env::var_os("ARG_DEPLOY_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/arg/deployments"))
}

fn app_root(app_id: &str) -> Option<PathBuf> {
    valid_app_id(app_id).then(|| deployment_root().join("apps").join(app_id))
}

fn valid_app_id(value: &str) -> bool {
    value.len() == 36
        && value.chars().enumerate().all(|(index, character)| {
            if [8, 13, 18, 23].contains(&index) {
                character == '-'
            } else {
                character.is_ascii_hexdigit()
            }
        })
}

fn app_group(app_id: &str) -> String {
    format!("argapp-{}", &app_id[..12])
}

fn deployment_in_progress(db: &Connection, app_id: &str) -> bool {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM deployment_jobs WHERE application_id=?1 AND state IN ('queued','validating','running','verifying'))",
        [app_id],
        |row| row.get::<_, bool>(0),
    )
    .unwrap_or(true)
}

fn active_release(app_id: &str) -> Result<PathBuf, (StatusCode, Json<Value>)> {
    let root = app_root(app_id)
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "شناسهٔ اپلیکیشن نامعتبر است."))?;
    let releases = root.join("releases").canonicalize().map_err(|_| {
        error(
            StatusCode::NOT_FOUND,
            "release فعالی برای این اپلیکیشن پیدا نشد.",
        )
    })?;
    let current = root.join("current").canonicalize().map_err(|_| {
        error(
            StatusCode::NOT_FOUND,
            "ابتدا یک release برای Node.js مستقر کن.",
        )
    })?;
    if !current.starts_with(&releases) || !current.is_dir() {
        return Err(error(
            StatusCode::CONFLICT,
            "release جاری خارج از پوشهٔ امن اپلیکیشن است.",
        ));
    }
    Ok(current)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PackageManager {
    Npm,
    Pnpm,
    YarnClassic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RuntimeEntrypoint {
    PackageScript(String),
    NextCli,
    NuxtOutput,
}

impl PackageManager {
    fn label(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::YarnClassic => "yarn-classic",
        }
    }

    fn binary(self) -> &'static [&'static str] {
        match self {
            Self::Npm => &["/usr/bin/npm", "/usr/local/bin/npm"],
            Self::Pnpm => &["/usr/bin/pnpm", "/usr/local/bin/pnpm"],
            Self::YarnClassic => &["/usr/bin/yarn", "/usr/local/bin/yarn"],
        }
    }

    fn install_args(self) -> &'static [&'static str] {
        match self {
            // Lifecycle scripts execute only in run_sandboxed's isolated
            // DynamicUser unit. Skipping them breaks common native packages.
            Self::Npm => &["ci", "--no-audit", "--no-fund"],
            Self::Pnpm => &[
                "install",
                "--frozen-lockfile",
                "--store-dir=/cache/pnpm-store",
            ],
            Self::YarnClassic => &["install", "--frozen-lockfile"],
        }
    }
}

struct Manifest {
    name: Option<String>,
    version: Option<String>,
    package_manager: Option<String>,
    node_engine: Option<String>,
    scripts: Vec<String>,
    manager: Option<PackageManager>,
    detected_framework: String,
}

fn inspect_manifest(root: &FsPath) -> Result<Manifest, String> {
    let package_file = root.join("package.json");
    let metadata =
        fs::symlink_metadata(&package_file).map_err(|_| "package.json پیدا نشد.".to_owned())?;
    if !metadata.file_type().is_file() || metadata.len() > 2 * 1024 * 1024 {
        return Err("package.json باید فایل عادی و حداکثر 2 MiB باشد.".into());
    }
    let content =
        fs::read_to_string(&package_file).map_err(|_| "package.json خوانده نشد.".to_owned())?;
    let value: Value =
        serde_json::from_str(&content).map_err(|_| "ساختار package.json معتبر نیست.".to_owned())?;
    if !value.is_object() {
        return Err("ریشهٔ package.json باید یک object باشد.".into());
    }
    let scripts = value["scripts"]
        .as_object()
        .map(|scripts| {
            scripts
                .keys()
                .filter(|key| valid_script_name(key))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let manager = choose_package_manager(root, value["packageManager"].as_str())?;
    let has_package = |name: &str| {
        value["dependencies"][name].is_string()
            || value["devDependencies"][name].is_string()
            || value["optionalDependencies"][name].is_string()
    };
    let detected_framework = if has_package("next") {
        "next"
    } else if has_package("nuxt") || has_package("nitropack") {
        "nuxt"
    } else if has_package("vite") {
        "vite"
    } else if has_package("vue") {
        "vue"
    } else if has_package("react") {
        "react"
    } else {
        "node"
    };
    Ok(Manifest {
        name: safe_manifest_text(value["name"].as_str()),
        version: safe_manifest_text(value["version"].as_str()),
        package_manager: safe_manifest_text(value["packageManager"].as_str()),
        node_engine: safe_manifest_text(value["engines"]["node"].as_str()),
        scripts,
        manager,
        detected_framework: detected_framework.to_owned(),
    })
}

pub(super) fn is_static_deployment(
    framework: &str,
    release: &FsPath,
    configured_output: &str,
) -> Result<bool, String> {
    if apps::is_static_framework(framework) {
        return Ok(true);
    }
    if !apps::is_auto_framework(framework) {
        return Ok(false);
    }
    if has_package_manifest(release) {
        let manifest = inspect_manifest(release)?;
        return Ok(matches!(manifest.detected_framework.as_str(), "react" | "vue" | "vite"));
    }
    Ok(resolve_static_output_path(release, configured_output).is_ok())
}

pub(super) fn has_package_manifest(release: &FsPath) -> bool {
    fs::symlink_metadata(release.join("package.json")).is_ok_and(|metadata| {
        metadata.is_file() && !metadata.file_type().is_symlink()
    })
}

pub(super) fn static_build_available(release: &FsPath) -> Result<bool, String> {
    if !has_package_manifest(release) {
        return Ok(false);
    }
    let manifest = inspect_manifest(release)?;
    Ok(manifest.manager.is_some() && manifest.scripts.iter().any(|script| script == "build"))
}

fn valid_public_build_name(name: &str) -> bool {
    let allowed_prefix = ["VITE_", "NEXT_PUBLIC_", "NUXT_PUBLIC_", "REACT_APP_", "PUBLIC_"]
        .iter()
        .any(|prefix| name.starts_with(prefix));
    allowed_prefix
        && name.len() <= 120
        && name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        && !["SECRET", "PASSWORD", "TOKEN", "PRIVATE", "CREDENTIAL", "API_KEY", "APIKEY", "ACCESS_KEY"]
            .iter()
            .any(|marker| name.contains(marker))
}

fn validate_public_build_environment(variables: &BTreeMap<String, String>) -> Result<(), String> {
    if variables.len() > 64 {
        return Err("حداکثر ۶۴ متغیر عمومی Build مجاز است.".into());
    }
    let mut total_bytes = 0usize;
    for (name, value) in variables {
        if !valid_public_build_name(name) {
            return Err(format!("نام متغیر Build «{name}» عمومی یا معتبر نیست."));
        }
        if value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(format!("مقدار متغیر «{name}» نامعتبر است یا از ۴ KiB بیشتر است."));
        }
        total_bytes = total_bytes.saturating_add(name.len()).saturating_add(value.len());
    }
    if total_bytes > 16 * 1024 {
        return Err("مجموع متغیرهای عمومی Build نباید از ۱۶ KiB بیشتر شود.".into());
    }
    Ok(())
}

fn build_environment_from_value(value: &Value) -> Result<BTreeMap<String, String>, String> {
    let variables: BTreeMap<String, String> = serde_json::from_value(value.clone())
        .map_err(|_| "متغیرهای Build باید یک شیء از نام و مقدار متنی باشند.".to_owned())?;
    validate_public_build_environment(&variables)?;
    Ok(variables)
}

pub(super) fn parse_public_build_environment(
    value: &str,
) -> Result<BTreeMap<String, String>, String> {
    let value: Value = serde_json::from_str(value)
        .map_err(|_| "ساختار متغیرهای عمومی Build در پایگاه داده معتبر نیست.".to_owned())?;
    build_environment_from_value(&value)
}

fn safe_manifest_text(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| value.len() <= 200 && !value.chars().any(char::is_control))
        .map(str::to_owned)
}

fn valid_script_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

fn valid_configured_script(value: &str, allow_none: bool) -> bool {
    value == "auto" || (value == "none" && allow_none) || (value != "none" && valid_script_name(value))
}

fn valid_healthcheck_path(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= 256
        && !value.contains("//")
        && !value.contains('#')
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"/._~?=&-".contains(&byte))
}

fn has_nuxt_server_entrypoint(release: &FsPath) -> bool {
    let entry = release.join(".output/server/index.mjs");
    let Ok(metadata) = fs::symlink_metadata(&entry) else { return false };
    if !metadata.is_file() || metadata.file_type().is_symlink() { return false; }
    let (Ok(root), Ok(real_entry)) = (release.canonicalize(), entry.canonicalize()) else { return false };
    real_entry.starts_with(root)
}

fn has_next_cli_entrypoint(release: &FsPath) -> bool {
    let entry = release.join("node_modules/next/dist/bin/next");
    let (Ok(root), Ok(real_entry)) = (release.canonicalize(), entry.canonicalize()) else { return false };
    real_entry.starts_with(root) && real_entry.is_file()
}

fn runtime_entrypoint(manifest: &Manifest, release: &FsPath, allow_prebuild: bool, configured_script: &str) -> Option<RuntimeEntrypoint> {
    if configured_script != "auto" {
        return manifest.scripts.iter().any(|script| script == configured_script)
            .then(|| RuntimeEntrypoint::PackageScript(configured_script.to_owned()));
    }
    if manifest.scripts.iter().any(|script| script == "start") {
        Some(RuntimeEntrypoint::PackageScript("start".to_owned()))
    } else if manifest.detected_framework == "next" && (allow_prebuild || has_next_cli_entrypoint(release)) {
        Some(RuntimeEntrypoint::NextCli)
    } else if manifest.detected_framework == "nuxt" && (allow_prebuild || has_nuxt_server_entrypoint(release)) {
        Some(RuntimeEntrypoint::NuxtOutput)
    } else {
        None
    }
}

fn choose_package_manager(
    root: &FsPath,
    declaration: Option<&str>,
) -> Result<Option<PackageManager>, String> {
    let lockfiles = [
        ("package-lock.json", PackageManager::Npm),
        ("npm-shrinkwrap.json", PackageManager::Npm),
        ("pnpm-lock.yaml", PackageManager::Pnpm),
        ("yarn.lock", PackageManager::YarnClassic),
    ]
    .into_iter()
    .filter(|(name, _)| root.join(name).is_file())
    .collect::<Vec<_>>();
    if lockfiles.len() > 1 {
        return Err(
            "چند lockfile متفاوت پیدا شد؛ فقط lockfile مدیر بستهٔ انتخابی را نگه‌دار.".into(),
        );
    }
    let Some((_, detected)) = lockfiles.first() else {
        return Ok(None);
    };
    if let Some(declaration) = declaration {
        if declaration.len() > 80 || declaration.contains(['\n', '\r', '\0']) {
            return Err("مقدار packageManager نامعتبر است.".into());
        }
        let declared = declaration.split('@').next().unwrap_or_default();
        let matches = matches!(
            (declared, detected),
            ("npm", PackageManager::Npm)
                | ("pnpm", PackageManager::Pnpm)
                | ("yarn", PackageManager::YarnClassic)
        );
        if !matches {
            return Err("packageManager با lockfile موجود هم‌خوانی ندارد.".into());
        }
        if declared == "yarn" {
            let version = declaration
                .rsplit_once('@')
                .map(|(_, version)| version)
                .unwrap_or_default();
            if version
                .split('.')
                .next()
                .and_then(|major| major.parse::<u32>().ok())
                .is_some_and(|major| major >= 2)
            {
                return Err("فعلاً فقط Yarn Classic همراه yarn.lock پشتیبانی می‌شود؛ Yarn Berry باید در فاز سازگاری جداگانه اضافه شود.".into());
            }
        }
    }
    Ok(Some(*detected))
}

fn inspection_json(
    root: &FsPath,
    app_id: &str,
    runtime_version: &str,
    environment_keys: Vec<String>,
    static_site: bool,
    configured_output: &str,
) -> Value {
    if static_site {
        return match resolve_static_output_path(root, configured_output) {
            Ok(output) => json!({"framework":"static","detected_framework":inspect_manifest(root).map(|manifest| manifest.detected_framework).unwrap_or_else(|_|"static".to_owned()),"state":"ready","release_path":root,"static_output_path":output,"environment_keys":[],"scripts":[],"message":"خروجی استاتیک معتبر است؛ Nginx آن را از current منتشر می‌کند و سرویس Runtime جداگانه‌ای اجرا نمی‌شود."}),
            Err(message) => json!({"framework":"static","detected_framework":inspect_manifest(root).map(|manifest| manifest.detected_framework).unwrap_or_else(|_|"static".to_owned()),"state":"static_output_missing","release_path":root,"static_output_path":configured_output,"environment_keys":[],"scripts":[],"message":message}),
        };
    }
    let Ok(manifest) = inspect_manifest(root) else {
        return json!({"framework":"node","state":"manifest_missing_or_invalid","release_path":root,"message":"package.json پیدا نشد یا ساختار آن معتبر نیست.","scripts":[],"environment_keys":[]});
    };
    let node = find_node_binary(runtime_version);
    let runtime_matches = node
        .as_ref()
        .is_some_and(|(_, version)| runtime_version == "system" || runtime_version == version);
    let manager_binary = manifest.manager.and_then(find_package_manager_binary);
    let dependencies_installed = root.join("node_modules").is_dir();
    let state = if manifest.manager.is_none() {
        "lockfile_missing"
    } else if !runtime_matches {
        "runtime_missing_or_mismatched"
    } else if manager_binary.is_none() {
        "package_manager_missing"
    } else if !dependencies_installed {
        "dependencies_missing"
    } else if static_site && !manifest.scripts.iter().any(|script| script == "build") {
        "build_script_missing"
    } else if !static_site && runtime_entrypoint(&manifest, root, false, "auto").is_none() {
        "start_entrypoint_missing"
    } else {
        "ready"
    };
    let service_unit = node_unit_name(app_id);
    json!({
        "framework":"node",
        "detected_framework":manifest.detected_framework,
        "state":state,
        "release_path":root,
        "node_version":node.map(|(_, version)| version),
        "requested_node_version":runtime_version,
        "package_name":manifest.name,
        "package_version":manifest.version,
        "package_manager":manifest.manager.map(PackageManager::label),
        "package_manager_declaration":manifest.package_manager,
        "node_engine":manifest.node_engine,
        "scripts":manifest.scripts,
        "has_lockfile":manifest.manager.is_some(),
        "dependencies_installed":dependencies_installed,
        "environment_keys":environment_keys,
        "service_unit":service_unit,
        "message":match state {
            "lockfile_missing" => "برای نصب قابل‌تکرار، package-lock.json، pnpm-lock.yaml یا yarn.lock لازم است.",
            "runtime_missing_or_mismatched" => "نسخهٔ Node درخواستی نصب نیست یا با Runtime این VPS تطبیق ندارد.",
            "package_manager_missing" => "مدیر بستهٔ انتخابی نصب نیست؛ Runtime Manager را بررسی کن.",
            "dependencies_missing" => "node_modules در release جاری موجود نیست؛ ابتدا deploy را اجرا کن تا وابستگی‌ها با lockfile نصب شوند.",
            "start_entrypoint_missing" => "برای اجرای SSR باید scripts.start یا entrypoint استاندارد Next/Nuxt پس از build در release موجود باشد.",
            "build_script_missing" => "برای انتشار استاتیک، scripts.build در package.json تعریف شود.",
            _ => "Manifest، lockfile، Runtime و entrypoint اجرایی پیدا شدند.",
        }
    })
}

fn find_node_binary(requested: &str) -> Option<(PathBuf, String)> {
    ["/usr/bin/node", "/usr/local/bin/node", "/usr/bin/nodejs"]
        .into_iter()
        .map(PathBuf::from)
        .find_map(|path| {
            let output = Command::new(&path)
                .arg("--version")
                .stdin(Stdio::null())
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let version = String::from_utf8_lossy(&output.stdout)
                .trim()
                .strip_prefix('v')
                .unwrap_or(String::from_utf8_lossy(&output.stdout).trim())
                .to_owned();
            if !valid_version(&version) || (requested != "system" && requested != version) {
                return None;
            }
            Some((path, version))
        })
}

fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.contains('.')
        && value
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn find_package_manager_binary(manager: PackageManager) -> Option<PathBuf> {
    manager
        .binary()
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

fn node_unit_name(app_id: &str) -> String {
    format!("arg-node-{}.service", &app_id[..12])
}

fn service_record_id() -> String {
    let mut bytes = [0_u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn valid_env_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.bytes().enumerate().all(|(index, byte)| {
            if index == 0 {
                byte.is_ascii_uppercase() || byte == b'_'
            } else {
                byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'
            }
        })
        && !name.starts_with("ARG_")
}

fn read_environment(path: &FsPath) -> Vec<(String, String)> {
    if !fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.file_type().is_file() && metadata.len() <= 1024 * 1024
    }) {
        return Vec::new();
    }
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (name, raw) = line.split_once('=')?;
            let name = name.trim();
            if !valid_env_name(name) {
                return None;
            }
            let value = unescape_environment_value(raw.trim().trim_matches('"'));
            Some((name.to_owned(), value))
        })
        .collect()
}

fn unescape_environment_value(raw: &str) -> String {
    let mut value = String::with_capacity(raw.len());
    let mut characters = raw.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            match characters.next() {
                Some(escaped @ ('\\' | '"' | '$' | '`')) => value.push(escaped),
                Some(other) => {
                    value.push('\\');
                    value.push(other);
                }
                None => value.push('\\'),
            }
        } else {
            value.push(character);
        }
    }
    value
}

fn write_environment(
    path: &FsPath,
    variables: &HashMap<String, String>,
    group: &str,
) -> Result<Vec<String>, String> {
    if fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.file_type().is_symlink() || metadata.len() > 1024 * 1024)
    {
        return Err("فایل محیط symlink است یا از سقف ۱ MiB بیشتر است.".into());
    }
    let mut current = read_environment(path)
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    for (name, value) in variables {
        if !valid_env_name(name)
            || value.len() > 8192
            || value.contains(['\n', '\r', '\0'])
            || value.chars().any(char::is_control)
        {
            return Err("نام متغیر یا مقدار آن نامعتبر است؛ کلیدها باید بزرگ‌حرف لاتین باشند و مقدار خط جدید نداشته باشد.".into());
        }
        current.insert(name.clone(), value.clone());
    }
    let mut output = String::new();
    for (name, value) in &current {
        let escaped = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`");
        output.push_str(&format!("{name}=\"{escaped}\"\n"));
    }
    let parent = path.parent().ok_or("مسیر فایل محیط نامعتبر است.")?;
    fs::create_dir_all(parent).map_err(|error_value| error_value.to_string())?;
    if !fs::symlink_metadata(parent).is_ok_and(|metadata| metadata.file_type().is_dir()) {
        return Err("پوشهٔ فایل محیط معتبر نیست.".into());
    }
    let temp = parent.join(format!(
        ".node-env-{}-{}.tmp",
        std::process::id(),
        hex::encode(rand::random::<[u8; 8]>())
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|error_value| error_value.to_string())?;
    if let Err(error_value) =
        std::io::Write::write_all(&mut file, output.as_bytes()).and_then(|_| file.sync_all())
    {
        let _ = fs::remove_file(&temp);
        return Err(format!("نوشتن فایل محیط ناموفق بود: {error_value}"));
    }
    drop(file);
    if let Err(error_value) = set_group_mode(&temp, group, 0o640) {
        let _ = fs::remove_file(&temp);
        return Err(format!("دسترسی فایل محیط امن نشد: {error_value}"));
    }
    fs::rename(&temp, path).map_err(|error_value| {
        let _ = fs::remove_file(&temp);
        format!("ثبت اتمی فایل محیط ناموفق بود: {error_value}")
    })?;
    Ok(current.keys().cloned().collect())
}

pub(super) fn prepare_release(app_id: &str, release: &FsPath) -> Result<(), String> {
    if !valid_app_id(app_id) {
        return Err("شناسهٔ Node.js معتبر نیست.".into());
    }
    let manifest = inspect_manifest(release)?;
    if manifest.manager.is_none() {
        return Err("برای ساخت Node.js یک lockfile یکتا و پشتیبانی‌شده لازم است.".into());
    }
    let app = app_root(app_id).ok_or("شناسهٔ اپلیکیشن نامعتبر است.")?;
    if fs::symlink_metadata(&app).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("ریشهٔ اپلیکیشن نباید symlink باشد.".into());
    }
    let shared = app.join("shared");
    fs::create_dir_all(&shared)
        .map_err(|error_value| format!("پوشهٔ پایدار Node.js ساخته نشد: {error_value}"))?;
    if fs::symlink_metadata(&shared).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("shared نباید symlink باشد.".into());
    }
    let environment = shared.join(".env");
    if fs::symlink_metadata(&release.join(".env"))
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(".env در منبع Node.js نباید symlink باشد.".into());
    }
    if !environment.exists() {
        if release.join(".env").is_file() {
            fs::rename(release.join(".env"), &environment)
                .map_err(|error_value| format!("انتقال .env به shared ناموفق شد: {error_value}"))?;
        } else {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&environment)
                .map_err(|error_value| format!("فایل محیط پایدار ساخته نشد: {error_value}"))?;
        }
    }
    if !fs::symlink_metadata(&environment).is_ok_and(|metadata| metadata.file_type().is_file()) {
        return Err("فایل محیط پایدار باید فایل معمولی باشد.".into());
    }
    fs::set_permissions(&environment, fs::Permissions::from_mode(0o600))
        .map_err(|error_value| error_value.to_string())?;
    let env_link = release.join(".env");
    if env_link.exists() {
        fs::remove_file(&env_link).map_err(|error_value| error_value.to_string())?;
    }
    fs::create_dir_all(shared.join("data"))
        .map_err(|error_value| format!("مسیر دادهٔ پایدار ساخته نشد: {error_value}"))?;
    Ok(())
}

pub(super) fn activate_release(release: &FsPath) -> Result<(), String> {
    let metadata = fs::symlink_metadata(release).map_err(|error_value| error_value.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Node release must be a regular directory".into());
    }
    let app = release.parent().and_then(FsPath::parent).ok_or("Release path is invalid")?;
    if !release.starts_with(app.join("releases")) {
        return Err("Node release is outside its managed release directory".into());
    }
    let environment = app.join("shared/.env");
    let env_meta = fs::symlink_metadata(&environment).map_err(|error_value| error_value.to_string())?;
    if !env_meta.file_type().is_file() {
        return Err("Persistent Node environment must be a regular file".into());
    }
    let shared_meta = fs::symlink_metadata(app.join("shared"))
        .map_err(|error_value| error_value.to_string())?;
    if shared_meta.file_type().is_symlink() || !shared_meta.file_type().is_dir() {
        return Err("Persistent Node shared directory must not be a symlink".into());
    }
    let env_link = release.join(".env");
    if fs::symlink_metadata(&env_link).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("Unexpected .env symlink in Node release".into());
    }
    if env_link.exists() {
        fs::remove_file(&env_link).map_err(|error_value| error_value.to_string())?;
    }
    std::os::unix::fs::symlink(PathBuf::from("../../shared/.env"), env_link)
        .map_err(|error_value| format!("Could not attach persistent environment: {error_value}"))
}

pub(super) async fn prepare_build(
    state: &AppState,
    app_id: &str,
    job_id: &str,
    release: &FsPath,
    build_runtime: &str,
    build_environment: &BTreeMap<String, String>,
    build_script: &str,
    start_script: &str,
) -> Result<(), String> {
    prepare_project_build(state, app_id, job_id, release, build_runtime, build_environment, build_script, start_script, true).await
}

pub(super) async fn prepare_static_build(
    state: &AppState,
    app_id: &str,
    job_id: &str,
    release: &FsPath,
    build_runtime: &str,
    build_environment: &BTreeMap<String, String>,
    build_script: &str,
) -> Result<(), String> {
    prepare_project_build(state, app_id, job_id, release, build_runtime, build_environment, build_script, "auto", false).await
}

pub(super) async fn run_pre_deploy_command(
    state: &AppState,
    app_id: &str,
    job_id: &str,
    step: &str,
    argv: Vec<String>,
) -> Result<(), String> {
    if !valid_deployment_log_step(step) {
        return Err("مرحلهٔ دستور سفارشی معتبر نیست.".into());
    }
    let log_path = create_live_build_log(job_id, step)?;
    let response = request_build_step_with_live_logs(
        state,
        job_id,
        step,
        &log_path,
        json!({"action":"node-pre-deploy-command","application_id":app_id,"job_id":job_id,"step":step,"argv":argv,"log_path":log_path}),
    ).await?;
    if response["ok"] != true {
        return Err(response["error"].as_str().unwrap_or("اجرای دستور سفارشی در sandbox شکست خورد.").to_owned());
    }
    let exit_code = response["data"]["exit_code"].as_i64().unwrap_or(1);
    if exit_code != 0 {
        let output = response["data"]["output"].as_str().unwrap_or("");
        return Err(if output.is_empty() { format!("فرمان با کد خروج {exit_code} تمام شد.") } else { format!("کد خروج: {exit_code}\n{output}") });
    }
    Ok(())
}

fn valid_deployment_log_step(step: &str) -> bool {
    matches!(step, "install" | "build") || (1..=8).any(|number| step == format!("pre-deploy-{number}"))
}

pub(super) fn resolve_static_output_path(
    release: &FsPath,
    configured: &str,
) -> Result<String, String> {
    let manifest = if has_package_manifest(release) {
        Some(inspect_manifest(release)?)
    } else {
        None
    };
    let candidates: Vec<&str> = if configured != "auto" {
        vec![configured]
    } else if let Some(manifest) = manifest.as_ref() {
        match manifest.detected_framework.as_str() {
            "next" => vec!["out", "dist"],
            "nuxt" => vec![".output/public", "dist"],
            "react" => vec!["build", "dist", "out"],
            "vue" | "vite" => vec!["dist", "build", "out"],
            _ => vec!["dist", "build", "out", "public"],
        }
    } else {
        vec!["dist", "build", "out", "public", "."]
    };
    for relative in candidates {
        if !apps::valid_static_output_path(relative) || relative == "auto" { continue; }
        let candidate = if relative == "." { release.to_path_buf() } else { release.join(relative) };
        let metadata = match fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() { continue; }
        let canonical_release = release.canonicalize().map_err(|error| error.to_string())?;
        let canonical_output = candidate.canonicalize().map_err(|error| error.to_string())?;
        if !canonical_output.starts_with(&canonical_release) { continue; }
        let index = canonical_output.join("index.html");
        if !fs::symlink_metadata(&index).is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink()) { continue; }
        return Ok(relative.to_owned());
    }
    Err(if configured == "auto" {
        "خروجی خودکار پیدا نشد؛ انتظار می‌رود dist، build، out یا .output/public شامل index.html باشد. مسیر خروجی را در مشخصات اپ تنظیم کن.".into()
    } else {
        format!("مسیر خروجی «{configured}» زیر release جاری پیدا نشد یا index.html ندارد.")
    })
}

async fn prepare_project_build(
    state: &AppState,
    app_id: &str,
    job_id: &str,
    release: &FsPath,
    build_runtime: &str,
    build_environment: &BTreeMap<String, String>,
    build_script: &str,
    start_script: &str,
    require_start: bool,
) -> Result<(), String> {
    let manifest = inspect_manifest(release)?;
    let manager = manifest.manager.ok_or("lockfile انتخابی پیدا نشد.")?;
    if require_start && runtime_entrypoint(&manifest, release, true, start_script).is_none() {
        return Err(if start_script == "auto" { "برای برنامهٔ Node.js، scripts.start در package.json تعریف نشده است." } else { "اسکریپت Runtime انتخاب‌شده در package.json پیدا نشد." }.into());
    }
    if !valid_configured_script(build_script, true) {
        return Err("نام scripts.build معتبر نیست.".into());
    }
    let selected_build = if build_script == "auto" {
        manifest.scripts.iter().any(|script| script == "build").then_some("build")
    } else if build_script == "none" {
        None
    } else {
        manifest.scripts.iter().any(|script| script == build_script).then_some(build_script)
    };
    if build_script != "auto" && build_script != "none" && selected_build.is_none() {
        return Err("اسکریپت Build انتخاب‌شده در package.json پیدا نشد.".into());
    }
    if !require_start && selected_build.is_none() {
        return Err("برای انتشار استاتیک باید یک scripts.build انتخاب شود.".into());
    }
    if find_node_binary(build_runtime).is_none() {
        return Err(format!(
            "Node.js {} نصب نیست یا نسخهٔ دقیق آن پیدا نشد.",
            build_runtime
        ));
    }
    if find_package_manager_binary(manager).is_none() {
        return Err(format!(
            "{} نصب نیست؛ از Runtime Manager نصبش کن.",
            manager.label()
        ));
    }
    for step in ["install", "build"] {
        let script = if step == "install" { None } else { selected_build };
        if step == "build" && script.is_none() {
            continue;
        }
        let progress_message = if step == "install" {
            "نصب وابستگی‌ها با lockfile انتخاب‌شده در sandbox ایزوله اجرا می‌شود.".to_owned()
        } else {
            format!("اسکریپت package.json با نام {} در sandbox ایزوله اجرا می‌شود.", script.unwrap_or("نامشخص"))
        };
        super::deploy::progress(
            state,
            job_id,
            "running",
            if step == "install" { "node-install" } else { "node-build-script" },
            &progress_message,
        );
        let environment = if step == "build" { json!(build_environment) } else { json!({}) };
        let log_path = create_live_build_log(job_id, step)?;
        let result = request_build_step_with_live_logs(
            state,
            job_id,
            step,
            &log_path,
            json!({"action":"node-build-step","application_id":app_id,"job_id":job_id,"step":step,"script":script,"node_version":build_runtime,"package_manager":manager.label(),"build_environment":environment,"log_path":log_path}),
        ).await?;
        super::deploy::record_output(state, job_id, step, &format!("Runner finished {step}; exit code {}.", result["data"]["exit_code"].as_i64().unwrap_or(-1)));
        if result["ok"] != true {
            return Err(result["error"]
                .as_str()
                .unwrap_or("Node build step failed")
                .to_owned());
        }
        if result["data"]["exit_code"].as_i64().unwrap_or(1) != 0 {
            let output = result["data"]["output"].as_str().unwrap_or("");
            return Err(if output.is_empty() {
                format!(
                    "اجرای {} ناموفق بود.",
                    if step == "install" {
                        "نصب وابستگی‌ها"
                    } else {
                        "build"
                    }
                )
            } else {
                format!(
                    "مرحلهٔ {} ناموفق بود: {}",
                    step,
                    output.lines().take(12).collect::<Vec<_>>().join("\n")
                )
            });
        }
    }
    Ok(())
}

fn create_live_build_log(job_id: &str, step: &str) -> Result<PathBuf, String> {
    if !valid_app_id(job_id) || !valid_deployment_log_step(step) {
        return Err("شناسه یا مرحلهٔ فایل لاگ Build معتبر نیست.".into());
    }
    let deployment = deployment_root();
    let deployment_metadata = fs::symlink_metadata(&deployment)
        .map_err(|error| format!("ریشهٔ استقرار برای لاگ Build پیدا نشد: {error}"))?;
    if !deployment_metadata.is_dir() || deployment_metadata.file_type().is_symlink() {
        return Err("ریشهٔ استقرار برای لاگ Build باید پوشهٔ واقعی باشد.".into());
    }
    let log_directory = deployment.join("job-logs");
    match fs::symlink_metadata(&log_directory) {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => return Err("پوشهٔ لاگ‌های Build معتبر نیست.".into()),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&log_directory).map_err(|error| format!("پوشهٔ لاگ Build ساخته نشد: {error}"))?,
        Err(error) => return Err(format!("پوشهٔ لاگ Build بررسی نشد: {error}")),
    }
    let canonical_deployment = deployment.canonicalize().map_err(|error| error.to_string())?;
    let canonical_logs = log_directory.canonicalize().map_err(|error| error.to_string())?;
    if canonical_logs.parent() != Some(canonical_deployment.as_path()) {
        return Err("پوشهٔ لاگ Build از ریشهٔ استقرار خارج شده است.".into());
    }
    let path = canonical_logs.join(format!("{job_id}-{step}.log"));
    OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path)
        .map_err(|error| format!("فایل لاگ Build امن ساخته نشد: {error}"))?;
    Ok(path)
}

fn drain_live_build_log(path: &FsPath, offset: &mut u64, pending: &mut Vec<u8>, final_read: bool, state: &AppState, job_id: &str, step: &str) -> bool {
    let mut file = match fs::File::open(path) { Ok(file) => file, Err(_) => return false };
    if file.seek(SeekFrom::Start(*offset)).is_err() { return false; }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() { return false; }
    *offset = offset.saturating_add(bytes.len() as u64);
    pending.extend_from_slice(&bytes);
    let mut lines = Vec::new();
    while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
        let line = pending.drain(..=end).collect::<Vec<_>>();
        let message = String::from_utf8_lossy(&line).trim().to_owned();
        if !message.is_empty() { lines.push(message); }
    }
    if final_read && !pending.is_empty() {
        let line = std::mem::take(pending);
        let message = String::from_utf8_lossy(&line).trim().to_owned();
        if !message.is_empty() { lines.push(message); }
    }
    if lines.is_empty() { false } else {
        super::deploy::record_output(state, job_id, step, &lines.join("\n"));
        true
    }
}

async fn request_build_step_with_live_logs(state: &AppState, job_id: &str, step: &str, log_path: &FsPath, request: Value) -> Result<Value, String> {
    let mut offset = 0_u64;
    let mut pending = Vec::new();
    let mut streamed = false;
    let request = privileged_request_async(request);
    tokio::pin!(request);
    loop {
        tokio::select! {
            response = &mut request => {
                streamed |= drain_live_build_log(log_path, &mut offset, &mut pending, true, state, job_id, step);
                let _ = fs::remove_file(log_path);
                let response = response.map_err(|(_, body)| body["error"].as_str().unwrap_or("Node build helper unavailable").to_owned())?;
                if !streamed {
                    super::deploy::record_output(state, job_id, step, response["data"]["output"].as_str().unwrap_or(""));
                }
                return Ok(response);
            }
            _ = tokio::time::sleep(Duration::from_millis(400)) => {
                streamed |= drain_live_build_log(log_path, &mut offset, &mut pending, false, state, job_id, step);
            }
        }
    }
}

pub(super) async fn inspect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let (framework, runtime_version, output_path, spa_fallback) = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let (framework, runtime_version, output_path, spa_fallback): (String, String, String, bool) = db
            .query_row(
                "SELECT framework,build_runtime_version,static_output_path,static_spa_fallback FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
                params![app_id, owner],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."))?;
        if !apps::is_node_runtime_framework(&framework) && !apps::is_static_framework(&framework) && !apps::is_auto_framework(&framework) {
            return Err(error(StatusCode::CONFLICT, "این اپلیکیشن Runner وب پشتیبانی‌شده ندارد."));
        }
        (framework, runtime_version, output_path, spa_fallback)
    };
    let release = active_release(&app_id)?;
    let static_site = is_static_deployment(&framework, &release, &output_path)
        .map_err(|message| error(StatusCode::CONFLICT, &message))?;
    let environment_keys = if static_site {
        Vec::new()
    } else {
        let environment = privileged_request_async(
            json!({"action":"node-environment-keys","application_id":app_id}),
        )
        .await?;
        if environment["ok"] != true {
            return Err(error(StatusCode::BAD_GATEWAY, environment["error"].as_str().unwrap_or("کلیدهای فایل محیط خوانده نشد.")));
        }
        environment["data"]["keys"]
            .as_array()
            .map(|keys| keys.iter().filter_map(Value::as_str).map(str::to_owned).collect())
            .unwrap_or_default()
    };
    Ok(Json(
        json!({"inspection":inspection_json(&release,&app_id,&runtime_version,environment_keys,static_site,&output_path),"static_output_path":output_path,"static_spa_fallback":spa_fallback}),
    ))
}

fn runtime_settings_for_app(db: &Connection, app_id: &str) -> rusqlite::Result<(String, String)> {
    db.query_row(
        "SELECT node_start_script,node_healthcheck_path FROM applications WHERE id=?1",
        [app_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
}

pub(super) async fn get_runtime_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let (build_script, start_script, healthcheck_path) = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        db.query_row(
            "SELECT node_build_script,node_start_script,node_healthcheck_path FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
            params![app_id, owner],
            |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?)),
        ).optional().map_err(db_error)?.ok_or_else(|| error(StatusCode::NOT_FOUND,"اپلیکیشن پیدا نشد."))?
    };
    let scripts = active_release(&app_id).ok().and_then(|release| inspect_manifest(&release).ok()).map(|manifest| manifest.scripts).unwrap_or_default();
    Ok(Json(json!({"build_script":build_script,"start_script":start_script,"healthcheck_path":healthcheck_path,"available_scripts":scripts,"arbitrary_shell":false})))
}

pub(super) async fn save_runtime_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<NodeRuntimeSettingsRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_configured_script(&input.build_script, true)
        || !valid_configured_script(&input.start_script, false)
        || !valid_healthcheck_path(&input.healthcheck_path)
    {
        return Err(error(StatusCode::BAD_REQUEST,"نام scripts یا مسیر Health Check معتبر نیست."));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let framework: String = db.query_row(
        "SELECT framework FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
        params![app_id,owner], |row| row.get(0),
    ).optional().map_err(db_error)?.ok_or_else(|| error(StatusCode::NOT_FOUND,"اپلیکیشن پیدا نشد."))?;
    if !apps::is_node_runtime_framework(&framework) && !apps::is_static_framework(&framework) && !apps::is_auto_framework(&framework) {
        return Err(error(StatusCode::CONFLICT,"تنظیمات Runtime فقط برای اپلیکیشن‌های Node قابل استفاده است."));
    }
    if deployment_in_progress(&db, &app_id) {
        return Err(error(StatusCode::CONFLICT,"تا پایان استقرار نمی‌توان تنظیمات Runtime را تغییر داد."));
    }
    if let Ok(release) = active_release(&app_id) {
        if has_package_manifest(&release) {
            let manifest = inspect_manifest(&release).map_err(|message| error(StatusCode::CONFLICT,&message))?;
            if input.build_script != "auto" && input.build_script != "none" && !manifest.scripts.contains(&input.build_script) {
                return Err(error(StatusCode::BAD_REQUEST,"اسکریپت Build در package.json نسخهٔ فعلی پیدا نشد."));
            }
            if input.start_script != "auto" && !manifest.scripts.contains(&input.start_script) {
                return Err(error(StatusCode::BAD_REQUEST,"اسکریپت Runtime در package.json نسخهٔ فعلی پیدا نشد."));
            }
        }
    }
    db.execute(
        "UPDATE applications SET node_build_script=?1,node_start_script=?2,node_healthcheck_path=?3,version=version+1,updated_at=?4 WHERE id=?5 AND owner_server_uuid=?6 AND deleted_at IS NULL",
        params![input.build_script,input.start_script,input.healthcheck_path,now(),app_id,owner],
    ).map_err(db_error)?;
    audit(&db,&actor,"node.runtime_settings.update",&format!("app={app_id}; build={}; start={}; healthcheck={}",input.build_script,input.start_script,input.healthcheck_path));
    Ok(Json(json!({"saved":true,"settings":{"build_script":input.build_script,"start_script":input.start_script,"healthcheck_path":input.healthcheck_path}})))
}

pub(super) async fn get_build_environment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let raw: String = db
        .query_row(
            "SELECT build_environment_json FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
            params![app_id, owner],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."))?;
    let variables = parse_public_build_environment(&raw)
        .map_err(|message| error(StatusCode::INTERNAL_SERVER_ERROR, &message))?;
    Ok(Json(json!({"variables":variables,"values_are_public":true})))
}

pub(super) async fn save_build_environment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<BuildEnvironmentRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    validate_public_build_environment(&input.variables)
        .map_err(|message| error(StatusCode::BAD_REQUEST, &message))?;
    let _guard = laravel::APP_EXECUTION_LOCK.lock().await;
    let (actor, owner) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let framework: String = db
            .query_row(
                "SELECT framework FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
                params![app_id, owner],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."))?;
        if !apps::is_node_runtime_framework(&framework)
            && !apps::is_static_framework(&framework)
            && !apps::is_auto_framework(&framework)
        {
            return Err(error(StatusCode::CONFLICT, "این قالب از Buildهای Node پشتیبانی نمی‌کند."));
        }
        if deployment_in_progress(&db, &app_id) {
            return Err(error(StatusCode::CONFLICT, "تا پایان Deploy نمی‌توان متغیرهای Build را تغییر داد."));
        }
        (actor, owner)
    };
    let encoded = serde_json::to_string(&input.variables)
        .map_err(|error_value| error(StatusCode::INTERNAL_SERVER_ERROR, &error_value.to_string()))?;
    let db = state.lock().unwrap();
    let changed = db
        .execute(
            "UPDATE applications SET build_environment_json=?1,version=version+1,updated_at=?2 WHERE id=?3 AND owner_server_uuid=?4 AND deleted_at IS NULL",
            params![encoded, now(), app_id, owner],
        )
        .map_err(db_error)?;
    if changed == 0 {
        return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."));
    }
    let keys = input.variables.keys().cloned().collect::<Vec<_>>();
    audit(
        &db,
        &actor,
        "node.build_environment.update",
        &format!("app={app_id}; keys={}", keys.join(",")),
    );
    Ok(Json(json!({"saved":true,"keys":keys,"values_are_public":true})))
}

pub(super) async fn service(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let (port, framework, output_path) = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let (framework, port, output_path): (String, Option<u16>, String) = db
            .query_row(
                "SELECT framework,listen_port,static_output_path FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
                params![app_id,owner],
                |row| Ok((row.get(0)?,row.get::<_,Option<i64>>(1)?.map(|v|v as u16),row.get(2)?)),
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."))?;
        if apps::is_static_framework(&framework) {
            return Ok(Json(json!({"service":{"unit":null,"state":"static","enabled":true},"port":null})));
        }
        if !apps::is_node_runtime_framework(&framework) && !apps::is_auto_framework(&framework) {
            return Err(error(StatusCode::CONFLICT, "این اپلیکیشن Runner وب ندارد."));
        }
        (port, framework, output_path)
    };
    if apps::is_auto_framework(&framework) {
        if let Ok(release) = active_release(&app_id) {
            if is_static_deployment(&framework, &release, &output_path).map_err(|message| error(StatusCode::CONFLICT, &message))? {
                return Ok(Json(json!({"service":{"unit":null,"state":"static","enabled":true},"port":null})));
            }
        }
    }
    let status =
        privileged_request_async(json!({"action":"node-service-status","application_id":app_id}))
            .await?;
    if status["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            status["error"]
                .as_str()
                .unwrap_or("وضعیت سرویس خوانده نشد."),
        ));
    }
    Ok(Json(json!({"service":status["data"],"port":port})))
}

pub(super) async fn set_service(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<ServiceRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    let _guard = laravel::APP_EXECUTION_LOCK.lock().await;
    let (actor, framework, runtime, port, domain, output_path, start_script, healthcheck_path) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let row:(String,String,Option<u16>,Option<String>,String)=db.query_row("SELECT framework,runtime_version,listen_port,domain,static_output_path FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",params![app_id,owner],|row|Ok((row.get(0)?,row.get(1)?,row.get::<_,Option<i64>>(2)?.map(|v|v as u16),row.get::<_,Option<String>>(3)?,row.get(4)?))).optional().map_err(db_error)?.ok_or_else(||error(StatusCode::NOT_FOUND,"اپلیکیشن پیدا نشد."))?;
        if !apps::is_node_runtime_framework(&row.0) && !apps::is_auto_framework(&row.0) {
            return Err(error(StatusCode::CONFLICT, "فقط اپ SSR دارای سرویس Node قابل روشن/خاموش‌کردن است."));
        }
        if deployment_in_progress(&db, &app_id) {
            return Err(error(
                StatusCode::CONFLICT,
                "تا پایان deploy نمی‌توان وضعیت سرویس Node را تغییر داد.",
            ));
        }
        let (start_script, healthcheck_path) = runtime_settings_for_app(&db, &app_id).map_err(db_error)?;
        (actor, row.0, row.1, row.2, row.3, row.4, start_script, healthcheck_path)
    };
    if apps::is_auto_framework(&framework) {
        let release = active_release(&app_id)?;
        if is_static_deployment("auto", &release, &output_path).map_err(|message| error(StatusCode::CONFLICT, &message))? {
            return Err(error(StatusCode::CONFLICT, "این پروژه به‌صورت Static منتشر شده و سرویس Node ندارد."));
        }
    }
    let previously_active = if input.enabled {
        let previous = privileged_request_async(
            json!({"action":"node-service-status","application_id":app_id}),
        )
        .await?;
        if previous["ok"] != true {
            return Err(error(
                StatusCode::BAD_GATEWAY,
                previous["error"].as_str().unwrap_or("وضعیت قبلی سرویس Node مشخص نشد؛ عملیات انجام نشد."),
            ));
        }
        previous["data"]["state"] == "active"
    } else {
        false
    };
    let operation = if input.enabled {
        let release = active_release(&app_id)?;
        let manifest =
            inspect_manifest(&release).map_err(|message| error(StatusCode::CONFLICT, &message))?;
        let entrypoint = runtime_entrypoint(&manifest, &release, false, &start_script);
        if manifest.manager.is_none()
            || entrypoint.is_none()
            || !release.join("node_modules").is_dir()
        {
            return Err(error(
                StatusCode::CONFLICT,
                "release جاری باید lockfile، وابستگی نصب‌شده و نقطهٔ ورود SSR معتبر داشته باشد؛ Next و Nuxt از entrypoint استاندارد خود نیز پشتیبانی می‌شوند.",
            ));
        }
        let port = port.filter(|port| *port >= 1024).ok_or_else(|| {
            error(
                StatusCode::CONFLICT,
                "برای سرویس Node یک پورت آزاد بزرگ‌تر از 1023 در مشخصات اپ ثبت کن.",
            )
        })?;
        json!({"action":"node-service-enable","application_id":app_id,"node_version":runtime,"port":port,"start_script":start_script,"healthcheck_path":healthcheck_path})
    } else {
        json!({"action":"node-service-disable","application_id":app_id})
    };
    let result = privileged_request_async(operation).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("تغییر سرویس Node ناموفق بود."),
        ));
    }
    let mut nginx_snapshot: Option<(String, String)> = None;
    if input.enabled {
        if let (Some(domain), Some(port)) = (domain.as_deref(), port) {
            let proxy = privileged_request_async(json!({"action":"node-nginx-proxy","application_id":app_id,"domain":domain,"port":port})).await;
            match proxy {
                Ok(value) if value["ok"] == true => {
                    if let (Some(name), Some(content)) = (value["data"]["site_name"].as_str(), value["data"]["content"].as_str()) {
                        nginx_snapshot = Some((name.to_owned(), content.to_owned()));
                    }
                }
                Ok(value) => {
                    let message = value["error"].as_str().unwrap_or("تنظیم reverse proxy در Nginx ناموفق بود.");
                    if !previously_active {
                        let _ = privileged_request_async(json!({"action":"node-service-disable","application_id":app_id})).await;
                    }
                    let db = state.lock().unwrap();
                    audit(&db, &actor, "node.service.proxy_failed", &format!("app={app_id}; domain={domain}; error={message}"));
                    return Err(error(StatusCode::BAD_GATEWAY, message));
                }
                Err((_, body)) => {
                    let message = body["error"].as_str().unwrap_or("ارتباط با helper تنظیم Nginx برقرار نشد.");
                    if !previously_active {
                        let _ = privileged_request_async(json!({"action":"node-service-disable","application_id":app_id})).await;
                    }
                    let db = state.lock().unwrap();
                    audit(&db, &actor, "node.service.proxy_failed", &format!("app={app_id}; domain={domain}; error={message}"));
                    return Err(error(StatusCode::BAD_GATEWAY, message));
                }
            }
        }
    }
    let db = state.lock().unwrap();
    let unit = node_unit_name(&app_id);
    let desired = if input.enabled { "running" } else { "stopped" };
    db.execute("INSERT INTO application_services(id,application_id,service_key,runtime_type,systemd_unit,desired_state,observed_state,listen_port,last_observed_at,metadata_json,created_at,updated_at) VALUES (?1,?2,'node-web','node',?3,?4,?4,?5,?6,'{}',?6,?6) ON CONFLICT(application_id,service_key) DO UPDATE SET systemd_unit=excluded.systemd_unit,desired_state=excluded.desired_state,observed_state=excluded.observed_state,listen_port=excluded.listen_port,last_observed_at=excluded.last_observed_at,updated_at=excluded.updated_at",params![service_record_id(),app_id,unit,desired,port.map(i64::from),now()]).map_err(db_error)?;
    audit(
        &db,
        &actor,
        "node.service.change",
        &format!("app={app_id}; enabled={}; unit={unit}", input.enabled),
    );
    if let Some((site_name, content)) = nginx_snapshot {
        db.execute("INSERT OR IGNORE INTO nginx_site_metadata(name,description) VALUES (?1,?2)", params![site_name, format!("Node application {app_id}")]).map_err(db_error)?;
        nginx::store_nginx_snapshot(&db, &site_name, &content, None).map_err(db_error)?;
    }
    Ok(Json(result["data"].clone()))
}

pub(super) async fn save_environment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<EnvironmentRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if input.variables.is_empty() || input.variables.len() > 100 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "بین ۱ تا ۱۰۰ متغیر ارسال کن.",
        ));
    }
    let _guard = laravel::APP_EXECUTION_LOCK.lock().await;
    let (actor, framework, output_path) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let (framework, output_path):(String,String)=db.query_row("SELECT framework,static_output_path FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",params![app_id,owner],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(db_error)?.ok_or_else(||error(StatusCode::NOT_FOUND,"اپلیکیشن پیدا نشد."))?;
        if !apps::is_node_runtime_framework(&framework) && !apps::is_auto_framework(&framework) {
            return Err(error(StatusCode::CONFLICT, "این اپلیکیشن سرویس Runtime Node ندارد."));
        }
        if deployment_in_progress(&db, &app_id) {
            return Err(error(
                StatusCode::CONFLICT,
                "تا پایان deploy فایل محیط را تغییر نده.",
            ));
        }
        (actor, framework, output_path)
    };
    if apps::is_auto_framework(&framework) {
        let release = active_release(&app_id)?;
        if is_static_deployment("auto", &release, &output_path).map_err(|message| error(StatusCode::CONFLICT, &message))? {
            return Err(error(StatusCode::CONFLICT, "این پروژه Static است و متغیر Runtime ندارد."));
        }
    }
    let result = privileged_request_async(json!({
        "action":"node-environment-save",
        "application_id":app_id,
        "variables":input.variables
    }))
    .await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"].as_str().unwrap_or("فایل محیط ذخیره نشد."),
        ));
    }
    let keys = result["data"]["keys"]
        .as_array()
        .map(|values| values.iter().filter_map(Value::as_str).map(str::to_owned).collect::<Vec<_>>())
        .unwrap_or_default();
    let db = state.lock().unwrap();
    audit(
        &db,
        &actor,
        "node.environment.update",
        &format!("app={app_id}; keys={}", keys.join(",")),
    );
    Ok(Json(json!({"saved":true,"keys":keys,"secrets_returned":false})))
}

pub(super) async fn refresh_after_deploy(
    app_id: &str,
    previous: Option<&str>,
) -> Result<bool, String> {
    let status = privileged_request_async(
        json!({"action":"node-service-status","application_id":app_id}),
    )
    .await;
    let status = match status {
        Ok(status) => status,
        Err((_, body)) => {
            let message = body["error"]
            .as_str()
            .unwrap_or("Node service status failed")
            .to_owned();
            let restored = restore_previous_release(app_id, previous).is_ok();
            return Err(format!("وضعیت سرویس Node بررسی نشد: {message}. اشاره‌گر release {}؛ چون وضعیت سرویس معلوم نیست، آن را خودکار start نکردم.", if restored { "به نسخهٔ قبلی برگشت" } else { "قابل‌بازگشت نبود" }));
        }
    };
    if status["ok"] != true {
        let message = status["error"]
            .as_str()
            .unwrap_or("Node service status failed")
            .to_owned();
        let restored = restore_previous_release(app_id, previous).is_ok();
        return Err(format!("وضعیت سرویس Node خوانده نشد: {message}. اشاره‌گر release {}.", if restored { "به نسخهٔ قبلی برگشت" } else { "بازنگشت" }));
    }
    if status["data"]["enabled"] != true && status["data"]["state"] != "active" {
        return Ok(false);
    }
    let result = privileged_request_async(
        json!({"action":"node-service-restart","application_id":app_id}),
    )
    .await;
    let result = match result {
        Ok(result) => result,
        Err((_, body)) => {
            let restart_error = body["error"]
            .as_str()
            .unwrap_or("Node service restart failed")
            .to_owned();
            let restore = restore_previous_release(app_id, previous);
            let old_service = if restore.is_ok() {
                privileged_request_async(
                    json!({"action":"node-service-restart","application_id":app_id}),
                )
                .await
            } else {
                Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":"restore failed"}))))
            };
            let recovered = restore.is_ok() && matches!(old_service, Ok(ref value) if value["ok"] == true);
            return Err(format!("راه‌اندازی release تازه شکست خورد: {restart_error}. {}", if recovered { "نسخهٔ قبلی دوباره فعال شد." } else { "بازگردانی کامل نشد؛ وضعیت current و سرویس را بررسی کن." }));
        }
    };
    if result["ok"] == true {
        return Ok(true);
    }
    let restart_error = result["error"]
        .as_str()
        .unwrap_or("Node service restart failed");
    let restore = restore_previous_release(app_id, previous);
    let old_service = privileged_request_async(
        json!({"action":"node-service-restart","application_id":app_id}),
    )
    .await;
    let detail = match (restore, old_service) {
        (Ok(()), Ok(value)) if value["ok"] == true => " نسخهٔ قبلی دوباره فعال شد.",
        (Ok(()), _) => " اشاره‌گر قبلی برگشت، اما سرویس قدیمی خودکار راه نیفتاد.",
        (Err(_), _) => " بازگردانی خودکار اشاره‌گر هم ناموفق بود؛ نسخهٔ جاری را بررسی کن.",
    };
    Err(format!("راه‌اندازی release تازه شکست خورد: {restart_error}.{detail}"))
}

fn restore_previous_release(app_id: &str, previous: Option<&str>) -> Result<(), String> {
    let root = app_root(app_id).ok_or("Application path is invalid")?;
    let current = root.join("current");
    let temp = root.join(format!(".current-rollback-{}", std::process::id()));
    let _ = fs::remove_file(&temp);
    let Some(previous) = previous else {
        if !fs::symlink_metadata(&current).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err("There is no previous release to restore".into());
        }
        return fs::remove_file(current).map_err(|error_value| error_value.to_string());
    };
    let target = FsPath::new(previous);
    let components = target.components().collect::<Vec<_>>();
    if target.is_absolute()
        || components.len() != 2
        || components[0] != std::path::Component::Normal("releases".as_ref())
        || components[1]
            .as_os_str()
            .to_str()
            .is_none_or(|id| !valid_app_id(id))
    {
        return Err("Previous release pointer is invalid".into());
    }
    let release_root = root
        .join("releases")
        .canonicalize()
        .map_err(|error_value| error_value.to_string())?;
    let release = root
        .join(target)
        .canonicalize()
        .map_err(|error_value| error_value.to_string())?;
    if !release.starts_with(&release_root) {
        return Err("Previous release escaped the managed directory".into());
    }
    std::os::unix::fs::symlink(target, &temp).map_err(|error_value| error_value.to_string())?;
    fs::rename(&temp, &current).map_err(|error_value| {
        let _ = fs::remove_file(&temp);
        error_value.to_string()
    })
}

pub(super) async fn logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Query(query): Query<NodeLogsQuery>,
) -> ApiResult {
    if query.after.as_deref().is_some_and(|cursor| !valid_journal_cursor(cursor)) {
        return Err(error(StatusCode::BAD_REQUEST, "شناسهٔ ادامهٔ لاگ معتبر نیست."));
    }
    let (framework, output_path) = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let record: Option<(String, String)> = db
            .query_row(
                "SELECT framework,static_output_path FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
                params![app_id, owner],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(db_error)?;
        if !record.as_ref().is_some_and(|(value, _)| apps::is_node_runtime_framework(value) || apps::is_auto_framework(value)) {
            return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن Node.js پیدا نشد."));
        }
        record.unwrap_or_default()
    };
    if apps::is_auto_framework(&framework) {
        let release = active_release(&app_id)?;
        if is_static_deployment("auto", &release, &output_path).map_err(|message| error(StatusCode::CONFLICT, &message))? {
            return Err(error(StatusCode::CONFLICT, "این پروژه Static است و سرویس اجرایی ندارد."));
        }
    }
    let result = privileged_request_async(
        json!({"action":"node-service-logs","application_id":app_id,"limit":query.limit.unwrap_or(100).clamp(1,500),"after":query.after}),
    )
    .await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"].as_str().unwrap_or("لاگ سرویس خوانده نشد."),
        ));
    }
    Ok(Json(result["data"].clone()))
}

pub(super) fn execute_privileged_request(action: &str, request: &Value) -> Value {
    match action {
        "node-build-step" => helper_build_step(request),
        "node-pre-deploy-command" => helper_pre_deploy_command(request),
        "node-service-enable" => helper_service_enable(request),
        "node-service-disable" => helper_service_disable(request),
        "node-service-restart" => helper_service_restart(request),
        "node-service-status" => helper_service_status(request, false),
        "node-service-logs" => helper_service_status(request, true),
        "node-environment-save" => helper_environment_save(request),
        "node-environment-keys" => helper_environment_keys(request),
        "node-environment-permissions" => helper_environment_permissions(request),
        "node-nginx-proxy" => helper_node_nginx_proxy(request),
        "node-nginx-static" => helper_node_nginx_static(request),
        _ => json!({"ok":false,"error":"Node helper operation is not allowlisted"}),
    }
}

fn helper_node_nginx_proxy(request: &Value) -> Value {
    let Some(app_id) = request["application_id"].as_str().filter(|value| valid_app_id(value)) else {
        return json!({"ok":false,"error":"Application ID is invalid"});
    };
    let Some(domain) = request["domain"].as_str() else {
        return json!({"ok":false,"error":"Application domain is missing"});
    };
    let Some(port) = request["port"].as_u64().filter(|port| (1024..=65535).contains(port)) else {
        return json!({"ok":false,"error":"Node port is invalid"});
    };
    match nginx::configure_node_proxy(app_id, domain, port as u16) {
        Ok(data) => json!({"ok":true,"data":data}),
        Err(message) => json!({"ok":false,"error":message}),
    }
}

fn helper_node_nginx_static(request: &Value) -> Value {
    let Some(app_id) = request["application_id"].as_str().filter(|value| valid_app_id(value)) else {
        return json!({"ok":false,"error":"Application ID is invalid"});
    };
    let (Some(domain), Some(output_path), Some(spa_fallback)) = (
        request["domain"].as_str(),
        request["output_path"].as_str(),
        request["spa_fallback"].as_bool(),
    ) else {
        return json!({"ok":false,"error":"Static site configuration is incomplete"});
    };
    match nginx::configure_static_site(app_id, domain, output_path, spa_fallback) {
        Ok(data) => json!({"ok":true,"data":data}),
        Err(message) => json!({"ok":false,"error":message}),
    }
}

fn helper_build_step(request: &Value) -> Value {
    let Some(app_id) = request["application_id"]
        .as_str()
        .filter(|value| valid_app_id(value))
    else {
        return json!({"ok":false,"error":"Application ID is invalid"});
    };
    let Some(job_id) = request["job_id"]
        .as_str()
        .filter(|value| valid_app_id(value))
    else {
        return json!({"ok":false,"error":"Deployment ID is invalid"});
    };
    let Some(step) = request["step"]
        .as_str()
        .filter(|value| matches!(*value, "install" | "build"))
    else {
        return json!({"ok":false,"error":"Node build step is not allowlisted"});
    };
    let Some(log_path) = request["log_path"].as_str() else {
        return json!({"ok":false,"error":"Live build log path is missing"});
    };
    let script = request["script"].as_str();
    if step == "build" && script.is_none_or(|value| !valid_script_name(value)) {
        return json!({"ok":false,"error":"Build script name is invalid"});
    }
    let Some(node_version) = request["node_version"].as_str() else {
        return json!({"ok":false,"error":"Node version is missing"});
    };
    let root = deployment_root().join("apps").join(app_id);
    let stage = root.join(format!(".staging-{job_id}"));
    if fs::symlink_metadata(&root).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return json!({"ok":false,"error":"Application root must not be a symlink"});
    }
    if !fs::symlink_metadata(root.join("shared"))
        .is_ok_and(|metadata| metadata.file_type().is_dir())
    {
        return json!({"ok":false,"error":"Application shared path must be a normal directory"});
    }
    let Ok(real_stage) = stage.canonicalize() else {
        return json!({"ok":false,"error":"Staging release does not exist"});
    };
    let Ok(real_app) = root.canonicalize() else {
        return json!({"ok":false,"error":"Application root does not exist"});
    };
    if !real_stage.starts_with(&real_app) || !real_stage.is_dir() {
        return json!({"ok":false,"error":"Staging path escaped the application root"});
    }
    let manifest = match inspect_manifest(&real_stage) {
        Ok(value) => value,
        Err(message) => return json!({"ok":false,"error":message}),
    };
    let Some(manager) = manifest.manager else {
        return json!({"ok":false,"error":"A single supported lockfile is required"});
    };
    if manager.label() != request["package_manager"].as_str().unwrap_or("") {
        return json!({"ok":false,"error":"Package manager does not match the source lockfile"});
    }
    let Some((node_binary, _)) = find_node_binary(node_version) else {
        return json!({"ok":false,"error":"Requested Node.js runtime is not installed"});
    };
    if step == "build" && !manifest.scripts.iter().any(|available| Some(available.as_str()) == script) {
        return json!({"ok":false,"error":"The selected build script does not exist in package.json"});
    }
    let build_environment = if step == "build" {
        match build_environment_from_value(&request["build_environment"]) {
            Ok(values) => values,
            Err(message) => return json!({"ok":false,"error":message}),
        }
    } else {
        BTreeMap::new()
    };
    let group = app_group(app_id);
    let build_cache = root.join("build-cache");
    if let Err(message) = ensure_group(&group)
        .and_then(|_| prepare_tree_for_build(&real_stage, &group))
        .and_then(|_| prepare_cache_for_build(&build_cache, &group))
    {
        return json!({"ok":false,"error":message});
    }
    let Some(binary) = find_package_manager_binary(manager) else {
        let _ = restore_tree_owner(&real_stage, &group);
        return json!({"ok":false,"error":format!("{} is not installed",manager.label())});
    };
    let log_file = match open_build_log_file(log_path, job_id, step) {
        Ok(file) => file,
        Err(message) => { let _ = restore_tree_owner(&real_stage, &group); return json!({"ok":false,"error":message}); }
    };
    let build_args = script.map(|name| vec!["run", name]);
    let args = if step == "install" { manager.install_args() } else { build_args.as_deref().unwrap_or(&[]) };
    let output = run_sandboxed(
        &real_stage,
        &build_cache,
        &group,
        &node_binary,
        &binary,
        args,
        &build_environment,
        app_id,
        job_id,
        step,
        log_file,
    );
    let restored = restore_tree_owner(&real_stage, &group);
    match (output, restored) {
        (Ok((code, output)), Ok(())) => {
            json!({"ok":true,"data":{"exit_code":code,"output":output}})
        }
        (Err(message), _) => json!({"ok":false,"error":message}),
        (_, Err(message)) => json!({"ok":false,"error":message}),
    }
}

fn helper_pre_deploy_command(request: &Value) -> Value {
    let Some(app_id) = request["application_id"].as_str().filter(|value| valid_app_id(value)) else {
        return json!({"ok":false,"error":"Application ID is invalid"});
    };
    let Some(job_id) = request["job_id"].as_str().filter(|value| valid_app_id(value)) else {
        return json!({"ok":false,"error":"Deployment ID is invalid"});
    };
    let Some(step) = request["step"].as_str().filter(|value| valid_deployment_log_step(value) && value.starts_with("pre-deploy-")) else {
        return json!({"ok":false,"error":"Pre-deploy step is invalid"});
    };
    let Some(log_path) = request["log_path"].as_str() else {
        return json!({"ok":false,"error":"Pre-deploy log path is missing"});
    };
    let Some(argv) = request["argv"].as_array().filter(|args| !args.is_empty() && args.len() <= 32) else {
        return json!({"ok":false,"error":"Pre-deploy argv is invalid"});
    };
    let mut owned_argv = Vec::with_capacity(argv.len());
    let mut total_bytes = 0usize;
    for item in argv {
        let Some(value) = item.as_str().filter(|value| !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)) else {
            return json!({"ok":false,"error":"Pre-deploy argument is invalid"});
        };
        total_bytes = total_bytes.saturating_add(value.len());
        owned_argv.push(value.to_owned());
    }
    if total_bytes > 4096 {
        return json!({"ok":false,"error":"Pre-deploy argv exceeds the size limit"});
    }
    let root = deployment_root().join("apps").join(app_id);
    let stage = root.join(format!(".staging-{job_id}"));
    if fs::symlink_metadata(&root).is_ok_and(|metadata| !metadata.is_dir() || metadata.file_type().is_symlink()) {
        return json!({"ok":false,"error":"Application root must be a regular directory"});
    }
    let (Ok(real_root), Ok(real_stage)) = (root.canonicalize(), stage.canonicalize()) else {
        return json!({"ok":false,"error":"Application or staging path does not exist"});
    };
    if !real_stage.starts_with(&real_root) || !real_stage.is_dir() {
        return json!({"ok":false,"error":"Staging path escaped the application root"});
    }
    let group = app_group(app_id);
    let build_cache = root.join("build-cache");
    if fs::symlink_metadata(&build_cache).is_ok_and(|metadata| !metadata.is_dir() || metadata.file_type().is_symlink()) {
        return json!({"ok":false,"error":"Build cache path must be a regular directory"});
    }
    if let Err(message) = fs::create_dir_all(&build_cache)
        .map_err(|error| error.to_string())
        .and_then(|_| ensure_group(&group))
        .and_then(|_| prepare_tree_for_build(&real_stage, &group))
        .and_then(|_| prepare_cache_for_build(&build_cache, &group))
    {
        return json!({"ok":false,"error":message});
    }
    let env = ["/usr/bin/env", "/bin/env"].iter().map(PathBuf::from).find(|path| path.is_file());
    let Some(env) = env else {
        let _ = restore_tree_owner(&real_stage, &group);
        return json!({"ok":false,"error":"env executable is not installed"});
    };
    let log_file = match open_build_log_file(log_path, job_id, step) {
        Ok(file) => file,
        Err(message) => { let _ = restore_tree_owner(&real_stage, &group); return json!({"ok":false,"error":message}); }
    };
    let mut args: Vec<&str> = vec!["--"];
    args.extend(owned_argv.iter().map(String::as_str));
    let empty_environment = BTreeMap::new();
    let output = run_sandboxed(&real_stage, &build_cache, &group, &env, &env, &args, &empty_environment, app_id, job_id, step, log_file);
    let restored = restore_tree_owner(&real_stage, &group);
    match (output, restored) {
        (Ok((code, output)), Ok(())) => json!({"ok":true,"data":{"exit_code":code,"output":output}}),
        (Err(message), _) => json!({"ok":false,"error":message}),
        (_, Err(message)) => json!({"ok":false,"error":message}),
    }
}

fn helper_service_enable(request: &Value) -> Value {
    let Some(app_id) = request["application_id"]
        .as_str()
        .filter(|value| valid_app_id(value))
    else {
        return json!({"ok":false,"error":"Application ID is invalid"});
    };
    let Some(port) = request["port"]
        .as_u64()
        .filter(|port| (1024..=65535).contains(port))
    else {
        return json!({"ok":false,"error":"Node port is invalid"});
    };
    let Some(runtime) = request["node_version"].as_str() else {
        return json!({"ok":false,"error":"Node runtime is missing"});
    };
    let start_script = request["start_script"].as_str().unwrap_or("auto");
    let healthcheck_path = request["healthcheck_path"].as_str().unwrap_or("/");
    if !valid_configured_script(start_script, false) || !valid_healthcheck_path(healthcheck_path) {
        return json!({"ok":false,"error":"Runtime script or health check path is invalid"});
    }
    let Some((node, _)) = find_node_binary(runtime) else {
        return json!({"ok":false,"error":"Node runtime does not match"});
    };
    let Some(root) = app_root(app_id) else {
        return json!({"ok":false,"error":"Application path is invalid"});
    };
    if fs::symlink_metadata(&root).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return json!({"ok":false,"error":"Application root must not be a symlink"});
    }
    let Ok(release) = root.join("current").canonicalize() else {
        return json!({"ok":false,"error":"Active release does not exist"});
    };
    if !release.starts_with(root.join("releases").canonicalize().unwrap_or_default()) {
        return json!({"ok":false,"error":"Active release escaped its root"});
    }
    let manifest = match inspect_manifest(&release) {
        Ok(value) => value,
        Err(message) => return json!({"ok":false,"error":message}),
    };
        let entrypoint = runtime_entrypoint(&manifest, &release, false, start_script);
        if entrypoint.is_none()
        || !release.join("node_modules").is_dir()
    {
        return json!({"ok":false,"error":"A start script and installed dependencies are required"});
    }
    let group = app_group(app_id);
    if let Err(message) = ensure_group(&group)
        .and_then(|_| prepare_tree_for_service(&release, &root.join("shared"), &group))
    {
        return json!({"ok":false,"error":message});
    }
    let Some(manager) = manifest.manager else {
        return json!({"ok":false,"error":"A supported package manager is required"});
    };
    let Some(package_manager) = find_package_manager_binary(manager) else {
        return json!({"ok":false,"error":format!("{} is not installed", manager.label())});
    };
    let unit = node_unit_name(app_id);
    let unit_path = PathBuf::from("/etc/systemd/system").join(&unit);
    let unit_content = match render_node_unit(
        &unit,
        &root.join("current"),
        &root.join("shared"),
        &group,
        &node,
        &package_manager,
        port,
        healthcheck_path,
        entrypoint.unwrap(),
    ) {
        Ok(content) => content,
        Err(message) => return json!({"ok":false,"error":message}),
    };
    let existing = match fs::symlink_metadata(&unit_path) {
        Ok(_) if !managed_unit(&unit_path) => {
            return json!({"ok":false,"error":"Refusing to replace a systemd unit not created by ARG"});
        }
        Ok(_) => match fs::read_to_string(&unit_path) {
            Ok(content) => Some(content),
            Err(error) => return json!({"ok":false,"error":error.to_string()}),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return json!({"ok":false,"error":error.to_string()}),
    };
    let was_active = existing.is_some()
        && systemctl_output(&["is-active", &unit]).is_ok_and(|state| state == "active");
    let was_enabled = existing.is_some()
        && systemctl_output(&["is-enabled", &unit]).is_ok_and(|state| state == "enabled");
    if let Err(message) = write_managed_unit(&unit_path, &unit_content) {
        return json!({"ok":false,"error":message});
    }
    let start = systemctl(&["daemon-reload"]).and_then(|()| {
        if was_active {
            systemctl(&["restart", &unit])
        } else {
            systemctl(&["enable", "--now", &unit])
        }
    });
    if let Err(message) = start {
        let restore = restore_node_unit(&unit_path, &unit, existing.as_deref(), was_enabled, was_active);
        return json!({"ok":false,"error":format!("{message}{}", restore.err().map(|problem| format!("؛ بازگرداندن تنظیم قبلی هم شکست خورد: {problem}")).unwrap_or_default())});
    }
    match verify_node_service(port as u16, &unit, healthcheck_path) {
        Ok(()) => {
            json!({"ok":true,"data":{"unit":unit,"enabled":true,"state":"active","port":port,"bind":"127.0.0.1","message":"Node service is running on its private loopback port"}})
        }
        Err(message) => {
            let restore = restore_node_unit(&unit_path, &unit, existing.as_deref(), was_enabled, was_active);
            json!({"ok":false,"error":format!("{message}{}", restore.err().map(|problem| format!("؛ بازگرداندن تنظیم قبلی هم شکست خورد: {problem}")).unwrap_or_default())})
        }
    }
}

fn restore_node_unit(
    unit_path: &FsPath,
    unit: &str,
    previous_content: Option<&str>,
    was_enabled: bool,
    was_active: bool,
) -> Result<(), String> {
    if !was_active {
        let _ = systemctl(&["stop", unit]);
    }
    if let Some(content) = previous_content {
        write_managed_unit(unit_path, content)?;
    } else if unit_path.exists() {
        fs::remove_file(unit_path).map_err(|error| error.to_string())?;
    }
    systemctl(&["daemon-reload"])?;
    if previous_content.is_none() {
        let _ = systemctl(&["disable", unit]);
    } else if was_active {
        systemctl(&["restart", unit])?;
        if let Some(port) = manifest_port(unit_path) {
            verify_node_service(port, unit, &manifest_healthcheck_path(unit_path))?;
        }
    } else if was_enabled {
        systemctl(&["enable", unit])?;
    } else {
        systemctl(&["disable", "--now", unit])?;
    }
    Ok(())
}

fn helper_service_disable(request: &Value) -> Value {
    let Some(app_id) = request["application_id"]
        .as_str()
        .filter(|value| valid_app_id(value))
    else {
        return json!({"ok":false,"error":"Application ID is invalid"});
    };
    let unit = node_unit_name(app_id);
    let unit_path = PathBuf::from("/etc/systemd/system").join(&unit);
    if !managed_unit(&unit_path) {
        return json!({"ok":true,"data":{"unit":unit,"enabled":false,"state":"inactive"}});
    }
    match systemctl(&["disable", "--now", &unit]) {
        Ok(()) => json!({"ok":true,"data":{"unit":unit,"enabled":false,"state":"inactive"}}),
        Err(message) => json!({"ok":false,"error":message}),
    }
}

fn helper_service_restart(request: &Value) -> Value {
    let Some(app_id) = request["application_id"]
        .as_str()
        .filter(|value| valid_app_id(value))
    else {
        return json!({"ok":false,"error":"Application ID is invalid"});
    };
    let unit = node_unit_name(app_id);
    let unit_path = PathBuf::from("/etc/systemd/system").join(&unit);
    if !managed_unit(&unit_path) {
        return json!({"ok":false,"error":"Node service is not configured"});
    }
    let port = match manifest_port(&unit_path) {
        Some(port) => port,
        None => return json!({"ok":false,"error":"Node service port is invalid"}),
    };
    if let Err(message) = systemctl(&["restart", &unit]) {
        return json!({"ok":false,"error":message});
    }
    match verify_node_service(port, &unit, &manifest_healthcheck_path(&unit_path)) {
        Ok(()) => json!({"ok":true,"data":{"unit":unit,"state":"active","port":port,"bind":"127.0.0.1"}}),
        Err(message) => {
            let stop = systemctl(&["stop", &unit]);
            let detail = stop.err().map(|problem| format!(" سرویس متوقف نشد: {problem}")).unwrap_or_default();
            json!({"ok":false,"error":format!("{message}{detail}")})
        },
    }
}

fn manifest_port(unit_path: &FsPath) -> Option<u16> {
    fs::read_to_string(unit_path)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("Environment=PORT=")?.parse::<u16>().ok())
        .filter(|port| (1024..=65535).contains(port))
}

fn manifest_healthcheck_path(unit_path: &FsPath) -> String {
    fs::read_to_string(unit_path)
        .ok()
        .and_then(|contents| contents.lines().find_map(|line| line.strip_prefix("Environment=ARG_HEALTHCHECK_PATH=").map(str::to_owned)))
        .filter(|path| valid_healthcheck_path(path))
        .unwrap_or_else(|| "/".to_owned())
}

fn managed_unit(path: &FsPath) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
        && fs::read_to_string(path)
            .is_ok_and(|contents| contents.starts_with("# Managed by ARG Node runner;"))
}

pub(super) fn purge_application_service(app_id: &str) -> Result<bool, String> {
    if !valid_app_id(app_id) {
        return Err("شناسهٔ سرویس Node برای حذف معتبر نیست.".into());
    }
    let unit = node_unit_name(app_id);
    let path = PathBuf::from("/etc/systemd/system").join(&unit);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("فایل سرویس Node بررسی نشد: {error}")),
    };
    if !metadata.file_type().is_file() {
        return Err("مسیر سرویس Node فایل معمولی نیست؛ حذف متوقف شد.".into());
    }
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("محتوای سرویس Node خوانده نشد: {error}"))?;
    let app_root = deployment_root().join("apps").join(app_id);
    if !contents.starts_with("# Managed by ARG Node runner;")
        || !contents.contains(&format!("BindReadOnlyPaths={}/current:/workspace", app_root.display()))
        || !contents.contains(&format!("BindPaths={}/shared/data:/shared/data", app_root.display()))
    {
        return Err("سرویس هم‌نام Node با این اپلیکیشن تطبیق ندارد و دست‌نخورده ماند.".into());
    }
    systemctl(&["disable", "--now", &unit])
        .map_err(|error| format!("توقف سرویس Node ناموفق بود: {error}"))?;
    fs::remove_file(&path).map_err(|error| format!("حذف فایل سرویس Node ناموفق بود: {error}"))?;
    systemctl(&["daemon-reload"])
        .map_err(|error| format!("systemd پس از حذف سرویس Node بارگذاری نشد: {error}"))?;

    let group = app_group(app_id);
    if let Ok(output) = Command::new("getent").args(["group", &group]).output() {
        if output.status.success() {
            let output = Command::new("groupdel")
                .arg(&group)
                .stdin(Stdio::null())
                .output()
                .map_err(|error| format!("گروه اختصاصی Node حذف نشد: {error}"))?;
            if !output.status.success() {
                return Err(format!(
                    "سرویس حذف شد اما گروه اختصاصی {group} حذف نشد: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
        }
    }
    Ok(true)
}

fn helper_environment_keys(request: &Value) -> Value {
    let Some(app_id) = request["application_id"].as_str().filter(|value| valid_app_id(value)) else {
        return json!({"ok":false,"error":"Application ID is invalid"});
    };
    let Some(root) = app_root(app_id) else {
        return json!({"ok":false,"error":"Application path is invalid"});
    };
    if fs::symlink_metadata(&root).is_ok_and(|metadata| metadata.file_type().is_symlink())
        || !fs::symlink_metadata(root.join("shared"))
            .is_ok_and(|metadata| metadata.file_type().is_dir())
    {
        return json!({"ok":false,"error":"Application shared path is invalid"});
    }
    let path = root.join("shared/.env");
    if !fs::symlink_metadata(&path).is_ok_and(|metadata| {
        metadata.file_type().is_file() && metadata.len() <= 1024 * 1024
    }) {
        return json!({"ok":false,"error":"Environment file is invalid"});
    }
    let keys = read_environment(&path)
        .into_iter()
        .map(|(key, _)| key)
        .collect::<Vec<_>>();
    json!({"ok":true,"data":{"keys":keys,"values_returned":false}})
}

fn helper_environment_save(request: &Value) -> Value {
    let Some(app_id) = request["application_id"].as_str().filter(|value| valid_app_id(value)) else {
        return json!({"ok":false,"error":"Application ID is invalid"});
    };
    let Some(root) = app_root(app_id) else {
        return json!({"ok":false,"error":"Application path is invalid"});
    };
    if fs::symlink_metadata(&root).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return json!({"ok":false,"error":"Application root must not be a symlink"});
    }
    let shared = root.join("shared");
    if !fs::symlink_metadata(&shared).is_ok_and(|metadata| metadata.file_type().is_dir()) {
        return json!({"ok":false,"error":"Shared path must be a normal directory"});
    }
    let path = shared.join(".env");
    if fs::symlink_metadata(&path).is_ok_and(|metadata| {
        metadata.file_type().is_symlink() || metadata.len() > 1024 * 1024
    }) {
        return json!({"ok":false,"error":"Environment file is invalid or exceeds 1 MiB"});
    }
    let Some(values) = request["variables"].as_object() else {
        return json!({"ok":false,"error":"Environment variables must be an object"});
    };
    if values.is_empty() || values.len() > 100 || values.values().any(|value| !value.is_string()) {
        return json!({"ok":false,"error":"Between 1 and 100 string environment values are required"});
    }
    let variables = values
        .iter()
        .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_owned())))
        .collect::<HashMap<_, _>>();
    let group = app_group(app_id);
    match ensure_group(&group).and_then(|_| write_environment(&path, &variables, &group)) {
        Ok(keys) => json!({"ok":true,"data":{"keys":keys,"secrets_returned":false}}),
        Err(message) => json!({"ok":false,"error":message}),
    }
}

fn helper_environment_permissions(request: &Value) -> Value {
    let Some(app_id) = request["application_id"]
        .as_str()
        .filter(|value| valid_app_id(value))
    else {
        return json!({"ok":false,"error":"Application ID is invalid"});
    };
    let Some(root) = app_root(app_id) else {
        return json!({"ok":false,"error":"Application path is invalid"});
    };
    if fs::symlink_metadata(&root).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return json!({"ok":false,"error":"Application root must not be a symlink"});
    }
    match fs::symlink_metadata(root.join("shared")) {
        Ok(metadata) if metadata.file_type().is_dir() => {}
        _ => return json!({"ok":false,"error":"Shared path must be a normal directory"}),
    }
    let environment = root.join("shared/.env");
    match fs::symlink_metadata(&environment) {
        Ok(metadata) if metadata.file_type().is_file() && metadata.len() <= 1024 * 1024 => {}
        _ => return json!({"ok":false,"error":"Environment must be a regular file under 1 MiB"}),
    }
    let group = app_group(app_id);
    match ensure_group(&group).and_then(|_| set_group_mode(&environment, &group, 0o640)) {
        Ok(()) => json!({"ok":true,"data":{"group":group,"mode":"0640"}}),
        Err(message) => json!({"ok":false,"error":message}),
    }
}

fn helper_service_status(request: &Value, logs: bool) -> Value {
    let Some(app_id) = request["application_id"]
        .as_str()
        .filter(|value| valid_app_id(value))
    else {
        return json!({"ok":false,"error":"Application ID is invalid"});
    };
    let unit = node_unit_name(app_id);
    let unit_path = PathBuf::from("/etc/systemd/system").join(&unit);
    if logs && !managed_unit(&unit_path) {
        return json!({"ok":true,"data":{"unit":unit,"logs":""}});
    }
    if !managed_unit(&unit_path) {
        return json!({"ok":true,"data":{"unit":unit,"state":"inactive","enabled":false}});
    }
    let active = systemctl_output(&["is-active", &unit]).unwrap_or_else(|_| "inactive".into());
    let enabled = systemctl_output(&["is-enabled", &unit]).is_ok_and(|value| value == "enabled");
    if logs {
        let limit = request["limit"].as_u64().unwrap_or(100).clamp(1, 500);
        let cursor = request["after"].as_str().filter(|value| valid_journal_cursor(value));
        let mut command = Command::new("/usr/bin/journalctl");
        command.args(["-u", &unit, "-n", &limit.to_string(), "--no-pager", "--show-cursor", "-o", "cat"]);
        if let Some(cursor) = cursor { command.arg(format!("--after-cursor={cursor}")); }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        return match command.spawn() {
            Ok(mut child) => {
                let Some(stdout) = child.stdout.take() else {
                    return json!({"ok":false,"error":"journalctl stdout unavailable"});
                };
                let reader = thread::spawn(move || read_capped_sync(stdout));
                let _ = child.wait();
                let output = reader.join().unwrap_or_default();
                let text=String::from_utf8_lossy(&output).into_owned();
                let next_cursor=text.lines().find_map(|line|line.strip_prefix("-- cursor: ").map(str::to_owned));
                let logs=text.lines().filter(|line|!line.starts_with("-- cursor: ")).collect::<Vec<_>>().join("\n");
                json!({"ok":true,"data":{"unit":unit,"logs":logs,"next_cursor":next_cursor,"source":"systemd-journal","persistent":true,"retention_scope":"host-journald"}})
            }
            Err(error_value) => json!({"ok":false,"error":format!("Could not read unit logs: {error_value}")}),
        };
    }
    json!({"ok":true,"data":{"unit":unit,"state":active,"enabled":enabled}})
}

pub(super) fn ensure_group(group: &str) -> Result<(), String> {
    if !group.starts_with("argapp-")
        || group.len() != 19
        || !group
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err("Per-application group name is invalid".into());
    }
    let existing = fs::read_to_string("/etc/group")
        .unwrap_or_default()
        .lines()
        .any(|line| line.split(':').next() == Some(group));
    if existing {
        return Ok(());
    }
    let binary = ["/usr/sbin/groupadd", "/usr/bin/groupadd"]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .ok_or("groupadd is not installed")?;
    let output = Command::new(binary)
        .args(["--system", group])
        .stdin(Stdio::null())
        .output()
        .map_err(|error_value| error_value.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Could not create application group: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn prepare_tree_for_build(root: &FsPath, group: &str) -> Result<(), String> {
    change_tree_group(root, group)?;
    chmod_tree(root, "g+rwX")
        .map_err(|error_value| format!("Build permissions could not be prepared: {error_value}"))?;
    Ok(())
}

fn prepare_cache_for_build(cache: &FsPath, group: &str) -> Result<(), String> {
    fs::create_dir_all(cache).map_err(|error_value| error_value.to_string())?;
    if fs::symlink_metadata(cache).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("Build cache must be a normal directory".into());
    }
    change_tree_group(cache, group)?;
    chmod_tree(cache, "g+rwX")
}

fn prepare_tree_for_service(release: &FsPath, shared: &FsPath, group: &str) -> Result<(), String> {
    change_tree_group(release, group)?;
    chmod_tree(release, "g+rX")?;
    fs::create_dir_all(shared.join("data")).map_err(|error_value| error_value.to_string())?;
    change_tree_group(shared, group)?;
    chmod_tree(shared, "g+rwX")?;
    if shared.join(".env").is_file() {
        set_group_mode(&shared.join(".env"), group, 0o640)?;
    }
    Ok(())
}

fn restore_tree_owner(root: &FsPath, group: &str) -> Result<(), String> {
    let chown = ["/usr/bin/chown", "/bin/chown"]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .ok_or("chown is not installed")?;
    let output = Command::new(chown)
        .args(["-R", "--no-dereference", &format!("arg:{group}"), "--"])
        .arg(root)
        .stdin(Stdio::null())
        .output()
        .map_err(|error_value| error_value.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "Build output ownership could not be restored: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    chmod_tree(root, "g+rX")
}

fn change_tree_group(root: &FsPath, group: &str) -> Result<(), String> {
    let chgrp = ["/usr/bin/chgrp", "/bin/chgrp"]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .ok_or("chgrp is not installed")?;
    let output = Command::new(chgrp)
        .args(["-R", "--no-dereference", group, "--"])
        .arg(root)
        .stdin(Stdio::null())
        .output()
        .map_err(|error_value| error_value.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Could not set application group: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn set_group_mode(path: &FsPath, group: &str, mode: u32) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error_value| error_value.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err("Environment file symlink is not allowed".into());
    }
    let chgrp = ["/usr/bin/chgrp", "/bin/chgrp"]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .ok_or("chgrp is not installed")?;
    let output = Command::new(chgrp)
        .args(["--no-dereference", group, "--"])
        .arg(path)
        .output()
        .map_err(|error_value| error_value.to_string())?;
    if !output.status.success() {
        return Err("Could not set environment group".into());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|error_value| error_value.to_string())?;
    Ok(())
}

fn chmod_tree(root: &FsPath, mode: &str) -> Result<(), String> {
    let chmod = ["/usr/bin/chmod", "/bin/chmod"]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .ok_or("chmod is not installed")?;
    let output = Command::new(chmod)
        .args(["-R", "--no-dereference", mode, "--"])
        .arg(root)
        .stdin(Stdio::null())
        .output()
        .map_err(|error_value| error_value.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn run_sandboxed(
    release: &FsPath,
    cache: &FsPath,
    group: &str,
    node_binary: &FsPath,
    binary: &FsPath,
    args: &[&str],
    build_environment: &BTreeMap<String, String>,
    app_id: &str,
    job_id: &str,
    step: &str,
    log_file: fs::File,
) -> Result<(i32, String), String> {
    let stage = release
        .to_str()
        .ok_or("Non-UTF8 build path is unsupported")?;
    let cache = cache
        .to_str()
        .ok_or("Non-UTF8 build cache path is unsupported")?;
    if [stage, cache]
        .iter()
        .any(|path| !safe_systemd_path(path))
    {
        return Err("Deployment path cannot be represented safely in systemd properties.".into());
    }
    let unit = format!("arg-node-build-{}-{}-{step}", &app_id[..8], &job_id[..8]);
    let systemd = PathBuf::from("/usr/bin/systemd-run");
    if !systemd.is_file() {
        return Err("systemd-run is required for isolated Node builds.".into());
    }
    let mut command = Command::new(systemd);
    command.args([
        "--quiet",
        "--wait",
        "--pipe",
        "--collect",
        &format!("--unit={unit}"),
    ]);
    let node_dir = node_binary.parent().unwrap_or(FsPath::new("/usr/bin"));
    if !safe_systemd_path(&node_dir.to_string_lossy()) {
        return Err("Node runtime path cannot be represented safely in systemd properties.".into());
    }
    for property in [
        "Type=oneshot",
        "DynamicUser=yes",
        &format!("Group={group}"),
        "WorkingDirectory=/workspace",
        &format!("BindPaths={stage}:/workspace"),
        &format!("BindPaths={cache}:/cache"),
        "ProtectSystem=strict",
        "ProtectHome=true",
        "PrivateTmp=true",
        "PrivateDevices=true",
        "NoNewPrivileges=true",
        "ProtectKernelTunables=true",
        "ProtectKernelModules=true",
        "ProtectControlGroups=true",
        "RestrictSUIDSGID=true",
        "LockPersonality=true",
        "InaccessiblePaths=/var/lib/arg",
        "InaccessiblePaths=/etc/arg",
        "InaccessiblePaths=/usr/local/lib/arg",
        "UMask=0007",
        "MemoryMax=2G",
        "CPUQuota=200%",
        "TasksMax=256",
        "RuntimeMaxSec=1200",
        "Environment=HOME=/tmp",
        "Environment=CI=1",
        "Environment=NODE_ENV=production",
        &format!("Environment=PATH={}:{}", node_dir.display(), "/usr/local/bin:/usr/bin:/bin"),
    ] {
        command.arg(format!("--property={property}"));
    }
    for (name, value) in build_environment {
        command.arg(format!("--setenv={name}={value}"));
    }
    command
        .arg("--")
        .arg(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error_value| format!("Could not start isolated Node task: {error_value}"))?;
    let stdout = child.stdout.take().ok_or("stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("stderr unavailable")?;
    let sink = Arc::new(Mutex::new(BuildLogSink { file: log_file, bytes_written: 0, truncated: false }));
    let out_sink = sink.clone();
    let err_sink = sink.clone();
    let out = thread::spawn(move || read_build_stream(stdout, "stdout", out_sink));
    let err = thread::spawn(move || read_build_stream(stderr, "stderr", err_sink));
    let status = child
        .wait()
        .map_err(|error_value| error_value.to_string())?;
    let mut output = String::from_utf8_lossy(&out.join().unwrap_or_default()).into_owned();
    let stderr_bytes = err.join().unwrap_or_default();
    let stderr = String::from_utf8_lossy(&stderr_bytes);
    if !stderr.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&stderr);
    }
    let mut sink = sink.lock().unwrap();
    if sink.truncated {
        let _ = sink.file.write_all(b"[ARG] build log reached the 4 MiB retention limit; additional process output was omitted.\n");
        let _ = sink.file.flush();
    }
    Ok((status.code().unwrap_or(128), sanitize_build_output(&output)))
}

const LIVE_BUILD_LOG_LIMIT: usize = 4 * 1024 * 1024;

struct BuildLogSink {
    file: fs::File,
    bytes_written: usize,
    truncated: bool,
}

fn open_build_log_file(path: &str, job_id: &str, step: &str) -> Result<fs::File, String> {
    if !valid_app_id(job_id) || !valid_deployment_log_step(step) {
        return Err("Live build log identity is invalid".into());
    }
    let directory = deployment_root().join("job-logs");
    let metadata = fs::symlink_metadata(&directory).map_err(|error| format!("Build log directory is unavailable: {error}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Build log directory must be a regular directory".into());
    }
    let canonical_directory = directory.canonicalize().map_err(|error| error.to_string())?;
    let requested = PathBuf::from(path);
    if !requested.is_absolute()
        || requested.file_name().and_then(|name| name.to_str()) != Some(format!("{job_id}-{step}.log").as_str())
        || requested.parent().and_then(|parent| parent.canonicalize().ok()).as_deref() != Some(canonical_directory.as_path())
    {
        return Err("Build log path is outside the job log directory".into());
    }
    let metadata = fs::symlink_metadata(&requested).map_err(|error| format!("Build log file is unavailable: {error}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("Build log must be a private regular file".into());
    }
    OpenOptions::new().append(true).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(requested)
        .map_err(|error| format!("Build log could not be opened safely: {error}"))
}

fn write_build_log_line(sink: &Arc<Mutex<BuildLogSink>>, stream: &str, line: &[u8]) {
    let mut sink = sink.lock().unwrap();
    if sink.truncated { return; }
    let prefix = format!("[{stream}] ");
    let remaining = LIVE_BUILD_LOG_LIMIT.saturating_sub(sink.bytes_written);
    if remaining == 0 { sink.truncated = true; return; }
    let total = prefix.len().saturating_add(line.len()).saturating_add(1);
    let keep = remaining.min(total);
    let prefix_bytes = prefix.as_bytes();
    let prefix_keep = keep.min(prefix_bytes.len());
    let _ = sink.file.write_all(&prefix_bytes[..prefix_keep]);
    let line_keep = keep.saturating_sub(prefix_keep).min(line.len());
    let _ = sink.file.write_all(&line[..line_keep]);
    if keep > prefix_keep + line_keep { let _ = sink.file.write_all(b"\n"); }
    let _ = sink.file.flush();
    sink.bytes_written = sink.bytes_written.saturating_add(keep);
    if keep < total { sink.truncated = true; }
}

fn read_build_stream<R: Read>(mut reader: R, stream: &'static str, sink: Arc<Mutex<BuildLogSink>>) -> Vec<u8> {
    let mut saved = Vec::new();
    let mut pending = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(length) => {
                let remaining = OUTPUT_LIMIT.saturating_sub(saved.len());
                saved.extend_from_slice(&buffer[..length.min(remaining)]);
                pending.extend_from_slice(&buffer[..length]);
                while let Some(end) = pending.iter().position(|byte| *byte == b'\n' || *byte == b'\r') {
                    let line = pending.drain(..end).collect::<Vec<_>>();
                    let consumed = pending.remove(0);
                    if !line.is_empty() { write_build_log_line(&sink, stream, &line); }
                    if consumed == b'\r' && pending.first() == Some(&b'\n') { pending.remove(0); }
                }
                while pending.len() > 16 * 1024 {
                    let chunk = pending.drain(..16 * 1024).collect::<Vec<_>>();
                    write_build_log_line(&sink, stream, &chunk);
                }
            }
        }
    }
    if !pending.is_empty() { write_build_log_line(&sink, stream, &pending); }
    saved
}

fn read_capped_sync<R: Read>(mut reader: R) -> Vec<u8> {
    let mut saved = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(length) => {
                let remaining = OUTPUT_LIMIT.saturating_sub(saved.len());
                saved.extend_from_slice(&buffer[..length.min(remaining)]);
            }
        }
    }
    saved
}

fn sanitize_build_output(output: &str) -> String {
    let markers = [
        "password=",
        "password:",
        "token=",
        "token:",
        "secret=",
        "secret:",
        "api_key=",
        "authorization:",
    ];
    output
        .lines()
        .take(600)
        .map(|line| {
            let mut result = line.to_owned();
            for marker in markers {
                let lower = result.to_ascii_lowercase();
                if let Some(start) = lower.find(marker) {
                    let start = start + marker.len();
                    let end = result[start..]
                        .find(char::is_whitespace)
                        .map(|offset| start + offset)
                        .unwrap_or(result.len());
                    if end > start {
                        result.replace_range(start..end, "[REDACTED]");
                    }
                }
            }
            result
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_node_unit(
    unit: &str,
    current: &FsPath,
    shared: &FsPath,
    group: &str,
    node: &FsPath,
    package_manager: &FsPath,
    port: u64,
    healthcheck_path: &str,
    entrypoint: RuntimeEntrypoint,
) -> Result<String, String> {
    let current = current.to_string_lossy().into_owned();
    let shared = shared.to_string_lossy().into_owned();
    let path = node
        .parent()
        .map(|parent| format!("{}:/usr/local/bin:/usr/bin:/bin", parent.display()))
        .unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into());
    let package_manager = package_manager.to_string_lossy().into_owned();
    let env_file = shared.to_string();
    if !valid_healthcheck_path(healthcheck_path) {
        return Err("Health check path is invalid".into());
    }
    let start_command = match entrypoint {
        RuntimeEntrypoint::PackageScript(script) if valid_script_name(&script) => format!("{package_manager} run {script}"),
        RuntimeEntrypoint::PackageScript(_) => return Err("Runtime script is invalid".into()),
        RuntimeEntrypoint::NextCli => format!("{} /workspace/node_modules/next/dist/bin/next start --hostname 127.0.0.1 --port {port}", node.display()),
        RuntimeEntrypoint::NuxtOutput => format!("{} /workspace/.output/server/index.mjs", node.display()),
    };
    if ![current.as_str(), env_file.as_str(), package_manager.as_str()]
        .iter()
        .all(|value| safe_systemd_path(value))
        || !path.split(':').all(safe_systemd_path)
    {
        return Err("Application paths cannot be safely represented in a systemd unit".into());
    }
    Ok(format!("# Managed by ARG Node runner; edits are replaced from the panel.\n[Unit]\nDescription=ARG Node application {unit}\nAfter=network-online.target\nWants=network-online.target\nStartLimitIntervalSec=60\nStartLimitBurst=5\n\n[Service]\nType=simple\nDynamicUser=yes\nGroup={group}\nWorkingDirectory=/workspace\nBindReadOnlyPaths={current}:/workspace\nBindReadOnlyPaths={shared}/.env:/shared/.env\nBindPaths={shared}/data:/shared/data\nEnvironmentFile=-{env_file}/.env\nInaccessiblePaths=/var/lib/arg\nInaccessiblePaths=/etc/arg\nInaccessiblePaths=/usr/local/lib/arg\nProtectSystem=strict\nProtectHome=true\nPrivateTmp=true\nPrivateDevices=true\nNoNewPrivileges=true\nProtectKernelTunables=true\nProtectKernelModules=true\nProtectControlGroups=true\nRestrictSUIDSGID=true\nLockPersonality=true\nCapabilityBoundingSet=\nUMask=0027\nEnvironment=HOME=/tmp\nEnvironment=NODE_ENV=production\nEnvironment=HOST=127.0.0.1\nEnvironment=PORT={port}\nEnvironment=NITRO_HOST=127.0.0.1\nEnvironment=NITRO_PORT={port}\nEnvironment=ARG_HEALTHCHECK_PATH={healthcheck_path}\nEnvironment=PATH={path}\nExecStart={start_command}\nRestart=on-failure\nRestartSec=3\nTimeoutStopSec=30\nKillMode=control-group\nMemoryMax=1G\nCPUQuota=200%\nTasksMax=256\n\n[Install]\nWantedBy=multi-user.target\n"))
}

fn safe_systemd_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.contains("..")
        && path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-".contains(&byte))
}

fn write_managed_unit(path: &FsPath, content: &str) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if !metadata.file_type().is_file() {
            return Err("Existing Node service unit is not a regular file".into());
        }
        let previous = fs::read_to_string(path).unwrap_or_default();
        if !previous.starts_with("# Managed by ARG Node runner;") {
            return Err("Refusing to replace a systemd unit not created by ARG".into());
        }
    }
    let parent = path.parent().ok_or("systemd unit path is invalid")?;
    let temp = parent.join(format!(".arg-node-{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&temp)
        .map_err(|error_value| error_value.to_string())?;
    if let Err(error_value) =
        std::io::Write::write_all(&mut file, content.as_bytes()).and_then(|_| file.sync_all())
    {
        let _ = fs::remove_file(&temp);
        return Err(error_value.to_string());
    }
    fs::rename(&temp, path).map_err(|error_value| {
        let _ = fs::remove_file(&temp);
        error_value.to_string()
    })
}

fn systemctl(args: &[&str]) -> Result<(), String> {
    let output = Command::new("/usr/bin/systemctl")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error_value| error_value.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "systemctl {} failed: {}",
            args.first().copied().unwrap_or("action"),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn systemctl_output(args: &[&str]) -> Result<String, String> {
    let output = Command::new("/usr/bin/systemctl")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error_value| error_value.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!text.is_empty())
        .then_some(text)
        .ok_or_else(|| String::from_utf8_lossy(&output.stderr).trim().to_owned())
}

fn ensure_loopback_listener(port: u16, unit: &str) -> Result<(), String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(12);
    loop {
        let output = Command::new("/usr/bin/ss")
            .args(["-H", "-ltn", &format!("sport = :{port}")])
            .stdin(Stdio::null())
            .output()
            .map_err(|_| "iproute2 ss is required to verify the private listener".to_owned())?;
        let text = String::from_utf8_lossy(&output.stdout);
        if output.status.success() && !text.trim().is_empty() {
            let safe = text.lines().all(|line| {
                line.split_whitespace().nth(3).is_some_and(|address| {
                    address.starts_with("127.0.0.1:") || address.starts_with("[::1]:")
                })
            });
            if safe {
                return Ok(());
            }
            return Err(format!("Node process listened outside loopback on port {port}; service {unit} was stopped."));
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "No listener appeared on 127.0.0.1:{port}; check the package start script."
            ));
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn verify_node_service(port: u16, unit: &str, healthcheck_path: &str) -> Result<(), String> {
    if !valid_healthcheck_path(healthcheck_path) {
        return Err("مسیر Health Check در سرویس معتبر نیست.".into());
    }
    ensure_loopback_listener(port, unit)?;
    let address = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
    let deadline = std::time::Instant::now() + Duration::from_secs(12);
    loop {
        let last_error = match probe_node_http(address, healthcheck_path) {
            Ok(status) if (200..300).contains(&status) => return Ok(()),
            Ok(status) => format!("بررسی HTTP مسیر {healthcheck_path} کد {status} گرفت؛ پاسخ 2xx لازم است."),
            Err(message) => message,
        };
        if std::time::Instant::now() >= deadline {
            return Err(format!("برنامه روی loopback پورت {port} گوش می‌دهد، اما بررسی HTTP موفق نشد: {last_error}"));
        }
        thread::sleep(Duration::from_millis(300));
    }
}

fn probe_node_http(address: SocketAddr, path: &str) -> Result<u16, String> {
    if !valid_healthcheck_path(path) { return Err("مسیر Health Check ناامن است.".into()); }
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(500))
        .map_err(|error| format!("اتصال به health check محلی برقرار نشد: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .and_then(|_| stream.set_write_timeout(Some(Duration::from_secs(2))))
        .map_err(|error| error.to_string())?;
    stream
        .write_all(format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nUser-Agent: ARG-healthcheck\r\n\r\n").as_bytes())
        .map_err(|error| format!("درخواست health check ارسال نشد: {error}"))?;
    let mut response = Vec::with_capacity(1024);
    let mut buffer = [0u8; 1024];
    while response.len() < 8192 {
        let capacity = buffer.len().min(8192 - response.len());
        let count = stream
            .read(&mut buffer[..capacity])
            .map_err(|error| format!("پاسخ health check خوانده نشد: {error}"))?;
        if count == 0 {
            break;
        }
        response.extend_from_slice(&buffer[..count]);
        if response.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let status = parse_node_http_status(&response)
        .ok_or_else(|| "پاسخ health check خط وضعیت HTTP معتبر ندارد.".to_owned())?;
    Ok(status)
}

fn parse_node_http_status(response: &[u8]) -> Option<u16> {
    let line = response.split(|byte| *byte == b'\n').next()?;
    let text = std::str::from_utf8(line).ok()?.trim_end_matches('\r');
    let mut parts = text.split_ascii_whitespace();
    if !matches!(parts.next(), Some("HTTP/1.0" | "HTTP/1.1")) {
        return None;
    }
    parts
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|status| (200..600).contains(status))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn build_runner_streams_each_real_output_line_to_its_job_log() {
        let path = std::env::temp_dir().join(format!("arg-build-live-log-{}.log", rand::random::<u64>()));
        let file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path).unwrap();
        let sink = Arc::new(Mutex::new(BuildLogSink { file, bytes_written: 0, truncated: false }));
        let output = read_build_stream(std::io::Cursor::new(b"installing package\r\nrun build\n"), "stdout", sink.clone());
        assert_eq!(String::from_utf8(output).unwrap(), "installing package\r\nrun build\n");
        drop(sink);
        let log = fs::read_to_string(&path).unwrap();
        assert_eq!(log, "[stdout] installing package\n[stdout] run build\n");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn static_framework_detection_and_output_resolution_cover_spa_and_ssr() {
        let root = std::env::temp_dir().join(format!("arg-static-test-{}", rand::random::<u64>()));
        fs::create_dir_all(root.join("dist")).unwrap();
        fs::write(root.join("dist/index.html"), "<!doctype html>").unwrap();
        assert_eq!(resolve_static_output_path(&root, "auto").unwrap(), "dist");
        assert!(is_static_deployment("auto", &root, "auto").unwrap());
        fs::write(root.join("package.json"), r#"{"dependencies":{"next":"15"},"scripts":{"start":"next start","build":"next build"}}"#).unwrap();
        fs::write(root.join("package-lock.json"), "{}").unwrap();
        assert!(!is_static_deployment("auto", &root, "auto").unwrap());
        assert!(!is_static_deployment("next", &root, "auto").unwrap());
        assert!(static_build_available(&root).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prebuilt_static_root_and_custom_output_are_resolved_without_node_manifest() {
        let root = std::env::temp_dir().join(format!("arg-static-root-{}", rand::random::<u64>()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("index.html"), "ok").unwrap();
        assert_eq!(resolve_static_output_path(&root, "auto").unwrap(), ".");
        assert_eq!(resolve_static_output_path(&root, ".").unwrap(), ".");
        assert!(is_static_deployment("auto", &root, "auto").unwrap());
        assert!(!static_build_available(&root).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn auto_detect_accepts_custom_prebuilt_static_path_without_manifest() {
        let root = std::env::temp_dir().join(format!("arg-static-custom-{}", rand::random::<u64>()));
        fs::create_dir_all(root.join("customer/site")).unwrap();
        fs::write(root.join("customer/site/index.html"), "ok").unwrap();
        assert!(is_static_deployment("auto", &root, "customer/site").unwrap());
        assert!(!is_static_deployment("auto", &root, "auto").unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn nuxt_entrypoint_must_be_a_regular_file_inside_the_release() {
        let root = std::env::temp_dir().join(format!("arg-nuxt-entry-{}", rand::random::<u64>()));
        fs::create_dir_all(root.join(".output/server")).unwrap();
        fs::write(root.join(".output/server/index.mjs"), "export {};").unwrap();
        assert!(has_nuxt_server_entrypoint(&root));
        fs::remove_file(root.join(".output/server/index.mjs")).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", root.join(".output/server/index.mjs")).unwrap();
        assert!(!has_nuxt_server_entrypoint(&root));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn next_cli_entrypoint_must_resolve_inside_the_release() {
        let root = std::env::temp_dir().join(format!("arg-next-entry-{}", rand::random::<u64>()));
        fs::create_dir_all(root.join("node_modules/next/dist/bin")).unwrap();
        fs::write(root.join("node_modules/next/dist/bin/next"), "#!/usr/bin/env node").unwrap();
        assert!(has_next_cli_entrypoint(&root));
        fs::remove_file(root.join("node_modules/next/dist/bin/next")).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", root.join("node_modules/next/dist/bin/next")).unwrap();
        assert!(!has_next_cli_entrypoint(&root));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn package_manager_is_selected_from_one_matching_lockfile() {
        let root = std::env::temp_dir().join(format!("arg-node-test-{}", now()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("package.json"),r#"{"packageManager":"npm@10.0.0","scripts":{"start":"node server.js","build":"vite build"},"engines":{"node":"^22"}}"#).unwrap();
        fs::write(root.join("package-lock.json"), "{}").unwrap();
        let m = inspect_manifest(&root).unwrap();
        assert_eq!(m.manager, Some(PackageManager::Npm));
        assert!(m.scripts.contains(&"start".into()));
        assert_eq!(m.node_engine.as_deref(), Some("^22"));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn conflicting_lockfiles_are_rejected_and_yarn_berry_is_not_guessed() {
        let root = std::env::temp_dir().join(format!("arg-node-test-{}", now()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("package-lock.json"), "{}").unwrap();
        fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'").unwrap();
        assert!(choose_package_manager(&root, None).is_err());
        let _ = fs::remove_file(root.join("package-lock.json"));
        let _ = fs::remove_file(root.join("pnpm-lock.yaml"));
        fs::write(root.join("yarn.lock"), "").unwrap();
        assert!(choose_package_manager(&root, Some("yarn@4.1.0")).is_err());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn node_environment_validation_rejects_reserved_and_multiline_values() {
        assert!(valid_env_name("DB_PASSWORD"));
        assert!(!valid_env_name("ARG_PRIVATE"));
        assert!(!valid_env_name("db_password"));
        let root = std::env::temp_dir().join(format!("arg-node-env-{}", now()));
        let mut values = HashMap::new();
        values.insert("A".into(), "one\ntwo".into());
        assert!(write_environment(&root, &values, "unused-test-group").is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn build_environment_allows_public_prefixes_and_rejects_secret_names() {
        let allowed = BTreeMap::from([
            ("VITE_API_ORIGIN".to_owned(), "https://example.test".to_owned()),
            ("NEXT_PUBLIC_MAP_ID".to_owned(), "public-map".to_owned()),
            ("NUXT_PUBLIC_TITLE".to_owned(), "ARG".to_owned()),
            ("REACT_APP_ORIGIN".to_owned(), "https://example.test".to_owned()),
            ("PUBLIC_ASSET_HOST".to_owned(), "cdn.example.test".to_owned()),
        ]);
        assert!(validate_public_build_environment(&allowed).is_ok());
        assert!(valid_public_build_name("VITE_API_ORIGIN"));
        assert!(!valid_public_build_name("VITE_SECRET_TOKEN"));
        assert!(!valid_public_build_name("DB_PASSWORD"));
        let invalid = BTreeMap::from([("VITE_LABEL".to_owned(), "first\nsecond".to_owned())]);
        assert!(validate_public_build_environment(&invalid).is_err());
        assert!(parse_public_build_environment(r#"{"VITE_REGION":"eu"}"#).is_ok());
        assert!(parse_public_build_environment(r#"{"VITE_SECRET":"no"}"#).is_err());
    }
    #[test]
    fn unit_and_service_group_names_are_uuid_derived() {
        let id = "12345678-1234-4234-8234-123456789abc";
        assert_eq!(node_unit_name(id), "arg-node-12345678-123.service");
        assert_eq!(app_group(id), "argapp-12345678-123");
    }

    #[test]
    fn install_lifecycle_scripts_are_enabled_only_inside_the_sandboxed_runner() {
        for manager in [PackageManager::Npm, PackageManager::Pnpm, PackageManager::YarnClassic] {
            assert!(!manager.install_args().contains(&"--ignore-scripts"));
        }
        assert!(PackageManager::Npm.install_args().contains(&"--no-audit"));
        assert!(PackageManager::Pnpm.install_args().contains(&"--frozen-lockfile"));
        assert!(PackageManager::YarnClassic.install_args().contains(&"--frozen-lockfile"));
    }

    #[test]
    fn node_unit_mounts_env_read_only_and_only_data_writable() {
        let unit = render_node_unit(
            "arg-node-12345678-123.service",
            FsPath::new("/var/lib/arg/deployments/apps/app/current"),
            FsPath::new("/var/lib/arg/deployments/apps/app/shared"),
            "argapp-12345678-123",
            FsPath::new("/usr/bin/node"),
            FsPath::new("/usr/bin/pnpm"),
            31234,
            "/health",
            RuntimeEntrypoint::PackageScript("start".into()),
        )
        .unwrap();
        assert!(unit.contains("BindReadOnlyPaths=/var/lib/arg/deployments/apps/app/shared/.env:/shared/.env"));
        assert!(unit.contains("BindPaths=/var/lib/arg/deployments/apps/app/shared/data:/shared/data"));
        assert!(!unit.contains("BindPaths=/var/lib/arg/deployments/apps/app/shared:/shared"));
        assert!(unit.contains("ExecStart=/usr/bin/pnpm run start"));
        assert!(unit.contains("Environment=ARG_HEALTHCHECK_PATH=/health"));
        assert!(unit.contains("TimeoutStopSec=30"));
        let nuxt = render_node_unit(
            "arg-node-12345678-123.service",
            FsPath::new("/var/lib/arg/deployments/apps/app/current"),
            FsPath::new("/var/lib/arg/deployments/apps/app/shared"),
            "argapp-12345678-123",
            FsPath::new("/usr/bin/node"),
            FsPath::new("/usr/bin/npm"),
            31234,
            "/ready",
            RuntimeEntrypoint::NuxtOutput,
        ).unwrap();
        assert!(nuxt.contains("ExecStart=/usr/bin/node /workspace/.output/server/index.mjs"));
        assert!(nuxt.contains("Environment=NITRO_HOST=127.0.0.1"));
        assert!(nuxt.contains("Environment=NITRO_PORT=31234"));
        assert!(!nuxt.contains("run start"));
        let next = render_node_unit(
            "arg-node-12345678-123.service",
            FsPath::new("/var/lib/arg/deployments/apps/app/current"),
            FsPath::new("/var/lib/arg/deployments/apps/app/shared"),
            "argapp-12345678-123",
            FsPath::new("/usr/bin/node"),
            FsPath::new("/usr/bin/npm"),
            31234,
            "/api/health",
            RuntimeEntrypoint::NextCli,
        ).unwrap();
        assert!(next.contains("ExecStart=/usr/bin/node /workspace/node_modules/next/dist/bin/next start --hostname 127.0.0.1 --port 31234"));
    }
    #[test]
    fn loopback_verifier_rejects_public_addresses() {
        assert!("127.0.0.1:3000".starts_with("127.0.0.1:"));
        assert!(!"0.0.0.0:3000".starts_with("127.0.0.1:"));
        assert!(!"[::]:3000".starts_with("[::1]:"));
    }

    #[test]
    fn node_healthcheck_parses_http_and_rejects_server_errors() {
        let response = b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n";
        assert_eq!(parse_node_http_status(response), Some(204));
        assert_eq!(parse_node_http_status(b"HTTP/1.1 503 Service Unavailable\r\n\r\n"), Some(503));
        assert_eq!(parse_node_http_status(b"not HTTP\r\n\r\n"), None);
        assert!((200..300).contains(&parse_node_http_status(response).unwrap()));
        assert!(!(200..300).contains(&parse_node_http_status(b"HTTP/1.1 404 Not Found\r\n\r\n").unwrap()));
        assert!(!(200..300).contains(&parse_node_http_status(b"HTTP/1.1 503 Error\r\n\r\n").unwrap()));
    }

    #[test]
    fn runtime_script_and_health_path_inputs_are_not_shell_commands_or_headers() {
        assert!(valid_configured_script("start:prod", false));
        assert!(valid_configured_script("auto", true));
        assert!(valid_configured_script("none", true));
        assert!(!valid_configured_script("start; touch /tmp/pwned", false));
        let manifest = Manifest {
            name: None,
            version: None,
            package_manager: None,
            node_engine: None,
            scripts: vec!["start".to_owned(), "start:prod".to_owned()],
            manager: None,
            detected_framework: "node".to_owned(),
        };
        assert_eq!(runtime_entrypoint(&manifest, FsPath::new("/unused"), false, "start:prod"), Some(RuntimeEntrypoint::PackageScript("start:prod".to_owned())));
        assert_eq!(runtime_entrypoint(&manifest, FsPath::new("/unused"), false, "missing"), None);
        assert!(valid_healthcheck_path("/api/health?ready=true"));
        assert!(!valid_healthcheck_path("/health\\r\\nHost:evil"));
        assert!(!valid_healthcheck_path("//example.test/path"));
        assert!(!valid_healthcheck_path("/health#fragment"));
    }

    #[test]
    fn journal_cursor_is_bounded_and_argument_safe() {
        assert!(valid_journal_cursor("s=abc123;i=4;b=ef01;m=2;t=5;x=9"));
        assert!(!valid_journal_cursor("--output=json"));
        assert!(!valid_journal_cursor("x".repeat(513).as_str()));
    }
}
