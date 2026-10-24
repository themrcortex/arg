use super::*;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

#[derive(Deserialize)]
pub(super) struct FileQuery {
    path: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct DiskScanInput {
    #[serde(default = "default_scan_root")]
    path: String,
}

fn default_scan_root() -> String { "/".into() }

#[derive(Deserialize)]
pub(super) struct FileActionInput {
    action: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    new_name: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    destination: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    confirmed: bool,
    #[serde(default)]
    permissions: String,
}

pub(super) async fn list_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FileQuery>,
) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let path = query.path.unwrap_or_else(|| "/".into());
    let response = privileged_request_async(json!({"action":"file-list", "path":path})).await?;
    if response["ok"] != true {
        return Err(error(
            StatusCode::BAD_REQUEST,
            response["error"]
                .as_str()
                .unwrap_or("Could not list this directory"),
        ));
    }
    Ok(Json(response["data"].clone()))
}

pub(super) async fn scan_disk(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<DiskScanInput>,
) -> ApiResult {
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    let path = input.path.trim().to_owned();
    if path.is_empty() || path.len() > 4096 || !path.starts_with('/') || path.split('/').any(|part| part == "..") {
        return Err(error(StatusCode::BAD_REQUEST, "مسیر باید یک آدرس مطلق معتبر باشد."));
    }
    let response = privileged_request_async(json!({"action":"file-disk-scan", "path":path})).await?;
    if response["ok"] != true {
        return Err(error(StatusCode::BAD_REQUEST, response["error"].as_str().unwrap_or("Disk scan failed")));
    }
    let db = state.lock().unwrap();
    audit(&db, &actor, "files.disk.scan", &format!("path={path}"));
    Ok(Json(response["data"].clone()))
}

pub(super) async fn download_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FileQuery>,
) -> Result<Response, (StatusCode, Json<Value>)> {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let path = query.path.unwrap_or_default();
    let response = privileged_request_async(json!({"action":"file-download", "path":path})).await?;
    if response["ok"] != true {
        return Err(error(
            StatusCode::BAD_REQUEST,
            response["error"]
                .as_str()
                .unwrap_or("Could not download this file"),
        ));
    }
    let encoded = response["data"]["content"].as_str().unwrap_or_default();
    let bytes = BASE64.decode(encoded).map_err(|_| {
        error(
            StatusCode::BAD_GATEWAY,
            "Invalid file data returned by the privileged manager",
        )
    })?;
    let mut result_headers = HeaderMap::new();
    result_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    result_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    result_headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"download\""),
    );
    Ok((result_headers, bytes).into_response())
}

pub(super) async fn read_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FileQuery>,
) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let path = query.path.unwrap_or_default();
    let response = privileged_request_async(json!({"action":"file-read", "path":path})).await?;
    if response["ok"] != true {
        return Err(error(
            StatusCode::BAD_REQUEST,
            response["error"]
                .as_str()
                .unwrap_or("Could not read this file"),
        ));
    }
    Ok(Json(response["data"].clone()))
}

pub(super) async fn file_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<FileActionInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    let allowed = [
        "mkdir", "create", "save", "rename", "delete", "paste", "upload", "chmod",
    ];
    if !allowed.contains(&input.action.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "Unsupported file operation"));
    }
    if input.content.len() > 3 * 1024 * 1024 {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "File content exceeds the 2 MiB upload limit",
        ));
    }
    let response = privileged_request_async(json!({
        "action":"file-action", "operation":input.action, "path":input.path,
        "name":input.name, "new_name":input.new_name, "source":input.source,
        "destination":input.destination, "mode":input.mode, "content":input.content, "confirmed":input.confirmed, "permissions":input.permissions
    })).await?;
    if response["ok"] != true {
        return Err(error(
            StatusCode::BAD_REQUEST,
            response["error"]
                .as_str()
                .unwrap_or("File operation failed"),
        ));
    }
    let detail = match input.action.as_str() {
        "delete" => format!("path={}", input.path),
        "paste" => format!("{} -> {}", input.source, input.destination),
        _ => format!("path={} name={}", input.path, input.name),
    };
    let db = state.lock().unwrap();
    audit(&db, &actor, &format!("files.{}", input.action), &detail);
    Ok(Json(response["data"].clone()))
}

