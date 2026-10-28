use super::*;
use std::{
    fs,
    path::{Component, Path as FsPath},
};

const MAX_ENTRIES: usize = 500;
const MAX_RAW_SCAN_ENTRIES: usize = 10_000;
const MAX_TEXT_BYTES: usize = 512 * 1024;

struct TreeEntryInfo {
    name: String,
    is_directory: bool,
    is_file: bool,
    size_bytes: u64,
}

fn visible_tree_rows(
    entries: impl IntoIterator<Item = TreeEntryInfo>,
    prefix: &str,
    raw_scan_truncated: bool,
) -> (Vec<Value>, bool) {
    let mut rows = Vec::new();
    let mut visible_truncated = false;
    for entry in entries {
        if hidden_or_sensitive(&entry.name) || (!entry.is_directory && !entry.is_file) {
            continue;
        }
        if rows.len() >= MAX_ENTRIES {
            visible_truncated = true;
            continue;
        }
        let relative = if prefix.is_empty() {
            entry.name.clone()
        } else {
            format!("{prefix}/{}", entry.name)
        };
        rows.push(json!({
            "name": entry.name,
            "path": relative,
            "kind": if entry.is_directory { "directory" } else { "file" },
            "size_bytes": if entry.is_file { entry.size_bytes } else { 0 }
        }));
    }
    rows.sort_by(|left, right| {
        left["kind"]
            .as_str()
            .cmp(&right["kind"].as_str())
            .then_with(|| left["name"].as_str().cmp(&right["name"].as_str()))
    });
    (rows, visible_truncated || raw_scan_truncated)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CodePath {
    #[serde(default)]
    path: String,
}

fn safe_relative(value: &str) -> bool {
    value.len() <= 1024
        && !value.contains(['\\', '\0'])
        && (value.is_empty()
            || FsPath::new(value)
                .components()
                .all(|part| matches!(part, Component::Normal(_))))
}

fn hidden_or_sensitive(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name.to_ascii_lowercase().as_str(),
            "vendor" | "node_modules" | "storage" | "secrets"
        )
        || name.to_ascii_lowercase().starts_with(".env")
        || matches!(
            name.to_ascii_lowercase().as_str(),
            "id_rsa" | "id_ed25519" | "authorized_keys"
        )
}

fn active_release(
    db: &Connection,
    app_id: &str,
    owner: &str,
) -> rusqlite::Result<Option<(String, String, Option<String>)>> {
    db.query_row(
        "SELECT r.path,r.id,r.source_commit FROM deployment_releases r JOIN applications a ON a.id=r.application_id WHERE r.application_id=?1 AND a.owner_server_uuid=?2 AND a.deleted_at IS NULL AND r.state='active' ORDER BY r.activated_at DESC LIMIT 1",
        params![app_id, owner],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()
}

fn release_root(app_id: &str, release_path: &str) -> Result<FsPathBuf, (StatusCode, Json<Value>)> {
    let root = FsPath::new(release_path)
        .canonicalize()
        .map_err(|_| error(StatusCode::NOT_FOUND, "release فعال روی دیسک پیدا نشد."))?;
    let app_root = deploy::deployment_root()
        .join("apps")
        .join(app_id)
        .canonicalize()
        .map_err(|_| error(StatusCode::NOT_FOUND, "پوشهٔ مالکیت اپ پیدا نشد."))?;
    if !root.starts_with(&app_root) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "release خارج از محدودهٔ این اپ است.",
        ));
    }
    Ok(root)
}

type FsPathBuf = std::path::PathBuf;

fn resolve_child(root: &FsPath, relative: &str) -> Result<FsPathBuf, (StatusCode, Json<Value>)> {
    if !safe_relative(relative) {
        return Err(error(StatusCode::BAD_REQUEST, "مسیر فایل نامعتبر است."));
    }
    if relative.split('/').any(hidden_or_sensitive) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "نمایش فایل‌های مخفی یا محرمانه از این مرورگر مجاز نیست.",
        ));
    }
    let path = if relative.is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative)
    };
    let canonical = path
        .canonicalize()
        .map_err(|_| error(StatusCode::NOT_FOUND, "مسیر در release پیدا نشد."))?;
    if !canonical.starts_with(root) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "مسیر انتخاب‌شده از release خارج می‌شود.",
        ));
    }
    Ok(canonical)
}

