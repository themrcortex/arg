use super::*;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::{
    ffi::CString,
    fs::File,
    io::Read,
    os::{fd::{AsRawFd, FromRawFd}, unix::ffi::OsStrExt},
    path::Component,
};

const MAX_FILE_UPLOAD_BYTES: usize = 2 * 1024 * 1024;
const MAX_EDITOR_FILE_BYTES: usize = 1024 * 1024;
const FILE_OPERATION_FREE_SPACE_RESERVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SCAN_LARGEST_FILES: usize = 100;
const MAX_SCAN_LARGEST_DIRECTORIES: usize = 100;
const MAX_SCAN_DIRECTORY_CHILDREN: usize = 2000;
static DISK_SCAN_LOCK: std::sync::LazyLock<Arc<tokio::sync::Semaphore>> =
    std::sync::LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(1)));

fn try_disk_scan_permit() -> Result<tokio::sync::OwnedSemaphorePermit, String> {
    DISK_SCAN_LOCK
        .clone()
        .try_acquire_owned()
        .map_err(|_| "A disk scan is already running; retry when it finishes.".to_owned())
}

#[derive(Deserialize)]
pub(super) struct FileQuery {
    path: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct DiskScanInput {
    #[serde(default = "default_scan_root")]
    path: String,
    #[serde(default = "default_scan_mode")]
    mode: String,
}

fn default_scan_root() -> String {
    "/".into()
}

fn default_scan_mode() -> String {
    "standard".into()
}

fn max_base64_encoded_bytes(raw_limit: usize) -> usize {
    raw_limit.saturating_add(2).saturating_div(3).saturating_mul(4)
}

fn file_content_limit(action: &str) -> Option<usize> {
    match action {
        "upload" => Some(MAX_FILE_UPLOAD_BYTES),
        "save" => Some(MAX_EDITOR_FILE_BYTES),
        _ => None,
    }
}

fn file_content_within_limit(action: &str, encoded_len: usize) -> bool {
    file_content_limit(action)
        .map(|limit| encoded_len <= max_base64_encoded_bytes(limit))
        .unwrap_or(true)
}

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
    #[serde(default)]
    link_target: String,
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
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    let path = input.path.trim().to_owned();
    if path.is_empty()
        || path.len() > 4096
        || !path.starts_with('/')
        || path.split('/').any(|part| part == "..")
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "مسیر باید یک آدرس مطلق معتبر باشد.",
        ));
    }
    if !["standard", "deep"].contains(&input.mode.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "حالت اسکن معتبر نیست."));
    }
    let _scan_permit = try_disk_scan_permit()
        .map_err(|message| error(StatusCode::TOO_MANY_REQUESTS, &message))?;
    let response = privileged_request_async(json!({"action":"file-disk-scan", "path":path, "mode":input.mode})).await?;
    if response["ok"] != true {
        return Err(error(
            StatusCode::BAD_REQUEST,
            response["error"].as_str().unwrap_or("Disk scan failed"),
        ));
    }
    let db = state.lock().unwrap();
    audit(&db, &actor, "files.disk.scan", &format!("path={path} mode={}",input.mode));
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
        "mkdir", "create", "save", "rename", "delete", "paste", "upload", "chmod", "zip", "unzip", "symlink-update",
    ];
    if !allowed.contains(&input.action.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "Unsupported file operation"));
    }
    if !file_content_within_limit(&input.action, input.content.len()) {
        let message = if input.action == "save" {
            "Edited file content exceeds the 1 MiB limit"
        } else {
            "File content exceeds the 2 MiB upload limit"
        };
        return Err(error(StatusCode::PAYLOAD_TOO_LARGE, message));
    }
    let response = privileged_request_async(json!({
        "action":"file-action", "operation":input.action, "path":input.path,
        "name":input.name, "new_name":input.new_name, "source":input.source,
        "destination":input.destination, "mode":input.mode, "content":input.content, "confirmed":input.confirmed, "permissions":input.permissions, "link_target":input.link_target
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

fn validated_absolute_path(path: &str) -> Result<std::path::PathBuf, String> {
    if path.len() > 4096 || !path.starts_with('/') || path.split('/').any(|part| part == "..") {
        return Err("Use a valid absolute path without parent-directory segments.".into());
    }
    Ok(std::path::PathBuf::from(path))
}

fn canonical_existing(path: &str) -> Result<std::path::PathBuf, String> {
    let path = validated_absolute_path(path)?;
    std::fs::canonicalize(path).map_err(|error| format!("Could not open path: {error}"))
}

struct AnchoredEntry {
    parent: File,
    name: CString,
    path: std::path::PathBuf,
}

fn c_name(name: &std::ffi::OsStr) -> Result<CString, String> {
    CString::new(name.as_bytes()).map_err(|_| "Path contains a NUL byte.".to_owned())
}

fn openat_file(parent: &File, name: &CString, flags: i32, mode: u32) -> std::io::Result<File> {
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags, mode as libc::mode_t) };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

fn open_directory_descriptor(path: &std::path::Path) -> Result<File, String> {
    if !path.is_absolute() || path.components().any(|part| matches!(part, Component::ParentDir)) {
        return Err("Use a valid absolute directory path.".into());
    }
    let root = CString::new("/").map_err(|error| error.to_string())?;
    let root_fd = unsafe { libc::open(root.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) };
    if root_fd < 0 { return Err(std::io::Error::last_os_error().to_string()); }
    let mut current = unsafe { File::from_raw_fd(root_fd) };
    for component in path.components() {
        let Component::Normal(name) = component else { continue };
        let name = c_name(name)?;
        current = openat_file(
            &current,
            &name,
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0,
        ).map_err(|error| format!("Could not securely open directory component: {error}"))?;
    }
    Ok(current)
}

fn anchor_directory(path: &str) -> Result<(std::path::PathBuf, File), String> {
    let requested = validated_absolute_path(path)?;
    let descriptor = open_directory_descriptor(&requested)?;
    if !descriptor.metadata().map_err(|error| error.to_string())?.is_dir() {
        return Err("The destination is not a directory.".into());
    }
    let actual = std::fs::canonicalize(format!("/proc/self/fd/{}", descriptor.as_raw_fd()))
        .map_err(|error| format!("Could not resolve the opened destination directory: {error}"))?;
    Ok((actual, descriptor))
}

fn anchor_existing_entry(path: &str) -> Result<AnchoredEntry, String> {
    let requested = validated_absolute_path(path)?;
    let parent_path = requested.parent().ok_or("Cannot operate on the filesystem root")?;
    let parent = open_directory_descriptor(parent_path)?;
    let name = c_name(requested.file_name().ok_or("Invalid file name")?)?;
    entry_metadata(&parent, &name).map_err(|error| format!("Could not access path securely: {error}"))?;
    let actual_parent = std::fs::canonicalize(format!("/proc/self/fd/{}", parent.as_raw_fd()))
        .map_err(|error| format!("Could not resolve the opened parent directory: {error}"))?;
    Ok(AnchoredEntry { parent, name: name.clone(), path: actual_parent.join(std::ffi::OsStr::from_bytes(name.as_bytes())) })
}

fn entry_metadata(parent: &File, name: &CString) -> std::io::Result<std::fs::Metadata> {
    let entry = openat_file(parent, name, libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0)?;
    entry.metadata()
}

fn open_readonly_entry(parent: &File, name: &CString) -> std::io::Result<File> {
    openat_file(parent, name, libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK, 0)
}

fn available_bytes_for_fd(fd: std::os::fd::RawFd) -> Result<u64, String> {
    let mut stats = unsafe { std::mem::zeroed::<libc::statvfs>() };
    if unsafe { libc::fstatvfs(fd, &mut stats) } != 0 {
        return Err(format!("Could not check destination free space: {}", std::io::Error::last_os_error()));
    }
    let block_size = if stats.f_frsize == 0 { stats.f_bsize } else { stats.f_frsize } as u64;
    Ok((stats.f_bavail as u64).saturating_mul(block_size))
}

fn copy_space_sufficient(available: u64, incoming_bytes: u64) -> bool {
    incoming_bytes
        .checked_add(FILE_OPERATION_FREE_SPACE_RESERVE_BYTES)
        .is_some_and(|required| available >= required)
}

fn ensure_copy_space(directory: &File, incoming_bytes: u64) -> Result<(), String> {
    ensure_copy_space_fd(directory.as_raw_fd(), incoming_bytes)
}

fn ensure_copy_space_fd(fd: std::os::fd::RawFd, incoming_bytes: u64) -> Result<(), String> {
    let available = available_bytes_for_fd(fd)?;
    if copy_space_sufficient(available, incoming_bytes) {
        Ok(())
    } else {
        Err("The operation would consume the 64 MiB free-space reserve; free space on the destination first.".into())
    }
}

fn dir_entry_names(directory: &File) -> Result<Vec<std::ffi::OsString>, String> {
    let descriptor_path = format!("/proc/self/fd/{}", directory.as_raw_fd());
    std::fs::read_dir(descriptor_path)
        .map_err(|error| error.to_string())?
        .map(|entry| entry.map(|entry| entry.file_name()).map_err(|error| error.to_string()))
        .collect()
}

fn open_or_create_directory(parent: &File, name: &CString) -> Result<File, String> {
    match openat_file(parent, name, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0) {
        Ok(directory) => Ok(directory),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let result = unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o755) };
            if result != 0 {
                let create_error = std::io::Error::last_os_error();
                if create_error.kind() != std::io::ErrorKind::AlreadyExists { return Err(create_error.to_string()); }
            }
            openat_file(parent, name, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0)
                .map_err(|error| error.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn ensure_zip_parent(root: &File, relative_parent: &std::path::Path) -> Result<(File, CString), String> {
    let components = relative_parent.components().collect::<Vec<_>>();
    let mut current = root.try_clone().map_err(|error| error.to_string())?;
    for component in components.iter().take(components.len().saturating_sub(1)) {
        let Component::Normal(name) = component else { return Err("Archive contains an unsafe directory path.".into()); };
        let name = c_name(name)?;
        current = open_or_create_directory(&current, &name)?;
    }
    let last = components.last().and_then(|component| match component { Component::Normal(name) => Some(name), _ => None }).ok_or("Archive entry has an invalid file name")?;
    Ok((current, c_name(last)?))
}

fn unlinkat_entry(parent: &File, name: &CString, flags: i32) -> std::io::Result<()> {
    let result = unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if result == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

fn renameat_noreplace(source_parent: &File, source: &CString, destination_parent: &File, destination: &CString) -> std::io::Result<()> {
    let result = unsafe { libc::syscall(libc::SYS_renameat2, source_parent.as_raw_fd(), source.as_ptr(), destination_parent.as_raw_fd(), destination.as_ptr(), libc::RENAME_NOREPLACE) };
    if result == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

fn renameat_exchange(parent: &File, left: &CString, right: &CString) -> std::io::Result<()> {
    let result = unsafe { libc::syscall(libc::SYS_renameat2, parent.as_raw_fd(), left.as_ptr(), parent.as_raw_fd(), right.as_ptr(), libc::RENAME_EXCHANGE) };
    if result == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

fn remove_entry_tree(parent: &File, name: &CString, depth: usize) -> Result<(), String> {
    if depth > 256 { return Err("Directory nesting is too deep for safe removal.".into()); }
    let metadata = entry_metadata(parent, name).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        return unlinkat_entry(parent, name, 0).map_err(|error| error.to_string());
    }
    if !metadata.is_dir() { return Err("Unsupported filesystem object.".into()); }
    let directory = openat_file(parent, name, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0)
        .map_err(|error| error.to_string())?;
    for child in dir_entry_names(&directory)? {
        let child = c_name(&child)?;
        remove_entry_tree(&directory, &child, depth + 1)?;
    }
    unlinkat_entry(parent, name, libc::AT_REMOVEDIR).map_err(|error| error.to_string())
}

fn copy_entry_tree(source_parent: &File, source: &CString, destination_parent: &File, destination: &CString, depth: usize, count: &mut usize) -> Result<(), String> {
    if depth > 64 { return Err("Directory nesting is too deep to copy safely.".into()); }
    *count += 1;
    if *count > 10_000 { return Err("A copy operation is limited to 10,000 filesystem entries.".into()); }
    let metadata = entry_metadata(source_parent, source).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() { return Err("Copying symbolic links is disabled to prevent copying files outside the selected tree.".into()); }
    if metadata.is_file() {
        let input = open_readonly_entry(source_parent, source).map_err(|error| error.to_string())?;
        let input_metadata = input.metadata().map_err(|error| error.to_string())?;
        if !input_metadata.is_file() { return Err("Only regular files can be copied.".into()); }
        ensure_copy_space(destination_parent, input_metadata.len())?;
        let mut output = openat_file(destination_parent, destination, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0o600)
            .map_err(|error| error.to_string())?;
        let copied = std::io::copy(&mut input.take(input_metadata.len()), &mut output).map_err(|error| error.to_string())?;
        if copied != input_metadata.len() { return Err("The source file changed size during copy; the partial copy was discarded.".into()); }
        output.sync_all().map_err(|error| error.to_string())?;
        return Ok(());
    }
    if !metadata.is_dir() { return Err("Only regular files and directories can be copied.".into()); }
    let result = unsafe { libc::mkdirat(destination_parent.as_raw_fd(), destination.as_ptr(), 0o700) };
    if result != 0 { return Err(std::io::Error::last_os_error().to_string()); }
    let source_dir = openat_file(source_parent, source, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0)
        .map_err(|error| error.to_string())?;
    let destination_dir = openat_file(destination_parent, destination, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0)
        .map_err(|error| error.to_string())?;
    for child in dir_entry_names(&source_dir)? {
        let child_name = c_name(&child)?;
        copy_entry_tree(&source_dir, &child_name, &destination_dir, &child_name, depth + 1, count)?;
    }
    Ok(())
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
    let link_target = if metadata.file_type().is_symlink() {
        std::fs::read_link(path).ok().map(|target| target.to_string_lossy().into_owned())
    } else {
        None
    };
    json!({
        "name":name,
        "path":path.to_string_lossy(),
        "kind":kind,
        "size":if metadata.is_file() { metadata.len() } else { 0 },
        "modified":modified,
        "permissions":format!("{:04o}", metadata.permissions().mode() & 0o7777),
        "read_only":metadata.permissions().readonly(),
        "link_target":link_target
    })
}

fn list_directory(path: &str) -> Result<Value, String> {
    const MAX_DIRECTORY_ENTRIES: usize = 10_000;
    let directory = canonical_existing(path)?;
    if !directory.is_dir() {
        return Err("The selected path is not a directory.".into());
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&directory)
        .map_err(|error| error.to_string())?
        .take(MAX_DIRECTORY_ENTRIES)
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
        json!({"path":directory.to_string_lossy(), "parent":directory.parent().map(|parent| parent.to_string_lossy()), "entries":entries, "truncated":entries.len() >= MAX_DIRECTORY_ENTRIES}),
    )
}

struct LargestScanEntries {
    capacity: usize,
    entries: std::collections::BinaryHeap<std::cmp::Reverse<(u64, String)>>,
}

impl LargestScanEntries {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: std::collections::BinaryHeap::new(),
        }
    }

    fn insert(&mut self, path: String, bytes: u64) {
        let item = (bytes, path);
        if self.entries.len() < self.capacity {
            self.entries.push(std::cmp::Reverse(item));
            return;
        }
        if self
            .entries
            .peek()
            .is_some_and(|std::cmp::Reverse(smallest)| &item > smallest)
        {
            self.entries.pop();
            self.entries.push(std::cmp::Reverse(item));
        }
    }

    fn into_sorted(self) -> Vec<(String, u64)> {
        let mut entries = self
            .entries
            .into_iter()
            .map(|std::cmp::Reverse((bytes, path))| (path, bytes))
            .collect::<Vec<_>>();
        entries.sort_by(|(left_path, left_bytes), (right_path, right_bytes)| {
            right_bytes
                .cmp(left_bytes)
                .then_with(|| left_path.cmp(right_path))
        });
        entries
    }
}

struct DiskScanState {
    device: u64,
    entries: usize,
    max_entries: usize,
    started: std::time::Instant,
    max_duration: std::time::Duration,
    max_depth: usize,
    truncated: bool,
    largest_files: LargestScanEntries,
    largest_directories: LargestScanEntries,
    directory_children: LargestScanEntries,
    directory_children_seen: usize,
    seen_files: std::collections::HashSet<(u64, u64)>,
}

fn scan_directory_size(
    path: &std::path::Path,
    depth: usize,
    state: &mut DiskScanState,
) -> (u64, bool) {
    use std::os::unix::fs::MetadataExt;
    if depth > state.max_depth
        || state.entries >= state.max_entries
        || state.started.elapsed() >= state.max_duration
    {
        state.truncated = true;
        return (0, false);
    }
    state.entries += 1;
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return (0, false);
    };
    if metadata.dev() != state.device || metadata.file_type().is_symlink() {
        return (0, false);
    }
    if metadata.is_file() {
        if state.seen_files.insert((metadata.dev(), metadata.ino())) {
            let allocated = metadata.blocks().saturating_mul(512);
            state
                .largest_files
                .insert(path.to_string_lossy().into_owned(), allocated);
            return (allocated, false);
        }
        return (0, false);
    }
    if !metadata.is_dir() {
        return (0, false);
    }
    let mut total = 0_u64;
    let Ok(entries) = std::fs::read_dir(path) else {
        return (0, false);
    };
    for entry in entries {
        if state.entries >= state.max_entries || state.started.elapsed() >= state.max_duration {
            state.truncated = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        let child = entry.path();
        let child_name = entry.file_name();
        if depth == 0
            && path == std::path::Path::new("/")
            && ["proc", "sys", "dev", "run"]
                .iter()
                .any(|name| child_name == *name)
        {
            continue;
        }
        let (size, is_directory) = scan_directory_size(&child, depth + 1, state);
        total = total.saturating_add(size);
        if depth == 0 && is_directory {
            state.directory_children_seen = state.directory_children_seen.saturating_add(1);
            state
                .directory_children
                .insert(child.to_string_lossy().into_owned(), size);
        }
    }
    state
        .largest_directories
        .insert(path.to_string_lossy().into_owned(), total);
    (total, true)
}

fn scan_disk_usage(path: &str, mode: &str) -> Result<Value, String> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    let root = canonical_existing(path)?;
    if !root.is_dir() {
        return Err("مسیر انتخاب‌شده پوشه نیست.".into());
    }
    let metadata = std::fs::metadata(&root).map_err(|error| error.to_string())?;
    let (max_entries, max_duration, max_depth) = if mode == "deep" {
        (500_000, std::time::Duration::from_secs(90), 128)
    } else {
        (100_000, std::time::Duration::from_secs(12), 64)
    };
    let mut state = DiskScanState {
        device: metadata.dev(),
        entries: 0,
        max_entries,
        started: std::time::Instant::now(),
        max_duration,
        max_depth,
        truncated: false,
        largest_files: LargestScanEntries::new(MAX_SCAN_LARGEST_FILES),
        largest_directories: LargestScanEntries::new(MAX_SCAN_LARGEST_DIRECTORIES),
        directory_children: LargestScanEntries::new(MAX_SCAN_DIRECTORY_CHILDREN),
        directory_children_seen: 0,
        seen_files: std::collections::HashSet::new(),
    };
    let (total_bytes, _) = scan_directory_size(&root, 0, &mut state);
    let c_root =
        std::ffi::CString::new(root.as_os_str().as_bytes()).map_err(|_| "مسیر معتبر نیست.")?;
    let mut volume = unsafe { std::mem::zeroed::<libc::statvfs>() };
    let (volume_total, volume_used, volume_available) =
        if unsafe { libc::statvfs(c_root.as_ptr(), &mut volume) } == 0 {
            let block_size = volume.f_frsize as u64;
            let total = (volume.f_blocks as u64).saturating_mul(block_size);
            let used =
                (volume.f_blocks.saturating_sub(volume.f_bfree) as u64).saturating_mul(block_size);
            let available = (volume.f_bavail as u64).saturating_mul(block_size);
            (total, used, available)
        } else {
            (0, 0, 0)
        };
    let directory_children_truncated = state.directory_children_seen > MAX_SCAN_DIRECTORY_CHILDREN;
    let directory_children = std::mem::replace(
        &mut state.directory_children,
        LargestScanEntries::new(MAX_SCAN_DIRECTORY_CHILDREN),
    )
        .into_sorted()
        .into_iter()
        .map(|(path, bytes)| json!({"path":path,"bytes":bytes}))
        .collect::<Vec<_>>();
    let largest_files = std::mem::replace(
        &mut state.largest_files,
        LargestScanEntries::new(MAX_SCAN_LARGEST_FILES),
    )
        .into_sorted()
        .into_iter()
        .map(|(path, bytes)| json!({"path":path,"bytes":bytes}))
        .collect::<Vec<_>>();
    let largest_directories = state
        .largest_directories
        .into_sorted()
        .into_iter()
        .map(|(path, bytes)| json!({"path":path,"bytes":bytes}))
        .collect::<Vec<_>>();
    let usage_percent = if volume_total == 0 {
        0.0
    } else {
        volume_used as f64 * 100.0 / volume_total as f64
    };
    Ok(
        json!({"root":root,"mode":mode,"limits":{"max_entries":max_entries,"max_duration_seconds":max_duration.as_secs(),"max_depth":max_depth},"total_bytes":total_bytes,"volume_total":volume_total,"volume_used":volume_used,"volume_available":volume_available,"usage_percent":usage_percent,"entries_scanned":state.entries,"truncated":state.truncated,"directory_children_truncated":directory_children_truncated,"directory_children":directory_children,"elapsed_ms":state.started.elapsed().as_millis(),"largest_files":largest_files,"largest_directories":largest_directories,"note":"Symbolic links are skipped; nested filesystems and pseudo-filesystems are excluded. Select a mount path directly to scan it."}),
    )
}

#[cfg(test)]
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

#[cfg(test)]
fn remove_tree(path: &std::path::Path) -> Result<(), String> {
    let entry = anchor_existing_entry(path.to_str().ok_or("Invalid path")?)?;
    remove_entry_tree(&entry.parent, &entry.name, 0)
}

fn save_file_atomically(entry: &AnchoredEntry, content: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let original = open_readonly_entry(&entry.parent, &entry.name).map_err(|error| error.to_string())?;
    let metadata = original.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() { return Err("Only regular files can be edited.".into()); }
    for _ in 0..8 {
        let temp_name = CString::new(format!(".arg-edit-{}-{:016x}", std::process::id(), rand::random::<u64>())).map_err(|error| error.to_string())?;
        let mut file = match openat_file(&entry.parent, &temp_name, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0o600) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        };
        let mut displaced_target_at_temp = false;
        let result = (|| {
            file.write_all(content).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            if unsafe { libc::fchown(file.as_raw_fd(), metadata.uid(), metadata.gid()) } != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            if unsafe { libc::fchmod(file.as_raw_fd(), metadata.permissions().mode()) } != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            file.sync_all().map_err(|error| error.to_string())?;
            drop(file);
            renameat_exchange(&entry.parent, &temp_name, &entry.name).map_err(|error| error.to_string())?;
            displaced_target_at_temp = true;
            let displaced = entry_metadata(&entry.parent, &temp_name).map_err(|error| error.to_string())?;
            if displaced.dev() != metadata.dev() || displaced.ino() != metadata.ino() {
                renameat_exchange(&entry.parent, &temp_name, &entry.name).map_err(|error| format!("The file changed during save, and restoring it failed: {error}"))?;
                displaced_target_at_temp = false;
                return Err("The file changed while it was being saved; no replacement was applied.".into());
            }
            unlinkat_entry(&entry.parent, &temp_name, 0).map_err(|error| error.to_string())?;
            displaced_target_at_temp = false;
            Ok(())
        })();
        if result.is_err() && !displaced_target_at_temp {
            let _ = unlinkat_entry(&entry.parent, &temp_name, 0);
        }
        return result;
    }
    Err("Could not create a unique temporary file.".into())
}