fn canonical_existing(path: &str) -> Result<std::path::PathBuf, String> {
    if path.len() > 4096 || !path.starts_with('/') || path.split('/').any(|part| part == "..") {
        return Err("Use a valid absolute path without parent-directory segments.".into());
    }
    std::fs::canonicalize(path).map_err(|error| format!("Could not open path: {error}"))
}

fn lexical_existing(path: &str) -> Result<std::path::PathBuf, String> {
    if path.len() > 4096 || !path.starts_with('/') || path.split('/').any(|part| part == "..") {
        return Err("Use a valid absolute path without parent-directory segments.".into());
    }
    let requested = std::path::Path::new(path);
    let parent = requested.parent().ok_or("Invalid path")?;
    let name = requested
        .file_name()
        .ok_or("Cannot modify a filesystem root")?;
    let parent = std::fs::canonicalize(parent)
        .map_err(|error| format!("Could not access parent directory: {error}"))?;
    let target = parent.join(name);
    std::fs::symlink_metadata(&target)
        .map_err(|error| format!("Could not access path: {error}"))?;
    Ok(target)
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.chars().any(char::is_control)
}

fn valid_mode(mode: &str) -> bool {
    (mode.len() == 3 || mode.len() == 4) && mode.bytes().all(|byte| matches!(byte, b'0'..=b'7'))
}

fn is_protected_system_root(path: &std::path::Path) -> bool {
    let protected = [
        "etc", "usr", "bin", "sbin", "lib", "lib64", "boot", "proc", "sys", "dev", "run", "var",
    ];
    path.parent()
        .is_some_and(|parent| parent == std::path::Path::new("/"))
        && path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|name| protected.contains(&name))
}

fn copy_tree_transactional(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), String> {
    let mut count = 0;
    if let Err(error) = copy_tree(source, destination, 0, &mut count) {
        let _ = remove_tree(destination);
        return Err(error);
    }
    Ok(())
}

fn path_metadata(path: &std::path::Path, name: String) -> Value {
    use std::os::unix::fs::PermissionsExt;
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Value::Null;
    };
    let kind = if metadata.file_type().is_symlink() {
        "symlink"
    } else if metadata.is_dir() {
        "directory"
    } else if metadata.is_file() {
        "file"
    } else {
        "other"
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    json!({
        "name":name,
        "path":path.to_string_lossy(),
        "kind":kind,
        "size":if metadata.is_file() { metadata.len() } else { 0 },
        "modified":modified,
        "permissions":format!("{:04o}", metadata.permissions().mode() & 0o7777),
        "read_only":metadata.permissions().readonly()
    })
}

fn list_directory(path: &str) -> Result<Value, String> {
    let directory = canonical_existing(path)?;
    if !directory.is_dir() {
        return Err("The selected path is not a directory.".into());
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&directory)
        .map_err(|error| error.to_string())?
        .take(2000)
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let item = path_metadata(
            &entry.path(),
            entry.file_name().to_string_lossy().into_owned(),
        );
        if !item.is_null() {
            entries.push(item);
        }
    }
    entries.sort_by(|a, b| {
        let a_dir = a["kind"] == "directory";
        let b_dir = b["kind"] == "directory";
        b_dir
            .cmp(&a_dir)
            .then_with(|| a["name"].as_str().cmp(&b["name"].as_str()))
    });
    Ok(
        json!({"path":directory.to_string_lossy(), "parent":directory.parent().map(|parent| parent.to_string_lossy()), "entries":entries, "truncated":entries.len() >= 2000}),
    )
}