fn app_release(
    state: &AppState,
    headers: &HeaderMap,
    app_id: &str,
) -> Result<(FsPathBuf, String, Option<String>), (StatusCode, Json<Value>)> {
    let db = state.lock().unwrap();
    require_admin(&db, headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let Some((path, release_id, commit)) = active_release(&db, app_id, &owner).map_err(db_error)?
    else {
        return Err(error(
            StatusCode::NOT_FOUND,
            "برای این اپ release فعال پیدا نشد.",
        ));
    };
    Ok((release_root(app_id, &path)?, release_id, commit))
}

pub(super) async fn tree(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Query(query): Query<CodePath>,
) -> ApiResult {
    let (root, release_id, commit) = app_release(&state, &headers, &app_id)?;
    let directory = resolve_child(&root, &query.path)?;
    if !directory.is_dir() {
        return Err(error(StatusCode::BAD_REQUEST, "مسیر انتخاب‌شده پوشه نیست."));
    }
    let entries = fs::read_dir(&directory)
        .map_err(|_| error(StatusCode::FORBIDDEN, "فهرست فایل‌های این مسیر خوانده نشد."))?;
    let mut candidates = Vec::new();
    let mut raw_scan_truncated = false;
    for (index, entry) in entries.take(MAX_RAW_SCAN_ENTRIES + 1).enumerate() {
        if index >= MAX_RAW_SCAN_ENTRIES {
            raw_scan_truncated = true;
            break;
        }
        let entry = entry.map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "خواندن یکی از ورودی‌های مسیر ناموفق بود.",
            )
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if hidden_or_sensitive(&name) {
            continue;
        }
        let metadata = entry.metadata().map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "metadata یکی از فایل‌ها خوانده نشد.",
            )
        })?;
        if !metadata.is_dir() && !metadata.is_file() {
            continue;
        }
        candidates.push(TreeEntryInfo {
            name,
            is_directory: metadata.is_dir(),
            is_file: metadata.is_file(),
            size_bytes: metadata.len(),
        });
    }
    let (rows, truncated) = visible_tree_rows(candidates, &query.path, raw_scan_truncated);
    Ok(Json(
        json!({"release_id":release_id,"source_commit":commit,"path":query.path,"entries":rows,"truncated":truncated,"read_only":true}),
    ))
}

pub(super) async fn file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Query(query): Query<CodePath>,
) -> ApiResult {
    let (root, release_id, commit) = app_release(&state, &headers, &app_id)?;
    let path = resolve_child(&root, &query.path)?;
    let metadata =
        fs::metadata(&path).map_err(|_| error(StatusCode::NOT_FOUND, "فایل پیدا نشد."))?;
    if !metadata.is_file() {
        return Err(error(StatusCode::BAD_REQUEST, "مسیر انتخاب‌شده فایل نیست."));
    }
    if metadata.len() as usize > MAX_TEXT_BYTES {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "فایل برای پیش‌نمایش بیش از ۵۱۲ KiB است.",
        ));
    }
    let bytes =
        fs::read(path).map_err(|_| error(StatusCode::FORBIDDEN, "خواندن فایل ناموفق بود."))?;
    let content = String::from_utf8(bytes).map_err(|_| {
        error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "این فایل متنی UTF-8 نیست و در مرورگر نمایش داده نمی‌شود.",
        )
    })?;
    Ok(Json(
        json!({"release_id":release_id,"source_commit":commit,"path":query.path,"content":content,"read_only":true}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_listing_filters_hidden_entries_before_visible_limit_and_sorts() {
        let hidden = (0..700)
            .map(|index| TreeEntryInfo {
                name: format!(".hidden-{index:04}"),
                is_directory: false,
                is_file: true,
                size_bytes: 1,
            })
            .collect::<Vec<_>>();
        let visible = vec![
            TreeEntryInfo {
                name: "z-last.txt".into(),
                is_directory: false,
                is_file: true,
                size_bytes: 2,
            },
            TreeEntryInfo {
                name: "a-first.txt".into(),
                is_directory: false,
                is_file: true,
                size_bytes: 3,
            },
        ];
        let (rows, truncated) = visible_tree_rows(hidden.into_iter().chain(visible), "src", false);
        let names = rows
            .iter()
            .filter_map(|row| row["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["a-first.txt", "z-last.txt"]);
        assert_eq!(rows[0]["path"], "src/a-first.txt");
        assert!(!truncated);
    }

    #[test]
    fn tree_listing_caps_visible_entries_and_reports_both_truncation_causes() {
        let visible = (0..=MAX_ENTRIES)
            .map(|index| TreeEntryInfo {
                name: format!("file-{index:04}.txt"),
                is_directory: false,
                is_file: true,
                size_bytes: index as u64,
            })
            .collect::<Vec<_>>();
        let (rows, visible_truncated) = visible_tree_rows(visible, "", false);
        assert_eq!(rows.len(), MAX_ENTRIES);
        assert!(visible_truncated);

        let (rows, raw_truncated) = visible_tree_rows(Vec::new(), "", true);
        assert!(rows.is_empty());
        assert!(raw_truncated);
    }

    #[test]
    fn rejects_traversal_and_hidden_files() {
        assert!(!safe_relative("../etc/passwd"));
        assert!(!safe_relative("a\\b"));
        assert!(hidden_or_sensitive(".env.production"));
        assert!(hidden_or_sensitive("id_rsa"));
        assert!(!hidden_or_sensitive("src/main.rs"));
    }
    #[test]
    fn canonical_paths_must_stay_under_release() {
        let root = std::env::temp_dir().join(format!("arg-code-{}", rand::random::<u64>()));
        let outside = std::env::temp_dir().join(format!("arg-code-out-{}", rand::random::<u64>()));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("leak.txt"), "secret").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.join("leak.txt"), root.join("link.txt")).unwrap();
        #[cfg(unix)]
        assert!(resolve_child(&root, "link.txt").is_err());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }
}