pub(super) fn execute_privileged_request(action: &str, request: &Value) -> Value {
    let _disk_scan_permit = if action == "file-disk-scan" {
        match try_disk_scan_permit() {
            Ok(permit) => Some(permit),
            Err(error) => return json!({"ok":false,"error":error}),
        }
    } else {
        None
    };
    let result = match action {
        "file-disk-scan" => request["path"]
            .as_str()
            .ok_or_else(|| "A scan path is required.".to_owned())
            .and_then(|path| {
                let mode = request["mode"].as_str().unwrap_or("standard");
                if !["standard", "deep"].contains(&mode) {
                    return Err("Scan mode is invalid.".to_owned());
                }
                scan_disk_usage(path, mode)
            }),
        "file-list" => request["path"]
            .as_str()
            .ok_or_else(|| "A path is required.".to_owned())
            .and_then(list_directory),
        "file-download" => request["path"]
            .as_str()
            .ok_or_else(|| "A file path is required.".to_owned())
            .and_then(|path| {
                let entry = anchor_existing_entry(path)?;
                let file = open_readonly_entry(&entry.parent, &entry.name).map_err(|error| error.to_string())?;
                let metadata = file.metadata().map_err(|error| error.to_string())?;
                if !metadata.is_file() {
                    return Err("Select a regular file to download.".into());
                }
                if metadata.len() > 16 * 1024 * 1024 {
                    return Err("Downloads are limited to 16 MiB.".into());
                }
                let mut content = Vec::with_capacity(metadata.len() as usize);
                file.take(16 * 1024 * 1024 + 1).read_to_end(&mut content).map_err(|error| error.to_string())?;
                if content.len() > 16 * 1024 * 1024 { return Err("Downloads are limited to 16 MiB.".into()); }
                Ok(json!({"content":BASE64.encode(content)}))
            }),
        "file-read" => request["path"]
            .as_str()
            .ok_or_else(|| "A file path is required.".to_owned())
            .and_then(|path| {
                let entry = anchor_existing_entry(path)?;
                let file = open_readonly_entry(&entry.parent, &entry.name).map_err(|error| error.to_string())?;
                let metadata = file.metadata().map_err(|error| error.to_string())?;
                if !metadata.is_file() {
                    return Err("Select a regular file to open in the editor.".into());
                }
                if metadata.len() > 1024 * 1024 {
                    return Err(
                        "The editor is limited to 1 MiB per file. Use Download for larger files."
                            .into(),
                    );
                }
                let mut content = String::new();
                file.take(1024 * 1024 + 1).read_to_string(&mut content).map_err(|error| {
                    format!("This file is binary or is not valid UTF-8: {error}")
                })?;
                if content.len() > 1024 * 1024 { return Err("The editor is limited to 1 MiB per file. Use Download for larger files.".into()); }
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
        "zip" => create_zip_archive(request),
        "unzip" => extract_zip_archive(request),
        "mkdir" | "create" | "upload" => {
            let (directory, directory_fd) = anchor_directory(
                request["path"]
                    .as_str()
                    .ok_or("A parent directory is required")?,
            )?;
            let name = request["name"].as_str().ok_or("A file name is required")?;
            if !safe_name(name) {
                return Err("Use a simple file name without slashes or control characters.".into());
            }
            let name_c = CString::new(name).map_err(|error| error.to_string())?;
            let target = directory.join(name);
            match operation {
                "mkdir" => {
                    let result = unsafe { libc::mkdirat(directory_fd.as_raw_fd(), name_c.as_ptr(), 0o755) };
                    if result != 0 { return Err(std::io::Error::last_os_error().to_string()); }
                }
                "create" => {
                    openat_file(&directory_fd, &name_c, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0o644).map_err(|error| error.to_string())?;
                }
                _ => {
                    let encoded = request["content"].as_str().ok_or("Missing upload data")?;
                    let content = BASE64
                        .decode(encoded)
                        .map_err(|_| "Invalid upload encoding")?;
                    if content.len() > MAX_FILE_UPLOAD_BYTES {
                        return Err("Uploads are limited to 2 MiB per file.".into());
                    }
                    let mut file = openat_file(&directory_fd, &name_c, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0o644).map_err(|error| error.to_string())?;
                    if let Err(error) = file.write_all(&content) {
                        drop(file);
                        let _ = unlinkat_entry(&directory_fd, &name_c, 0);
                        return Err(error.to_string());
                    }
                    file.sync_all().map_err(|error| error.to_string())?;
                }
            }
            Ok(path_metadata(&target, name.into()))
        }
        "save" => {
            let entry = anchor_existing_entry(request["path"].as_str().ok_or("A file path is required")?)?;
            if !open_readonly_entry(&entry.parent, &entry.name).map_err(|error| error.to_string())?.metadata().map_err(|error| error.to_string())?.is_file() {
                return Err("Only regular files can be edited.".into());
            }
            let encoded = request["content"].as_str().ok_or("Missing file content")?;
            let content = BASE64
                .decode(encoded)
                .map_err(|_| "Invalid file encoding")?;
            if content.len() > MAX_EDITOR_FILE_BYTES {
                return Err("The editor is limited to 1 MiB per file.".into());
            }
            save_file_atomically(&entry, &content)?;
            Ok(path_metadata(
                &entry.path,
                entry.path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ))
        }
        "rename" => {
            let source = anchor_existing_entry(
                request["path"]
                    .as_str()
                    .ok_or("A source path is required")?,
            )?;
            if is_protected_system_root(&source.path) {
                return Err("Renaming a top-level system directory is blocked.".into());
            }
            let new_name = request["new_name"]
                .as_str()
                .ok_or("A new name is required")?;
            if !safe_name(new_name) {
                return Err("Use a simple file name without slashes or control characters.".into());
            }
            let new_name_c = CString::new(new_name).map_err(|error| error.to_string())?;
            renameat_noreplace(&source.parent, &source.name, &source.parent, &new_name_c).map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists { "A file with that name already exists.".to_owned() } else { error.to_string() }
            })?;
            let destination = source.path.parent().ok_or("Cannot rename this filesystem root")?.join(new_name);
            Ok(path_metadata(&destination, new_name.into()))
        }
        "symlink-update" => {
            use std::os::unix::fs::MetadataExt;
            let source = anchor_existing_entry(
                request["path"].as_str().ok_or("A symbolic link path is required")?,
            )?;
            if is_protected_system_root(&source.path) {
                return Err("Editing a symbolic link in a protected system location is blocked.".into());
            }
            let metadata = entry_metadata(&source.parent, &source.name).map_err(|error| error.to_string())?;
            if !metadata.file_type().is_symlink() {
                return Err("The selected entry is no longer a symbolic link.".into());
            }
            let target = request["link_target"].as_str().ok_or("A link target is required")?;
            if target.is_empty() || target.len() > 4096 || target.chars().any(char::is_control) {
                return Err("The link target must be between 1 and 4096 bytes and contain no control characters.".into());
            }
            let file_name = source.path.file_name().ok_or("Invalid symbolic link name")?.to_string_lossy();
            let mut temporary = None;
            for _ in 0..100 {
                let candidate = CString::new(format!(".arg-link-{:016x}", rand::random::<u64>())).map_err(|error| error.to_string())?;
                let target_c = CString::new(target).map_err(|error| error.to_string())?;
                let result = unsafe { libc::symlinkat(target_c.as_ptr(), source.parent.as_raw_fd(), candidate.as_ptr()) };
                if result == 0 {
                    temporary = Some(candidate);
                    break;
                }
                if std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists {
                    return Err("Could not prepare the new symbolic link.".into());
                }
            }
            let temporary = temporary.ok_or("Could not allocate a temporary symbolic link name")?;
            if let Err(error) = renameat_exchange(&source.parent, &temporary, &source.name) {
                let _ = unlinkat_entry(&source.parent, &temporary, 0);
                return Err(format!("Could not replace the symbolic link: {error}"));
            }
            let displaced = entry_metadata(&source.parent, &temporary).map_err(|error| error.to_string())?;
            if displaced.dev() != metadata.dev() || displaced.ino() != metadata.ino() {
                renameat_exchange(&source.parent, &temporary, &source.name).map_err(|error| format!("The link changed during update and restoring it failed: {error}"))?;
                let _ = unlinkat_entry(&source.parent, &temporary, 0);
                return Err("The symbolic link changed while it was being updated; no replacement was applied.".into());
            }
            unlinkat_entry(&source.parent, &temporary, 0).map_err(|error| error.to_string())?;
            Ok(path_metadata(&source.path, file_name.into_owned()))
        }
        "chmod" => {
            let source = anchor_existing_entry(request["path"].as_str().ok_or("A path is required")?)?;
            let mode = request["permissions"]
                .as_str()
                .ok_or("A permission mode is required")?;
            if !valid_mode(mode) {
                return Err(
                    "Enter permissions as 3 or 4 octal digits, for example 644 or 0755.".into(),
                );
            }
            if source.path == std::path::Path::new("/") || is_protected_system_root(&source.path) {
                return Err(
                    "Changing permissions on a top-level system directory is blocked.".into(),
                );
            }
            let file = open_readonly_entry(&source.parent, &source.name).map_err(|error| error.to_string())?;
            if file.metadata().map_err(|error| error.to_string())?.file_type().is_symlink() {
                return Err("Changing permissions on symbolic links is not supported.".into());
            }
            let mode = u32::from_str_radix(mode, 8).map_err(|error| error.to_string())?;
            if unsafe { libc::fchmod(file.as_raw_fd(), mode as libc::mode_t) } != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            Ok(path_metadata(
                &source.path,
                source.path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ))
        }
        "delete" => {
            let source = anchor_existing_entry(request["path"].as_str().ok_or("A path is required")?)?;
            if request["confirmed"] != true {
                return Err("Confirm deletion before continuing.".into());
            }
            if source.path.parent().is_none() {
                return Err("The filesystem root cannot be deleted.".into());
            }
            if is_protected_system_root(&source.path) {
                return Err("Deleting a top-level system directory is blocked.".into());
            }
            let name = source.path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            remove_entry_tree(&source.parent, &source.name, 0)?;
            Ok(json!({"deleted":name}))
        }
        "paste" => {
            let source = anchor_existing_entry(
                request["source"]
                    .as_str()
                    .ok_or("A source path is required")?,
            )?;
            let (destination_dir, destination_fd) = anchor_directory(
                request["destination"]
                    .as_str()
                    .ok_or("A destination directory is required")?,
            )?;
            if destination_dir.starts_with(&source.path) {
                return Err(
                    "A directory cannot be copied into itself or one of its descendants.".into(),
                );
            }
            let name = source.path
                .file_name()
                .ok_or("Cannot copy this filesystem root")?;
            let destination = destination_dir.join(name);
            let destination_name = c_name(name)?;
            let operation = request["mode"].as_str().unwrap_or("copy");
            if operation == "move" {
                if request["confirmed"] != true {
                    return Err("Confirm moving this item before continuing.".into());
                }
                if is_protected_system_root(&source.path) {
                    return Err("Moving a top-level system directory is blocked.".into());
                }
                match renameat_noreplace(&source.parent, &source.name, &destination_fd, &destination_name) {
                    Ok(()) => {}
                    Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {
                        let mut count = 0;
                        if let Err(copy_error) = copy_entry_tree(&source.parent, &source.name, &destination_fd, &destination_name, 0, &mut count) {
                            let _ = remove_entry_tree(&destination_fd, &destination_name, 0);
                            return Err(copy_error);
                        }
                        remove_entry_tree(&source.parent, &source.name, 0)?;
                    }
                    Err(error) => return Err(error.to_string()),
                }
            } else if operation == "copy" {
                let mut count = 0;
                if let Err(copy_error) = copy_entry_tree(&source.parent, &source.name, &destination_fd, &destination_name, 0, &mut count) {
                    let _ = remove_entry_tree(&destination_fd, &destination_name, 0);
                    return Err(copy_error);
                }
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

fn create_zip_archive(request: &Value) -> Result<Value, String> {
    use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

    let source = anchor_existing_entry(request["path"].as_str().ok_or("A source path is required")?)?;
    let metadata = entry_metadata(&source.parent, &source.name).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("Symbolic links cannot be archived directly.".into());
    }
    if source.path == std::path::Path::new("/") || is_protected_system_root(&source.path) {
        return Err("Creating an archive from a protected system root is blocked.".into());
    }
    if !(metadata.is_dir() || metadata.is_file()) {
        return Err("Select a regular file or directory to archive.".into());
    }
    let base_name = source.path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or("The source name is not UTF-8")?;
    if !safe_name(base_name) || base_name.contains('\\') {
        return Err(
            "Source names with control characters or backslashes cannot be archived.".into(),
        );
    }
    let archive_name = request["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .unwrap_or(&format!("{base_name}.zip"))
        .to_owned();
    if !safe_name(&archive_name) || !archive_name.to_ascii_lowercase().ends_with(".zip") {
        return Err("Enter a safe archive name ending in .zip.".into());
    }
    let archive_name_c = CString::new(archive_name.as_str()).map_err(|error| error.to_string())?;
    let output = openat_file(&source.parent, &archive_name_c, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0o600)
        .map_err(|error| if error.kind() == std::io::ErrorKind::AlreadyExists { "An item with that archive name already exists.".to_owned() } else { error.to_string() })?;
    let archive_path = source.path.parent().ok_or("Cannot archive a filesystem root")?.join(&archive_name);
    let mut writer = ZipWriter::new(output);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6))
        .unix_permissions(0o600);
    let mut count = 0usize;
    let mut total = 0u64;
    let result = if metadata.is_dir() {
        let root_entry = format!("{base_name}/");
        let root = openat_file(&source.parent, &source.name, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0).map_err(|error| error.to_string());
        match root {
            Ok(root) => writer.add_directory(&root_entry, options).map_err(|error| error.to_string()).and_then(|()| archive_directory(&mut writer, &root, base_name, options, &mut count, &mut total, 0)),
            Err(error) => Err(error),
        }
    } else {
        open_readonly_entry(&source.parent, &source.name)
            .map_err(|error| error.to_string())
            .and_then(|file| archive_file_opened(&mut writer, file, base_name, options, &mut count, &mut total))
    };
    if let Err(error) = result {
        let _ = writer.finish();
        let _ = unlinkat_entry(&source.parent, &archive_name_c, 0);
        return Err(error);
    }
    if let Err(error) = writer.finish() {
        let _ = unlinkat_entry(&source.parent, &archive_name_c, 0);
        return Err(error.to_string());
    }
    let display = archive_path.to_string_lossy().into_owned();
    Ok(
        json!({"archive":display,"entries":count,"source_bytes":total,"item":path_metadata(&archive_path,archive_name)}),
    )
}

fn archive_directory<W: Write + std::io::Seek + std::os::fd::AsRawFd>(
    writer: &mut zip::ZipWriter<W>,
    directory: &File,
    archive_prefix: &str,
    options: zip::write::SimpleFileOptions,
    count: &mut usize,
    total: &mut u64,
    depth: usize,
) -> Result<(), String> {
    const MAX_ARCHIVE_DIRECTORY_DEPTH: usize = 64;
    if depth > MAX_ARCHIVE_DIRECTORY_DEPTH {
        return Err("Archive directory nesting is limited to 64 levels.".into());
    }
    for name in dir_entry_names(directory)? {
        let display_name = name.to_str().ok_or("Archive paths must be valid UTF-8")?;
        if !safe_name(display_name) || display_name.contains('\\') {
            return Err("Archive paths cannot contain control characters or backslashes.".into());
        }
        let name_c = c_name(&name)?;
        let metadata = entry_metadata(directory, &name_c).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("Symbolic links are skipped for archive safety; remove the link or archive its target explicitly.".into());
        }
        let archive_name = format!("{archive_prefix}/{display_name}");
        if metadata.is_dir() {
            *count += 1;
            if *count > 50_000 {
                return Err("An archive can contain at most 50,000 items.".into());
            }
            writer
                .add_directory(format!("{archive_name}/"), options)
                .map_err(|error| error.to_string())?;
            let child = openat_file(directory, &name_c, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0).map_err(|error| error.to_string())?;
            archive_directory(writer, &child, &archive_name, options, count, total, depth + 1)?;
        } else if metadata.is_file() {
            let file = open_readonly_entry(directory, &name_c).map_err(|error| error.to_string())?;
            archive_file_opened(writer, file, &archive_name, options, count, total)?;
        } else {
            return Err("Special files cannot be included in an archive.".into());
        }
    }
    Ok(())
}

fn archive_file_opened<W: Write + std::io::Seek + std::os::fd::AsRawFd>(
    writer: &mut zip::ZipWriter<W>,
    file: File,
    archive_name: &str,
    options: zip::write::SimpleFileOptions,
    count: &mut usize,
    total: &mut u64,
) -> Result<(), String> {
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("Only regular files may be archived.".into());
    }
    *total = total.saturating_add(metadata.len());
    if *total > 2 * 1024 * 1024 * 1024 {
        return Err("Archive input is limited to 2 GiB.".into());
    }
    let output_fd = writer
        .get_ref()
        .ok_or("The archive output is no longer available.")?
        .as_raw_fd();
    ensure_copy_space_fd(output_fd, metadata.len())?;
    *count += 1;
    if *count > 50_000 {
        return Err("An archive can contain at most 50,000 items.".into());
    }
    writer
        .start_file(archive_name, options)
        .map_err(|error| error.to_string())?;
    let copied = std::io::copy(&mut file.take(metadata.len()), writer).map_err(|error| error.to_string())?;
    if copied != metadata.len() { return Err("The source file changed size while the archive was being created.".into()); }
    Ok(())
}