struct DiskScanState {
    device: u64,
    entries: usize,
    max_entries: usize,
    started: std::time::Instant,
    max_duration: std::time::Duration,
    truncated: bool,
    files: Vec<(String, u64)>,
    directories: Vec<(String, u64)>,
    seen_files: std::collections::HashSet<(u64, u64)>,
}

fn scan_directory_size(path: &std::path::Path, depth: usize, state: &mut DiskScanState) -> u64 {
    use std::os::unix::fs::MetadataExt;
    if depth > 64 || state.entries >= state.max_entries || state.started.elapsed() >= state.max_duration {
        state.truncated = true;
        return 0;
    }
    state.entries += 1;
    let Ok(metadata) = std::fs::symlink_metadata(path) else { return 0 };
    if metadata.dev() != state.device || metadata.file_type().is_symlink() { return 0; }
    if metadata.is_file() {
        if state.seen_files.insert((metadata.dev(), metadata.ino())) {
            let allocated = metadata.blocks().saturating_mul(512);
            state.files.push((path.to_string_lossy().into_owned(), allocated));
            return allocated;
        }
        return 0;
    }
    if !metadata.is_dir() { return 0; }
    let mut total = 0_u64;
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    for entry in entries {
        if state.entries >= state.max_entries || state.started.elapsed() >= state.max_duration {
            state.truncated = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        let child = entry.path();
        let child_name = entry.file_name();
        if depth == 0 && path == std::path::Path::new("/")
            && ["proc", "sys", "dev", "run"].iter().any(|name| child_name == *name) {
            continue;
        }
        let size = scan_directory_size(&child, depth + 1, state);
        total = total.saturating_add(size);
    }
    state.directories.push((path.to_string_lossy().into_owned(), total));
    total
}

fn scan_disk_usage(path: &str) -> Result<Value, String> {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::ffi::OsStrExt;
    let root = canonical_existing(path)?;
    if !root.is_dir() { return Err("مسیر انتخاب‌شده پوشه نیست.".into()); }
    let metadata = std::fs::metadata(&root).map_err(|error| error.to_string())?;
    let mut state = DiskScanState {
        device: metadata.dev(), entries: 0, max_entries: 100_000,
        started: std::time::Instant::now(), max_duration: std::time::Duration::from_secs(12),
        truncated: false, files: Vec::new(), directories: Vec::new(),
        seen_files: std::collections::HashSet::new(),
    };
    let total_bytes = scan_directory_size(&root, 0, &mut state);
    let c_root = std::ffi::CString::new(root.as_os_str().as_bytes()).map_err(|_| "مسیر معتبر نیست.")?;
    let mut volume = unsafe { std::mem::zeroed::<libc::statvfs>() };
    let (volume_total, volume_used, volume_available) = if unsafe { libc::statvfs(c_root.as_ptr(), &mut volume) } == 0 {
        let block_size = volume.f_frsize as u64;
        let total = (volume.f_blocks as u64).saturating_mul(block_size);
        let used = (volume.f_blocks.saturating_sub(volume.f_bfree) as u64).saturating_mul(block_size);
        let available = (volume.f_bavail as u64).saturating_mul(block_size);
        (total, used, available)
    } else { (0, 0, 0) };
    state.files.sort_by(|a, b| b.1.cmp(&a.1));
    state.directories.sort_by(|a, b| b.1.cmp(&a.1));
    let largest_files = state.files.into_iter().take(100).map(|(path, bytes)| json!({"path":path,"bytes":bytes})).collect::<Vec<_>>();
    let largest_directories = state.directories.into_iter().take(100).map(|(path, bytes)| json!({"path":path,"bytes":bytes})).collect::<Vec<_>>();
    let usage_percent = if volume_total == 0 { 0.0 } else { volume_used as f64 * 100.0 / volume_total as f64 };
    Ok(json!({"root":root,"total_bytes":total_bytes,"volume_total":volume_total,"volume_used":volume_used,"volume_available":volume_available,"usage_percent":usage_percent,"entries_scanned":state.entries,"truncated":state.truncated,"elapsed_ms":state.started.elapsed().as_millis(),"largest_files":largest_files,"largest_directories":largest_directories,"note":"Symbolic links are skipped; nested filesystems and pseudo-filesystems are excluded. Select a mount path directly to scan it."}))
}

fn copy_tree(
    source: &std::path::Path,
    destination: &std::path::Path,
    depth: usize,
    count: &mut usize,
) -> Result<(), String> {
    if depth > 64 {
        return Err("Directory nesting is too deep to copy safely.".into());
    }
    *count += 1;
    if *count > 10000 {
        return Err("A copy operation is limited to 10,000 filesystem entries.".into());
    }
    let metadata = std::fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("Copying symbolic links is disabled to prevent copying files outside the selected tree.".into());
    }
    if metadata.is_file() {
        std::fs::copy(source, destination).map_err(|error| error.to_string())?;
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err("Only regular files and directories can be copied.".into());
    }
    std::fs::create_dir(destination).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        copy_tree(
            &entry.path(),
            &destination.join(entry.file_name()),
            depth + 1,
            count,
        )?;
    }
    Ok(())
}

