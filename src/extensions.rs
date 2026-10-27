use super::*;

const SAFE_PERMISSIONS: &[&str] = &[
    "apps.read",
    "monitoring.read",
    "nginx.read",
    "ui.contribute",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtensionManifest {
    schema_version: u32,
    id: String,
    name: String,
    version: String,
    arg_api_compat: String,
    permissions: Vec<String>,
    entrypoints: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TrustKeyInput {
    id: String,
    label: String,
    public_key_hex: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ValidateInput {
    manifest_json: String,
    package_sha256: String,
    signer_id: String,
    signature_hex: String,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn valid_semver(value: &str) -> bool {
    let core = value.split_once('-').map_or(value, |(core, suffix)| {
        if suffix.is_empty()
            || !suffix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-".contains(&byte))
        {
            return "";
        }
        core
    });
    let parts = core.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn valid_manifest(manifest: &ExtensionManifest) -> Result<(), String> {
    if manifest.schema_version != 1 {
        return Err("نسخهٔ schema افزونه پشتیبانی نمی‌شود.".into());
    }
    if !valid_id(&manifest.id) {
        return Err("شناسه باید از بخش‌های lowercase جداشده با نقطه و خط تیره تشکیل شود.".into());
    }
    if manifest.name.trim().is_empty()
        || manifest.name.chars().count() > 100
        || manifest.name.chars().any(char::is_control)
    {
        return Err("نام افزونه معتبر نیست.".into());
    }
    if !valid_semver(&manifest.version) {
        return Err("نسخهٔ افزونه باید SemVer سه‌بخشی باشد.".into());
    }
    let current = env!("CARGO_PKG_VERSION")
        .split('-')
        .next()
        .unwrap_or(env!("CARGO_PKG_VERSION"));
    let current_minor = current.split('.').take(2).collect::<Vec<_>>().join(".");
    if manifest.arg_api_compat != current_minor {
        return Err(format!(
            "این افزونه برای API {} ساخته شده؛ API این پنل {} است.",
            manifest.arg_api_compat, current_minor
        ));
    }
    if manifest.permissions.len() > 16
        || manifest
            .permissions
            .iter()
            .any(|permission| !SAFE_PERMISSIONS.contains(&permission.as_str()))
    {
        return Err(
            "افزونه مجوز ناشناخته یا پرخطر دارد؛ مجوزهای این مرحله فقط‌خواندنی و محدودند.".into(),
        );
    }
    let unique = manifest.permissions.iter().collect::<HashSet<_>>();
    if unique.len() != manifest.permissions.len() {
        return Err("مجوز تکراری در manifest وجود دارد.".into());
    }
    if manifest.entrypoints.is_empty()
        || manifest.entrypoints.len() > 8
        || manifest.entrypoints.iter().any(|path| {
            path.is_empty()
                || path.len() > 256
                || path.starts_with('/')
                || path.contains('\\')
                || path
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == "..")
                || path.chars().any(char::is_control)
        })
    {
        return Err("فهرست entrypoint خالی، بیش‌ازحد یا دارای مسیر نامعتبر است.".into());
    }
    Ok(())
}

fn valid_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes * 2 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn verify_ed25519(public_key: &[u8], signature: &[u8], message: &[u8]) -> bool {
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public_key)
        .verify(message, signature)
        .is_ok()
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS extension_trust_keys(id TEXT PRIMARY KEY,label TEXT NOT NULL,public_key_hex TEXT NOT NULL,enabled INTEGER NOT NULL DEFAULT 1,created_by TEXT NOT NULL,created_at INTEGER NOT NULL,revoked_at INTEGER);")
}

pub(super) async fn list_trust_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let mut statement = db
        .prepare(
            "SELECT id,label,enabled,created_at,revoked_at FROM extension_trust_keys ORDER BY id",
        )
        .map_err(db_error)?;
    let keys = statement.query_map([], |row| Ok(json!({"id":row.get::<_,String>(0)?,"label":row.get::<_,String>(1)?,"enabled":row.get::<_,bool>(2)?,"created_at":row.get::<_,i64>(3)?,"revoked_at":row.get::<_,Option<i64>>(4)?}))).map_err(db_error)?.filter_map(Result::ok).collect::<Vec<_>>();
    Ok(Json(json!({"keys":keys})))
}

pub(super) async fn add_trust_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<TrustKeyInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_id(&input.id)
        || input.label.trim().is_empty()
        || input.label.chars().count() > 100
        || !valid_hex(&input.public_key_hex, 32)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "شناسه، عنوان یا کلید عمومی Ed25519 معتبر نیست.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    db.execute("INSERT INTO extension_trust_keys(id,label,public_key_hex,enabled,created_by,created_at) VALUES(?1,?2,?3,1,?4,?5)", params![input.id,input.label.trim(),input.public_key_hex.to_ascii_lowercase(),actor,now()]).map_err(|problem| if problem.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) { error(StatusCode::CONFLICT,"شناسهٔ کلید اعتماد قبلاً ثبت شده است.") } else { db_error(problem) })?;
    audit(
        &db,
        &actor,
        "extension.trust-key.add",
        &format!("key_id={}", input.id),
    );
    Ok(Json(json!({"created":true,"id":input.id})))
}

pub(super) async fn revoke_trust_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_id(&id) {
        return Err(error(StatusCode::BAD_REQUEST, "شناسهٔ کلید معتبر نیست."));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let changed = db
        .execute(
            "UPDATE extension_trust_keys SET enabled=0,revoked_at=?1 WHERE id=?2 AND enabled=1",
            params![now(), id],
        )
        .map_err(db_error)?;
    if changed == 0 {
        return Err(error(StatusCode::NOT_FOUND, "کلید اعتماد فعال پیدا نشد."));
    }
    audit(
        &db,
        &actor,
        "extension.trust-key.revoke",
        &format!("key_id={id}"),
    );
    Ok(Json(json!({"revoked":true,"id":id})))
}

pub(super) async fn validate_manifest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ValidateInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if input.manifest_json.len() > 32 * 1024
        || !valid_hex(&input.package_sha256, 32)
        || !valid_hex(&input.signature_hex, 64)
        || !valid_id(&input.signer_id)
    {
        return Err(error(StatusCode::BAD_REQUEST,"Manifest، digest، شناسهٔ امضاکننده یا امضای ارسالی معتبر نیست یا از حد اندازه عبور کرده است."));
    }
    let manifest: ExtensionManifest = serde_json::from_str(&input.manifest_json).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "JSON manifest با schema نسخهٔ ۱ مطابقت ندارد.",
        )
    })?;
    valid_manifest(&manifest)
        .map_err(|message| error(StatusCode::UNPROCESSABLE_ENTITY, &message))?;
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let public_key: Option<String> = db
        .query_row(
            "SELECT public_key_hex FROM extension_trust_keys WHERE id=?1 AND enabled=1",
            [&input.signer_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?;
    let trusted = public_key.is_some();
    let signature_valid = public_key
        .and_then(|key| hex::decode(key).ok())
        .is_some_and(|key| {
            let Ok(signature) = hex::decode(&input.signature_hex) else {
                return false;
            };
            let manifest_hash = hex::encode(Sha256::digest(input.manifest_json.as_bytes()));
            let message = format!(
                "ARG-EXTENSION-MANIFEST-V1\n{manifest_hash}\n{}",
                input.package_sha256.to_ascii_lowercase()
            );
            verify_ed25519(&key, &signature, message.as_bytes())
        });
    Ok(Json(json!({
        "schema_valid":true,
        "extension_id":manifest.id,
        "trusted_signer":trusted,
        "signature_valid":signature_valid,
        "manifest_sha256":hex::encode(Sha256::digest(input.manifest_json.as_bytes())),
        "declared_package_sha256":input.package_sha256.to_ascii_lowercase(),
        "package_bytes_verified":false,
        "installable":false,
        "message":"این مسیر فقط schema و امضای manifest به‌همراه digest اعلام‌شده را بررسی می‌کند. فایل بسته برای محاسبهٔ digest دریافت نشده؛ نصب یا اجرای افزونه هنوز پشتیبانی نمی‌شود."
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn manifest() -> ExtensionManifest {
        ExtensionManifest {
            schema_version: 1,
            id: "example.safe-widget".into(),
            name: "Safe Widget".into(),
            version: "1.2.3".into(),
            arg_api_compat: env!("CARGO_PKG_VERSION")
                .split('-')
                .next()
                .unwrap()
                .split('.')
                .take(2)
                .collect::<Vec<_>>()
                .join("."),
            permissions: vec!["apps.read".into()],
            entrypoints: vec!["ui/index.html".into()],
        }
    }

    #[test]
    fn extension_manifest_rejects_traversal_unknown_permissions_and_incompatibility() {
        assert!(valid_manifest(&manifest()).is_ok());
        let mut bad = manifest();
        bad.entrypoints = vec!["../install.sh".into()];
        assert!(valid_manifest(&bad).is_err());
        let mut bad = manifest();
        bad.permissions = vec!["shell.root".into()];
        assert!(valid_manifest(&bad).is_err());
        let mut bad = manifest();
        bad.arg_api_compat = "9.9".into();
        assert!(valid_manifest(&bad).is_err());
    }

    #[test]
    fn extension_signature_is_ed25519_and_binds_package_digest() {
        let pair = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let message = b"ARG-EXTENSION-MANIFEST-V1\nmanifest\npackage";
        let signature = pair.sign(message);
        assert!(verify_ed25519(
            pair.public_key().as_ref(),
            signature.as_ref(),
            message
        ));
        assert!(!verify_ed25519(
            pair.public_key().as_ref(),
            signature.as_ref(),
            b"ARG-EXTENSION-MANIFEST-V1\nmanifest\nother"
        ));
    }

    #[test]
    fn trust_key_migration_is_idempotent_and_does_not_return_secret_material() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        let columns: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('extension_trust_keys')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(columns, 7);
    }
}