fn extract_zip_archive(request: &Value) -> Result<Value, String> {
    use zip::ZipArchive;

    if request["confirmed"] != true {
        return Err("Confirm ZIP extraction before continuing.".into());
    }
    let archive = anchor_existing_entry(request["path"].as_str().ok_or("A ZIP file is required")?)?;
    let archive_path = &archive.path;
    let (destination, destination_fd) = anchor_directory(
        request["destination"]
            .as_str()
            .ok_or("A destination directory is required")?,
    )?;
    if !entry_metadata(&archive.parent, &archive.name).map_err(|error| error.to_string())?.is_file()
        || !archive_path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        return Err("Choose a .zip file and an existing destination directory.".into());
    }
    if is_protected_system_root(&destination) || destination == std::path::Path::new("/") {
        return Err("Extracting into a protected system root is blocked.".into());
    }
    let folder_name = archive_path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or("ZIP filename must be UTF-8")?;
    if !safe_name(folder_name) {
        return Err("ZIP filename cannot be used as a destination folder name.".into());
    }
    let output_root = destination.join(folder_name);
    let output_root_name = CString::new(folder_name).map_err(|error| error.to_string())?;
    let made_root = unsafe { libc::mkdirat(destination_fd.as_raw_fd(), output_root_name.as_ptr(), 0o700) };
    if made_root != 0 {
        let error = std::io::Error::last_os_error();
        return Err(if error.kind() == std::io::ErrorKind::AlreadyExists {
            "The extraction folder already exists; choose another destination or rename it first.".into()
        } else { error.to_string() });
    }
    let output_root_fd = match openat_file(&destination_fd, &output_root_name, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0) {
        Ok(directory) => directory,
        Err(error) => {
            let _ = remove_entry_tree(&destination_fd, &output_root_name, 0);
            return Err(error.to_string());
        }
    };
    let result = (|| -> Result<(usize, u64), String> {
        let file = open_readonly_entry(&archive.parent, &archive.name).map_err(|error| error.to_string())?;
        let mut archive = ZipArchive::new(file)
            .map_err(|_| "The ZIP archive is invalid or encrypted.".to_owned())?;
        if archive.len() > 50_000 {
            return Err("An archive can contain at most 50,000 items.".into());
        }
        let mut total = 0u64;
        let mut extracted = 0usize;
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|_| "Could not read a ZIP entry.")?;
            if entry.is_symlink()
                || entry
                    .unix_mode()
                    .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err("ZIP archives containing symbolic links are not extracted.".into());
            }
            if entry.unix_mode().is_some_and(|mode| {
                let kind = mode & 0o170000;
                kind != 0 && kind != 0o100000 && kind != 0o040000
            }) {
                return Err(
                    "ZIP archives containing special filesystem objects are not extracted.".into(),
                );
            }
            let enclosed = entry
                .enclosed_name()
                .ok_or("The archive contains a path that escapes the destination.")?
                .to_owned();
            if enclosed
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
            {
                return Err("The archive contains an unsafe path.".into());
            }
            total = total.saturating_add(entry.size());
            if total > 2 * 1024 * 1024 * 1024 {
                return Err("Expanded ZIP content is limited to 2 GiB.".into());
            }
            let (parent, name) = ensure_zip_parent(&output_root_fd, &enclosed)?;
            if entry.is_dir() {
                open_or_create_directory(&parent, &name).map_err(|_| "A destination item conflicts with an archive directory.")?;
            } else {
                let expected_size = entry.size();
                ensure_copy_space(&parent, expected_size)?;
                let mut output = openat_file(&parent, &name, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0o644).map_err(|error| if error.kind()==std::io::ErrorKind::AlreadyExists { "An extracted file already exists; existing files are never overwritten.".into() } else { error.to_string() })?;
                let copied = match std::io::copy(&mut entry.take(expected_size), &mut output) {
                    Ok(copied) if copied == expected_size => copied,
                    Ok(_) => {
                        let _ = unlinkat_entry(&parent, &name, 0);
                        return Err("ZIP entry size did not match its directory record.".into());
                    }
                    Err(error) => {
                        let _ = unlinkat_entry(&parent, &name, 0);
                        return Err(error.to_string());
                    }
                };
                if copied != expected_size {
                    let _ = unlinkat_entry(&parent, &name, 0);
                    return Err("ZIP entry size did not match its directory record.".into());
                }
                output.sync_all().map_err(|error| error.to_string())?;
                extracted += 1;
            }
        }
        Ok((extracted, total))
    })();
    match result {
        Ok((files, bytes)) => {
            if unsafe { libc::fchmod(output_root_fd.as_raw_fd(), 0o755) } != 0 {
                let error = std::io::Error::last_os_error().to_string();
                let _ = remove_entry_tree(&destination_fd, &output_root_name, 0);
                return Err(error);
            }
            Ok(
                json!({"destination":output_root.to_string_lossy(),"files":files,"expanded_bytes":bytes}),
            )
        }
        Err(error) => {
            let _ = remove_entry_tree(&destination_fd, &output_root_name, 0);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        copy_tree, create_zip_archive, execute_action, extract_zip_archive,
        file_content_limit, file_content_within_limit, max_base64_encoded_bytes, remove_tree,
        safe_name, scan_disk_usage, try_disk_scan_permit, valid_mode, LargestScanEntries, BASE64,
        MAX_FILE_UPLOAD_BYTES,
        anchor_directory, openat_file, c_name, anchor_existing_entry, remove_entry_tree,
        copy_space_sufficient,
    };
    use base64::Engine;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::{fs, os::unix::fs::symlink, path::PathBuf};

    fn test_dir() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "arg-file-manager-test-{}-{}-{}",
            std::process::id(),
            super::now(),
            NEXT.fetch_add(1, Ordering::Relaxed)
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
    fn file_copy_preserves_the_free_space_reserve() {
        let reserve = super::FILE_OPERATION_FREE_SPACE_RESERVE_BYTES;
        assert!(copy_space_sufficient(reserve + 1024, 1024));
        assert!(!copy_space_sufficient(reserve + 1023, 1024));
        assert!(!copy_space_sufficient(u64::MAX, u64::MAX));
    }

    #[test]
    fn anchored_file_operation_keeps_the_open_directory_when_the_path_is_swapped() {
        let root = test_dir();
        let selected = root.join("selected");
        let moved = root.join("selected-original");
        let outside = root.join("outside");
        fs::create_dir(&selected).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(selected.join("editable.txt"), b"original").unwrap();
        fs::write(outside.join("editable.txt"), b"outside").unwrap();
        fs::write(outside.join("outside-marker"), b"outside").unwrap();
        let editable = anchor_existing_entry(selected.join("editable.txt").to_str().unwrap()).unwrap();
        let (_, directory) = anchor_directory(selected.to_str().unwrap()).unwrap();
        fs::rename(&selected, &moved).unwrap();
        symlink(&outside, &selected).unwrap();

        let filename = c_name(std::ffi::OsStr::new("created.txt")).unwrap();
        let mut file = openat_file(&directory, &filename, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0o600).unwrap();
        use std::io::Write as _;
        file.write_all(b"anchored").unwrap();
        super::save_file_atomically(&editable, b"saved through anchor").unwrap();

        assert_eq!(fs::read(moved.join("created.txt")).unwrap(), b"anchored");
        assert_eq!(fs::read(moved.join("editable.txt")).unwrap(), b"saved through anchor");
        assert_eq!(fs::read(outside.join("editable.txt")).unwrap(), b"outside");
        assert!(!outside.join("created.txt").exists());
        assert!(anchor_directory(selected.to_str().unwrap()).is_err(), "a path whose parent was replaced by a symlink must be rejected");
        let entry = anchor_existing_entry(selected.join("outside-marker").to_str().unwrap());
        assert!(entry.is_err(), "an untrusted symlink parent must not be traversed");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn anchored_recursive_delete_removes_a_symlink_without_touching_its_target() {
        let root = test_dir();
        let target = root.join("target");
        let tree = root.join("tree");
        fs::create_dir(&target).unwrap();
        fs::create_dir(&tree).unwrap();
        fs::write(target.join("keep.txt"), b"keep").unwrap();
        symlink(&target, tree.join("external")).unwrap();
        let entry = anchor_existing_entry(tree.join("external").to_str().unwrap()).unwrap();
        remove_entry_tree(&entry.parent, &entry.name, 0).unwrap();
        assert_eq!(fs::read(target.join("keep.txt")).unwrap(), b"keep");
        assert!(!tree.join("external").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_editor_replacement_does_not_follow_a_symlink_target() {
        let root = test_dir();
        let outside = root.join("outside.txt");
        let link = root.join("link.txt");
        fs::write(&outside, b"original").unwrap();
        symlink(&outside, &link).unwrap();
        let entry = anchor_existing_entry(link.to_str().unwrap()).unwrap();
        assert!(super::save_file_atomically(&entry, b"replaced").is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"original");
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn zip_creation_rejects_excessive_directory_depth_and_removes_partial_archive() {
        let root = test_dir();
        let tree = root.join("tree");
        fs::create_dir(&tree).unwrap();
        let mut current = tree.clone();
        for index in 0..65 {
            current.push(format!("d{index}"));
            fs::create_dir(&current).unwrap();
        }

        let result = create_zip_archive(&serde_json::json!({"path":tree,"name":"tree.zip"}));
        assert!(result.unwrap_err().contains("64 levels"));
        assert!(!root.join("tree.zip").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn upload_request_limit_accounts_for_base64_and_matches_decoded_file_limit() {
        let encoded_limit = max_base64_encoded_bytes(MAX_FILE_UPLOAD_BYTES);
        assert_eq!(encoded_limit, 2_796_204);
        assert_eq!(file_content_limit("upload"), Some(MAX_FILE_UPLOAD_BYTES));
        assert!(encoded_limit <= 3 * 1024 * 1024);
        assert!(file_content_within_limit("upload", encoded_limit));
        assert!(!file_content_within_limit("upload", encoded_limit + 1));

        let root = test_dir();
        let payload = vec![b'x'; MAX_FILE_UPLOAD_BYTES];
        let encoded = BASE64.encode(&payload);
        assert_eq!(encoded.len(), encoded_limit);
        execute_action(&serde_json::json!({
            "operation": "upload",
            "path": root,
            "name": "at-limit.bin",
            "content": encoded,
        }))
        .expect("2 MiB upload is accepted by the privileged layer");

        let oversized = BASE64.encode(vec![b'x'; MAX_FILE_UPLOAD_BYTES + 1]);
        let error = execute_action(&serde_json::json!({
            "operation": "upload",
            "path": root,
            "name": "over-limit.bin",
            "content": oversized,
        }))
        .expect_err("one byte over the raw limit must be rejected");
        assert!(error.contains("2 MiB"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn editor_request_limit_matches_its_one_mibibyte_file_limit() {
        let encoded_limit = max_base64_encoded_bytes(1024 * 1024);
        assert_eq!(encoded_limit, 1_398_104);
        assert_eq!(file_content_limit("save"), Some(1024 * 1024));
    }

    #[test]
    fn only_one_privileged_disk_scan_can_run_at_a_time() {
        let first = try_disk_scan_permit().expect("first scan should acquire the permit");
        assert!(try_disk_scan_permit().is_err());
        drop(first);
        assert!(try_disk_scan_permit().is_ok());
    }

    #[tokio::test]
    async fn disk_scan_requires_same_origin_before_admin_or_helper_work() {
        use super::{scan_disk, DiskScanInput};
        use axum::{
            extract::State,
            http::{header, HeaderMap, HeaderValue, StatusCode},
            Json,
        };
        use rusqlite::Connection;
        use std::sync::{Arc, Mutex};

        let state = State(Arc::new(Mutex::new(
            Connection::open_in_memory().expect("database"),
        )));
        let input = || {
            Json(DiskScanInput {
                path: "/invalid-for-test".into(),
                mode: "standard".into(),
            })
        };
        let missing_origin = scan_disk(state.clone(), HeaderMap::new(), input()).await;
        assert_eq!(
            missing_origin.expect_err("Origin is required").0,
            StatusCode::FORBIDDEN
        );

        let mut foreign_headers = HeaderMap::new();
        foreign_headers.insert(header::HOST, HeaderValue::from_static("panel.example.test"));
        foreign_headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://attacker.example"),
        );
        let foreign_origin = scan_disk(state.clone(), foreign_headers, input()).await;
        assert_eq!(
            foreign_origin.expect_err("foreign Origin is rejected").0,
            StatusCode::FORBIDDEN
        );

        let mut same_origin_headers = HeaderMap::new();
        same_origin_headers.insert(header::HOST, HeaderValue::from_static("panel.example.test"));
        same_origin_headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://panel.example.test"),
        );
        let unauthenticated = scan_disk(state, same_origin_headers, input()).await;
        assert_eq!(
            unauthenticated
                .expect_err("admin session is still required")
                .0,
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn symbolic_link_target_edit_replaces_the_link_without_touching_target_files() {
        let root = test_dir();
        let target_a = root.join("target-a");
        let target_b = root.join("target-b");
        let link = root.join("current");
        fs::write(&target_a, "A").unwrap();
        fs::write(&target_b, "B").unwrap();
        symlink("target-a", &link).unwrap();

        super::execute_action(&serde_json::json!({
            "operation": "symlink-update",
            "path": link,
            "link_target": "target-b"
        })).unwrap();

        assert_eq!(fs::read_link(&link).unwrap(), PathBuf::from("target-b"));
        assert_eq!(fs::read_to_string(target_a).unwrap(), "A");
        assert_eq!(fs::read_to_string(target_b).unwrap(), "B");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disk_scan_returns_direct_children_for_expandable_folder_navigation() {
        let root = test_dir();
        let alpha = root.join("alpha");
        let nested = alpha.join("nested");
        let beta = root.join("beta");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir(&beta).unwrap();
        fs::write(nested.join("payload.bin"), vec![7_u8; 16 * 1024]).unwrap();
        fs::write(beta.join("small.txt"), "small").unwrap();

        let report = scan_disk_usage(root.to_str().unwrap(), "standard").unwrap();
        let children = report["directory_children"].as_array().unwrap();
        let paths = children
            .iter()
            .filter_map(|item| item["path"].as_str())
            .collect::<Vec<_>>();
        assert!(paths.contains(&alpha.to_str().unwrap()));
        assert!(paths.contains(&beta.to_str().unwrap()));
        assert!(!paths.contains(&nested.to_str().unwrap()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disk_scan_keeps_only_bounded_top_entries_in_deterministic_order() {
        let mut entries = LargestScanEntries::new(3);
        for (path, bytes) in [
            ("/tmp/a", 10),
            ("/tmp/b", 20),
            ("/tmp/c", 30),
            ("/tmp/e", 30),
            ("/tmp/d", 15),
            ("/tmp/f", 5),
        ] {
            entries.insert(path.to_owned(), bytes);
        }

        let sorted = entries.into_sorted();
        assert_eq!(sorted.len(), 3);
        assert_eq!(
            sorted,
            vec![
                ("/tmp/c".to_owned(), 30),
                ("/tmp/e".to_owned(), 30),
                ("/tmp/b".to_owned(), 20),
            ]
        );
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
    fn zip_round_trip_works_and_zip_slip_is_rejected() {
        use zip::{write::SimpleFileOptions, ZipWriter};
        let root = test_dir();
        let source = root.join("site");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("index.html"), "hello ARG").unwrap();
        let created =
            create_zip_archive(&serde_json::json!({"path":source,"name":"backup.zip"})).unwrap();
        assert_eq!(created["entries"], 1);
        extract_zip_archive(&serde_json::json!({"path":root.join("backup.zip"),"destination":root,"confirmed":true})).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("backup/site/index.html")).unwrap(),
            "hello ARG"
        );

        let malicious = root.join("bad.zip");
        let mut writer = ZipWriter::new(fs::File::create(&malicious).unwrap());
        writer
            .start_file("../escaped.txt", SimpleFileOptions::default())
            .unwrap();
        use std::io::Write;
        writer.write_all(b"must not escape").unwrap();
        writer.finish().unwrap();
        assert!(extract_zip_archive(
            &serde_json::json!({"path":malicious,"destination":root,"confirmed":true})
        )
        .is_err());
        assert!(!root.parent().unwrap().join("escaped.txt").exists());
        assert!(!root.join("bad").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn zip_rejects_a_symlink_as_the_selected_source() {
        let root = test_dir();
        let target = root.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("private.txt"), "private").unwrap();
        let link = root.join("linked-site");
        symlink(&target, &link).unwrap();
        let result = create_zip_archive(&serde_json::json!({"path":link,"name":"archive.zip"}));
        assert!(result.is_err());
        assert!(!root.join("archive.zip").exists());
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