fn remove_tree(path: &std::path::Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        std::fs::remove_file(path).map_err(|error| error.to_string())
    } else if metadata.is_dir() {
        std::fs::remove_dir_all(path).map_err(|error| error.to_string())
    } else {
        Err("Unsupported filesystem object.".into())
    }
}

fn save_file_atomically(target: &std::path::Path, content: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(target).map_err(|error| error.to_string())?;
    let parent = target.parent().ok_or("Invalid file path")?;
    for _ in 0..8 {
        let temporary = parent.join(format!(
            ".arg-edit-{}-{:016x}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        };
        let result = (|| {
            file.write_all(content).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            drop(file);
            let path = std::ffi::CString::new(temporary.to_string_lossy().as_bytes())
                .map_err(|_| "Invalid temporary file path")?;
            if unsafe { libc::chown(path.as_ptr(), metadata.uid(), metadata.gid()) } != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            std::fs::set_permissions(&temporary, metadata.permissions())
                .map_err(|error| error.to_string())?;
            std::fs::rename(&temporary, target).map_err(|error| error.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        return result;
    }
    Err("Could not create a unique temporary file.".into())
}

pub(super) fn execute_privileged_request(action: &str, request: &Value) -> Value {
    let result = match action {
        "file-disk-scan" => request["path"].as_str().ok_or_else(|| "A scan path is required.".to_owned()).and_then(scan_disk_usage),
        "file-list" => request["path"]
            .as_str()
            .ok_or_else(|| "A path is required.".to_owned())
            .and_then(list_directory),
        "file-download" => request["path"]
            .as_str()
            .ok_or_else(|| "A file path is required.".to_owned())
            .and_then(|path| {
                let path = canonical_existing(path)?;
                let metadata = std::fs::metadata(&path).map_err(|error| error.to_string())?;
                if !metadata.is_file() {
                    return Err("Select a regular file to download.".into());
                }
                if metadata.len() > 16 * 1024 * 1024 {
                    return Err("Downloads are limited to 16 MiB.".into());
                }
                let content = std::fs::read(path).map_err(|error| error.to_string())?;
                Ok(json!({"content":BASE64.encode(content)}))
            }),
        "file-read" => request["path"]
            .as_str()
            .ok_or_else(|| "A file path is required.".to_owned())
            .and_then(|path| {
                let path = canonical_existing(path)?;
                let metadata = std::fs::metadata(&path).map_err(|error| error.to_string())?;
                if !metadata.is_file() {
                    return Err("Select a regular file to open in the editor.".into());
                }
                if metadata.len() > 1024 * 1024 {
                    return Err(
                        "The editor is limited to 1 MiB per file. Use Download for larger files."
                            .into(),
                    );
                }
                let content = std::fs::read_to_string(path).map_err(|error| {
                    format!("This file is binary or is not valid UTF-8: {error}")
                })?;
                Ok(json!({"content":content}))
            }),
        "file-action" => execute_action(request),
        _ => Err("Unsupported file manager action.".into()),
    };
    match result {
        Ok(data) => json!({"ok":true,"data":data}),
        Err(error) => json!({"ok":false,"error":error}),
    }
}

fn execute_action(request: &Value) -> Result<Value, String> {
    let operation = request["operation"]
        .as_str()
        .ok_or("Missing file operation")?;
    match operation {
        "mkdir" | "create" | "upload" => {
            let directory = canonical_existing(
                request["path"]
                    .as_str()
                    .ok_or("A parent directory is required")?,
            )?;
            if !directory.is_dir() {
                return Err("The destination is not a directory.".into());
            }
            let name = request["name"].as_str().ok_or("A file name is required")?;
            if !safe_name(name) {
                return Err("Use a simple file name without slashes or control characters.".into());
            }
            let target = directory.join(name);
            match operation {
                "mkdir" => std::fs::create_dir(&target).map_err(|error| error.to_string())?,
                "create" => {
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&target)
                        .map_err(|error| error.to_string())?;
                }
                _ => {
                    let encoded = request["content"].as_str().ok_or("Missing upload data")?;
                    let content = BASE64
                        .decode(encoded)
                        .map_err(|_| "Invalid upload encoding")?;
                    if content.len() > 2 * 1024 * 1024 {
                        return Err("Uploads are limited to 2 MiB per file.".into());
                    }
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&target)
                        .map_err(|error| error.to_string())?;
                    if let Err(error) = file.write_all(&content) {
                        drop(file);
                        let _ = std::fs::remove_file(&target);
                        return Err(error.to_string());
                    }
                }
            }
            Ok(path_metadata(&target, name.into()))
        }
        "save" => {
            let target =
                canonical_existing(request["path"].as_str().ok_or("A file path is required")?)?;
            if !target.is_file() {
                return Err("Only regular files can be edited.".into());
            }
            let encoded = request["content"].as_str().ok_or("Missing file content")?;
            let content = BASE64
                .decode(encoded)
                .map_err(|_| "Invalid file encoding")?;
            if content.len() > 1024 * 1024 {
                return Err("The editor is limited to 1 MiB per file.".into());
            }
            save_file_atomically(&target, &content)?;
            Ok(path_metadata(
                &target,
                target
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ))
        }
        "rename" => {
            let source = lexical_existing(
                request["path"]
                    .as_str()
                    .ok_or("A source path is required")?,
            )?;
            if is_protected_system_root(&source) {
                return Err("Renaming a top-level system directory is blocked.".into());
            }
            let new_name = request["new_name"]
                .as_str()
                .ok_or("A new name is required")?;
            if !safe_name(new_name) {
                return Err("Use a simple file name without slashes or control characters.".into());
            }
            let destination = source
                .parent()
                .ok_or("Cannot rename this filesystem root")?
                .join(new_name);
            if destination.exists() {
                return Err("A file with that name already exists.".into());
            }
            std::fs::rename(&source, &destination).map_err(|error| error.to_string())?;
            Ok(path_metadata(&destination, new_name.into()))
        }
        "chmod" => {
            use std::os::unix::fs::PermissionsExt;
            let source = lexical_existing(request["path"].as_str().ok_or("A path is required")?)?;
            let mode = request["permissions"]
                .as_str()
                .ok_or("A permission mode is required")?;
            if !valid_mode(mode) {
                return Err(
                    "Enter permissions as 3 or 4 octal digits, for example 644 or 0755.".into(),
                );
            }
            if source == std::path::Path::new("/") || is_protected_system_root(&source) {
                return Err(
                    "Changing permissions on a top-level system directory is blocked.".into(),
                );
            }
            if std::fs::symlink_metadata(&source)
                .map_err(|error| error.to_string())?
                .file_type()
                .is_symlink()
            {
                return Err("Changing permissions on symbolic links is not supported.".into());
            }
            std::fs::set_permissions(
                &source,
                std::fs::Permissions::from_mode(
                    u32::from_str_radix(mode, 8).map_err(|error| error.to_string())?,
                ),
            )
            .map_err(|error| error.to_string())?;
            Ok(path_metadata(
                &source,
                source
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ))
        }
        "delete" => {
            let source = lexical_existing(request["path"].as_str().ok_or("A path is required")?)?;
            if request["confirmed"] != true {
                return Err("Confirm deletion before continuing.".into());
            }
            if source.parent().is_none() {
                return Err("The filesystem root cannot be deleted.".into());
            }
            if is_protected_system_root(&source) {
                return Err("Deleting a top-level system directory is blocked.".into());
            }
            let name = source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            remove_tree(&source)?;
            Ok(json!({"deleted":name}))
        }
        "paste" => {
            let source = lexical_existing(
                request["source"]
                    .as_str()
                    .ok_or("A source path is required")?,
            )?;
            let destination_dir = canonical_existing(
                request["destination"]
                    .as_str()
                    .ok_or("A destination directory is required")?,
            )?;
            if !destination_dir.is_dir() {
                return Err("The destination is not a directory.".into());
            }
            if destination_dir.starts_with(&source) {
                return Err(
                    "A directory cannot be copied into itself or one of its descendants.".into(),
                );
            }
            let name = source
                .file_name()
                .ok_or("Cannot copy this filesystem root")?;
            let destination = destination_dir.join(name);
            if destination.exists() {
                return Err("An item with that name already exists in the destination.".into());
            }
            let operation = request["mode"].as_str().unwrap_or("copy");
            if operation == "move" {
                if request["confirmed"] != true {
                    return Err("Confirm moving this item before continuing.".into());
                }
                if is_protected_system_root(&source) {
                    return Err("Moving a top-level system directory is blocked.".into());
                }
                match std::fs::rename(&source, &destination) {
                    Ok(()) => {}
                    Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {
                        copy_tree_transactional(&source, &destination)?;
                        remove_tree(&source)?;
                    }
                    Err(error) => return Err(error.to_string()),
                }
            } else if operation == "copy" {
                copy_tree_transactional(&source, &destination)?;
            } else {
                return Err("Choose copy or move.".into());
            }
            Ok(path_metadata(
                &destination,
                name.to_string_lossy().into_owned(),
            ))
        }
        _ => Err("Unsupported file operation.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{copy_tree, remove_tree, safe_name, valid_mode};
    use std::{fs, os::unix::fs::symlink, path::PathBuf};

    fn test_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "arg-file-manager-test-{}-{}",
            std::process::id(),
            super::now()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn file_names_are_single_safe_path_components() {
        assert!(safe_name("site.conf"));
        assert!(safe_name(".env"));
        assert!(!safe_name("../shadow"));
        assert!(!safe_name("folder/file"));
        assert!(!safe_name("line\nbreak"));
        assert!(valid_mode("0644"));
        assert!(valid_mode("755"));
        assert!(!valid_mode("0899"));
        assert!(!valid_mode("77777"));
    }

    #[test]
    fn deleting_a_symlink_does_not_delete_its_target() {
        let root = test_dir();
        let target = root.join("keep.txt");
        let link = root.join("link.txt");
        fs::write(&target, "keep").unwrap();
        symlink(&target, &link).unwrap();
        remove_tree(&link).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "keep");
        assert!(fs::symlink_metadata(&link).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recursive_copy_rejects_symlinks_and_copies_regular_tree() {
        let root = test_dir();
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("file.txt"), "content").unwrap();
        copy_tree(&source, &destination, 0, &mut 0).unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("file.txt")).unwrap(),
            "content"
        );
        symlink(source.join("file.txt"), source.join("link")).unwrap();
        assert!(copy_tree(&source, &root.join("rejected"), 0, &mut 0).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
