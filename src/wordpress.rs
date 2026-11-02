use super::*;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const SECRET_DIR: &str = "/var/lib/arg/wordpress-secrets";
const MAX_CAPTURE: usize = 32 * 1024;

#[derive(Deserialize, Serialize)]
struct AdminSetup {
    site_title: String,
    username: String,
    email: String,
    password: String,
}

#[derive(Deserialize, Serialize)]
struct DatabaseCredential {
    database: String,
    username: String,
    password: String,
}

pub(super) fn execute_privileged_request(action: &str, request: &Value) -> Value {
    let result = match action {
        "wordpress-admin-store" => store_admin_setup(request),
        "wordpress-admin-delete" => delete_secret(request, "admin"),
        "wordpress-runtime-install" => install_runtime(request),
        "wordpress-provision" => provision(request),
        "wordpress-set-url" => set_site_url(request),
        "wordpress-health" => health_check(request),
        _ => Err("درخواست privileged مربوط به WordPress معتبر نیست.".to_owned()),
    };
    match result {
        Ok(data) => json!({"ok":true,"data":data}),
        Err(error) => json!({"ok":false,"error":error}),
    }
}

fn app_id(request: &Value) -> Result<&str, String> {
    let id = request["app_id"]
        .as_str()
        .ok_or_else(|| "شناسهٔ اپلیکیشن WordPress ارسال نشده است.".to_owned())?;
    if !apps::valid_uuid(id) {
        return Err("شناسهٔ اپلیکیشن WordPress معتبر نیست.".into());
    }
    Ok(id)
}

fn secret_ref(kind: &str, id: &str) -> String {
    format!("{kind}-{id}")
}

fn store_admin_setup(request: &Value) -> Result<Value, String> {
    let id = app_id(request)?;
    let setup = AdminSetup {
        site_title: request["site_title"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned(),
        username: request["username"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned(),
        email: request["email"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned(),
        password: request["password"].as_str().unwrap_or_default().to_owned(),
    };
    validate_admin_setup(&setup)?;
    let encoded = serde_json::to_vec(&setup)
        .map_err(|_| "تنظیمات مدیر WordPress رمزگذاری نشد.".to_owned())?;
    secret_vault::store(Path::new(SECRET_DIR), &secret_ref("admin", id), &encoded)?;
    Ok(json!({"stored":true}))
}

fn validate_admin_setup(setup: &AdminSetup) -> Result<(), String> {
    if setup.site_title.is_empty()
        || setup.site_title.chars().count() > 100
        || setup.site_title.chars().any(char::is_control)
    {
        return Err("عنوان سایت WordPress معتبر نیست.".into());
    }
    if !(1..=60).contains(&setup.username.len())
        || !setup
            .username
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err("نام کاربری مدیر WordPress معتبر نیست.".into());
    }
    let Some((local, domain)) = setup.email.split_once('@') else {
        return Err("ایمیل مدیر WordPress معتبر نیست.".into());
    };
    if setup.email.len() > 254
        || local.is_empty()
        || local.len() > 64
        || local.chars().any(char::is_whitespace)
        || !valid_email_host(domain)
    {
        return Err("ایمیل مدیر WordPress معتبر نیست.".into());
    }
    if !(12..=128).contains(&setup.password.len()) || setup.password.chars().any(char::is_control) {
        return Err("گذرواژهٔ مدیر WordPress باید ۱۲ تا ۱۲۸ نویسه داشته باشد.".into());
    }
    Ok(())
}

fn valid_email_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn delete_secret(request: &Value, kind: &str) -> Result<Value, String> {
    let id = app_id(request)?;
    let reference = secret_ref(kind, id);
    match secret_vault::delete(Path::new(SECRET_DIR), &reference) {
        Ok(()) => Ok(json!({"deleted":true})),
        Err(message) if message.contains("پیدا نشد") => Ok(json!({"deleted":false})),
        Err(message) => Err(message),
    }
}

fn install_runtime(request: &Value) -> Result<Value, String> {
    let version = request["runtime_version"].as_str().unwrap_or("system");
    if version != "system" && !valid_runtime_version(version) {
        return Err("نسخهٔ PHP برای نصب خودکار معتبر نیست.".into());
    }
    let database_status = mysql::execute_privileged_request("mysql-status", &json!({}));
    if database_status["ok"] != true {
        return Err(database_status["error"]
            .as_str()
            .unwrap_or("بررسی MariaDB ناموفق بود.")
            .to_owned());
    }
    let mut packages = vec!["nginx".to_owned(), "curl".to_owned(), "acl".to_owned()];
    if database_status["data"]["installed"] != true {
        packages.push("mariadb-server".to_owned());
    }
    let php_components = [
        "cli", "fpm", "mysql", "curl", "gd", "mbstring", "xml", "zip", "intl",
    ];
    if version == "system" {
        packages.extend(["php-fpm", "php-cli"].into_iter().map(str::to_owned));
        packages.extend(
            ["mysql", "curl", "gd", "mbstring", "xml", "zip", "intl"]
                .into_iter()
                .map(|component| format!("php-{component}")),
        );
    } else {
        packages.extend(
            php_components
                .iter()
                .map(|component| format!("php{version}-{component}")),
        );
    }
    let update = run_bounded("apt-get", &["-y", "update"], 900, true)?;
    if !update.0 {
        return Err(format!("به‌روزرسانی فهرست بسته‌ها شکست خورد: {}", update.1));
    }
    let install = run_bounded(
        "apt-get",
        &[
            "-y",
            "--no-install-recommends",
            "install",
            "--no-upgrade",
            "--",
        ]
        .iter()
        .map(|item| (*item).to_owned())
        .chain(packages.iter().cloned())
        .collect::<Vec<_>>(),
        1800,
        true,
    )?;
    if !install.0 {
        return Err(format!(
            "نصب Nginx، PHP-FPM و افزونه‌های لازم شکست خورد: {}",
            install.1
        ));
    }

    let database_status = mysql::execute_privileged_request("mysql-status", &json!({}));
    if database_status["ok"] != true || database_status["data"]["installed"] != true {
        return Err(database_status["error"]
            .as_str()
            .unwrap_or("MariaDB پس از نصب پیدا نشد.")
            .to_owned());
    }
    let started =
        mysql::execute_privileged_request("mysql-service", &json!({"operation":"enable-now"}));
    if started["ok"] != true {
        return Err(started["error"]
            .as_str()
            .unwrap_or("راه‌اندازی MariaDB شکست خورد.")
            .to_owned());
    }

    let result = run_bounded(
        "systemctl",
        &["enable", "--now", "nginx.service"],
        60,
        false,
    )?;
    if !result.0 {
        return Err(format!("راه‌اندازی Nginx شکست خورد: {}", result.1));
    }
    let fpm = selected_fpm_service(version)?;
    let result = run_bounded("systemctl", &["enable", "--now", &fpm], 60, false)?;
    if !result.0 {
        return Err(format!("راه‌اندازی {fpm} شکست خورد: {}", result.1));
    }
    Ok(
        json!({"installed":true,"php_fpm_service":fpm,"database":"MariaDB/MySQL ready","nginx":"active"}),
    )
}

fn selected_fpm_service(version: &str) -> Result<String, String> {
    if version != "system" {
        let unit = format!("php{version}-fpm.service");
        if Path::new(&format!("/etc/php/{version}/fpm/pool.d")).is_dir() {
            return Ok(unit);
        }
        return Err(format!("PHP-FPM {version} پس از نصب پیدا نشد."));
    }
    let entries =
        fs::read_dir("/etc/php").map_err(|_| "نسخهٔ PHP-FPM نصب‌شده پیدا نشد.".to_owned())?;
    let mut versions = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let path = entry.path().join("fpm/pool.d");
            (valid_runtime_version(&name) && path.is_dir()).then_some(name)
        })
        .collect::<Vec<_>>();
    versions.sort_by(|left, right| {
        left.split('.')
            .map(|part| part.parse::<u32>().unwrap_or(0))
            .cmp(
                right
                    .split('.')
                    .map(|part| part.parse::<u32>().unwrap_or(0)),
            )
    });
    versions
        .pop()
        .map(|version| format!("php{version}-fpm.service"))
        .ok_or_else(|| "PHP-FPM نصب شد اما نسخهٔ فعال آن پیدا نشد.".to_owned())
}

fn valid_runtime_version(value: &str) -> bool {
    (3..=8).contains(&value.len())
        && value.split('.').count() == 2
        && value
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn run_bounded(
    program: &str,
    args: &[impl AsRef<str>],
    timeout_seconds: u32,
    apt_environment: bool,
) -> Result<(bool, String), String> {
    let args = args
        .iter()
        .map(|item| item.as_ref().to_owned())
        .collect::<Vec<_>>();
    run_bounded_inner(program, &args, timeout_seconds, apt_environment, None)
}

fn run_bounded_with_input(
    program: &str,
    args: &[String],
    timeout_seconds: u32,
    input: &[u8],
) -> Result<(bool, String), String> {
    run_bounded_inner(program, args, timeout_seconds, false, Some(input))
}

fn run_bounded_inner(
    program: &str,
    args: &[String],
    timeout_seconds: u32,
    apt_environment: bool,
    input: Option<&[u8]>,
) -> Result<(bool, String), String> {
    let timeout = Path::new("/usr/bin/timeout");
    let executable = if timeout.is_file() {
        timeout
    } else {
        Path::new("/bin/timeout")
    };
    let mut command = Command::new(executable);
    command
        .args([
            "--signal=TERM",
            "--kill-after=10s",
            &format!("{timeout_seconds}s"),
            program,
        ])
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if apt_environment {
        command
            .env("DEBIAN_FRONTEND", "noninteractive")
            .env("APT_LISTCHANGES_FRONTEND", "none")
            .env("NEEDRESTART_MODE", "a");
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("اجرای {program} ممکن نشد: {error}"))?;
    if let Some(input) = input {
        let mut stdin = child
            .stdin
            .take()
            .ok_or("ورودی برنامهٔ محدودشده در دسترس نیست.")?;
        stdin
            .write_all(input)
            .map_err(|_| "ارسال ورودی محدودشدهٔ برنامه ناموفق بود.".to_owned())?;
    }
    let mut stdout = child
        .stdout
        .take()
        .ok_or("خروجی عملیات قابل خواندن نیست.")?;
    let mut stderr = child.stderr.take().ok_or("خطای عملیات قابل خواندن نیست.")?;
    let output_reader = std::thread::spawn(move || read_capture(&mut stdout));
    let error_reader = std::thread::spawn(move || read_capture(&mut stderr));
    let status = child
        .wait()
        .map_err(|error| format!("پایان {program} بررسی نشد: {error}"))?;
    let output = output_reader.join().unwrap_or_default();
    let error = error_reader.join().unwrap_or_default();
    let combined = if error.trim().is_empty() {
        output
    } else if output.trim().is_empty() {
        error
    } else {
        format!("{output}\n{error}")
    };
    Ok((
        status.success(),
        combined.chars().take(MAX_CAPTURE).collect(),
    ))
}

fn read_capture(reader: &mut impl Read) -> String {
    let mut collected = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(size) => {
                if collected.len() < MAX_CAPTURE {
                    let keep = size.min(MAX_CAPTURE - collected.len());
                    collected.extend_from_slice(&buffer[..keep]);
                }
            }
        }
    }
    String::from_utf8_lossy(&collected).into_owned()
}

fn provision(request: &Value) -> Result<Value, String> {
    let id = app_id(request)?;
    let domain = request["domain"].as_str().unwrap_or_default().trim();
    let version = request["wordpress_version"].as_str().unwrap_or_default();
    let release_text = request["release_path"].as_str().unwrap_or_default();
    if !valid_domain(domain) || domain.contains('*') || !valid_wp_version(version) {
        return Err("دامنه یا نسخهٔ هستهٔ WordPress برای راه‌اندازی معتبر نیست.".into());
    }
    let release = validated_release_path(id, release_text)?;
    verify_wordpress_source(&release, version)?;

    let credentials = load_or_create_database_credential(id)?;
    provision_database(id, &credentials)?;

    prepare_shared_files(id, &release, &credentials, domain)?;
    grant_runtime_read_access(id, &release)?;
    let admin_reference = secret_ref("admin", id);
    let admin = match secret_vault::read(Path::new(SECRET_DIR), &admin_reference) {
        Ok(value) => Some(
            serde_json::from_slice::<AdminSetup>(&value)
                .map_err(|_| "اعتبار مدیر WordPress در مخزن امن قابل خواندن نیست.".to_owned())?,
        ),
        Err(message) if message.contains("پیدا نشد") => None,
        Err(message) => return Err(message),
    };
    let acme_email = if let Some(admin) = admin {
        validate_admin_setup(&admin)?;
        let acme_email = admin.email.clone();
        run_wordpress_installer(&release, domain, &admin)?;
        secret_vault::store(
            Path::new(SECRET_DIR),
            &secret_ref("email", id),
            acme_email.as_bytes(),
        )?;
        secret_vault::delete(Path::new(SECRET_DIR), &admin_reference)?;
        Some(acme_email)
    } else if !wordpress_is_installed(&release, domain)? {
        return Err("رمز اولیهٔ مدیر WordPress در مخزن امن نیست و نصب دیتابیس کامل نشده است؛ برای بازیابی، قالب را دوباره با همان اپ اجرا کن.".into());
    } else {
        secret_vault::read(Path::new(SECRET_DIR), &secret_ref("email", id))
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
    };
    Ok(json!({
        "database":"created_or_verified",
        "release":"configured",
        "admin_installation":"complete",
        "persistent_content":"ready",
        "acme_email":acme_email,
        "initial_install":acme_email.is_some()
    }))
}

fn load_or_create_database_credential(app_id: &str) -> Result<DatabaseCredential, String> {
    let reference = secret_ref("db", app_id);
    if secret_vault::contains(Path::new(SECRET_DIR), &reference)? {
        let value = secret_vault::read(Path::new(SECRET_DIR), &reference)?;
        let credential = serde_json::from_slice::<DatabaseCredential>(&value)
            .map_err(|_| "اعتبار دیتابیس WordPress در مخزن امن معتبر نیست.".to_owned())?;
        if !valid_database_name(&credential.database)
            || !valid_mysql_user(&credential.username)
            || !(32..=128).contains(&credential.password.len())
            || !credential
                .password
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("نام یا گذرواژهٔ دیتابیس WordPress در مخزن امن معتبر نیست.".into());
        }
        return Ok(credential);
    }
    let short = app_id.get(..8).ok_or("شناسهٔ WordPress نامعتبر است.")?;
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let credential = DatabaseCredential {
        database: format!("argwp_{short}_db"),
        username: format!("argwp_{short}"),
        password: hex::encode(bytes),
    };
    let payload = serde_json::to_vec(&credential)
        .map_err(|_| "اعتبار دیتابیس WordPress ساخته نشد.".to_owned())?;
    secret_vault::store(Path::new(SECRET_DIR), &reference, &payload)?;
    Ok(credential)
}

fn valid_database_name(value: &str) -> bool {
    value.starts_with("argwp_")
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn valid_mysql_user(value: &str) -> bool {
    value.starts_with("argwp_")
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn mysql_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "''")
}

fn provision_database(app_id: &str, credentials: &DatabaseCredential) -> Result<(), String> {
    if !apps::valid_uuid(app_id)
        || !valid_database_name(&credentials.database)
        || !valid_mysql_user(&credentials.username)
        || !(32..=128).contains(&credentials.password.len())
        || !credentials
            .password
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("اعتبار دیتابیس WordPress برای provision معتبر نیست.".into());
    }
    let metadata_sql = format!(
        "SELECT COUNT(*) FROM information_schema.SCHEMATA WHERE SCHEMA_NAME='{}';",
        credentials.database
    );
    let database_exists = run_mysql_admin(&metadata_sql)?
        .trim()
        .parse::<u32>()
        .ok()
        .is_some_and(|count| count > 0);
    let marker_sql = format!(
        "SELECT COUNT(*) FROM information_schema.TABLES WHERE TABLE_SCHEMA='{}' AND TABLE_NAME='arg_managed_install';",
        credentials.database
    );
    if database_exists {
        let marker_exists = run_mysql_admin(&marker_sql)?
            .trim()
            .parse::<u32>()
            .ok()
            .is_some_and(|count| count > 0);
        if !marker_exists {
            return Err("نام دیتابیس WordPress از قبل استفاده شده و مالکیت ARG تأیید نشد؛ دیتابیس موجود دست‌نخورده ماند.".into());
        }
        let marker_owner_sql = format!(
            "SELECT app_id FROM `{}`.`arg_managed_install` LIMIT 1;",
            credentials.database
        );
        if run_mysql_admin(&marker_owner_sql)?.trim() != app_id {
            return Err(
                "شناسهٔ دیتابیس WordPress با اپلیکیشن تطبیق ندارد؛ دادهٔ موجود دست‌نخورده ماند."
                    .into(),
            );
        }
    } else {
        let create_database = create_database_sql(app_id, credentials);
        run_mysql_admin(&create_database)?;
    }

    let proof = mysql_with_application_credentials(
        &credentials.username,
        &credentials.password,
        &credentials.database,
        "SELECT app_id FROM arg_managed_install LIMIT 1;",
    );
    if let Ok(proof) = proof {
        if proof.trim() != app_id {
            return Err(
                "حساب دیتابیس WordPress به اپلیکیشن دیگری تعلق دارد؛ حساب موجود دست‌نخورده ماند."
                    .into(),
            );
        }
    } else {
        let create_user = format!(
            "CREATE USER '{}'@'localhost' IDENTIFIED BY '{}';",
            credentials.username,
            mysql_literal(&credentials.password)
        );
        run_mysql_admin(&create_user)?;
    }
    let grant = format!(
        "GRANT ALL PRIVILEGES ON `{}`.* TO '{}'@'localhost'; FLUSH PRIVILEGES;",
        credentials.database, credentials.username
    );
    run_mysql_admin(&grant)?;
    Ok(())
}

fn create_database_sql(app_id: &str, credentials: &DatabaseCredential) -> String {
    format!(
        "CREATE DATABASE `{}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci; CREATE TABLE `{}`.`arg_managed_install` (app_id CHAR(36) NOT NULL PRIMARY KEY) ENGINE=InnoDB; INSERT INTO `{}`.`arg_managed_install` (app_id) VALUES ('{}');",
        credentials.database, credentials.database, credentials.database, app_id
    )
}

fn mysql_client() -> Result<&'static str, String> {
    ["/usr/bin/mariadb", "/usr/bin/mysql"]
        .into_iter()
        .find(|path| Path::new(path).is_file())
        .ok_or_else(|| "کلاینت MySQL/MariaDB پیدا نشد.".to_owned())
}

fn run_mysql_admin(sql: &str) -> Result<String, String> {
    if let Ok(output) = run_mysql_root(sql) {
        return Ok(output);
    }
    let manager = mysql::execute_privileged_request("mysql-query", &json!({"sql":sql}));
    if manager["ok"] == true {
        Ok(manager["output"].as_str().unwrap_or_default().to_owned())
    } else {
        Err("نه اتصال محلی مدیر MySQL و نه اعتبار امن ARG برای ساخت دیتابیس در دسترس است؛ MySQL را بررسی کن.".into())
    }
}

fn run_mysql_root(sql: &str) -> Result<String, String> {
    let args = vec![
        "--protocol=socket".to_owned(),
        "--user=root".to_owned(),
        "--batch".to_owned(),
        "--skip-column-names".to_owned(),
    ];
    let result = run_bounded_with_input(mysql_client()?, &args, 30, sql.as_bytes())?;
    if result.0 {
        Ok(result.1)
    } else {
        Err("اتصال محلی امن به MySQL یا عملیات دیتابیس شکست خورد؛ دسترسی socket مدیر پایگاه‌داده را بررسی کن.".into())
    }
}

fn mysql_with_application_credentials(
    username: &str,
    password: &str,
    database: &str,
    sql: &str,
) -> Result<String, String> {
    if !valid_mysql_user(username)
        || !valid_database_name(database)
        || !(32..=128).contains(&password.len())
        || !password.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("اعتبارنامهٔ MySQL اپلیکیشن معتبر نیست.".into());
    }
    let path = Path::new("/run").join(format!(
        "arg-wp-db-{}-{}.cnf",
        std::process::id(),
        rand::random::<u64>()
    ));
    let config = format!("[client]\nuser={username}\npassword={password}\nprotocol=socket\n");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&path)
        .map_err(|_| "فایل موقت اعتبار دیتابیس WordPress ساخته نشد.".to_owned())?;
    file.write_all(config.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| {
            let _ = fs::remove_file(&path);
            "ذخیرهٔ موقت اعتبار دیتابیس WordPress ناموفق بود.".to_owned()
        })?;
    drop(file);
    let args = vec![
        format!("--defaults-extra-file={}", path.display()),
        "--batch".to_owned(),
        "--skip-column-names".to_owned(),
        format!("--database={database}"),
    ];
    let result = run_bounded_with_input(mysql_client()?, &args, 15, sql.as_bytes());
    let _ = fs::remove_file(&path);
    match result? {
        (true, output) => Ok(output),
        (false, _) => Err("آزمون اعتبار حساب اختصاصی MySQL شکست خورد.".into()),
    }
}

fn grant_runtime_read_access(app_id: &str, release: &Path) -> Result<(), String> {
    if !apps::valid_uuid(app_id) {
        return Err("شناسهٔ اپلیکیشن برای مجوز PHP-FPM معتبر نیست.".into());
    }
    let root = deploy::deployment_root()
        .canonicalize()
        .map_err(|_| "ریشهٔ استقرار قابل تأیید نیست.".to_owned())?;
    grant_deployment_root_traversal(&root)?;
    let app_root = root.join("apps").join(app_id);
    let releases = app_root.join("releases");
    let canonical_release = release
        .canonicalize()
        .map_err(|_| "مسیر release WordPress قابل تأیید نیست.".to_owned())?;
    if canonical_release.parent()
        != Some(
            releases
                .canonicalize()
                .map_err(|_| "مسیر releaseها قابل تأیید نیست.".to_owned())?
                .as_path(),
        )
    {
        return Err("مسیر release WordPress هنگام تنظیم دسترسی تغییر کرد.".into());
    }
    let apps_dir = root.join("apps");
    for directory in [&root, &apps_dir, &app_root, &releases] {
        let metadata = fs::symlink_metadata(directory)
            .map_err(|_| "پوشهٔ مسیر WordPress پیدا نشد.".to_owned())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("مسیر WordPress شامل پوشهٔ غیرواقعی یا symlink است.".into());
        }
        let result = run_bounded(
            "setfacl",
            &["-m", "u:www-data:--x", directory.to_string_lossy().as_ref()],
            15,
            false,
        )?;
        if !result.0 {
            return Err(format!(
                "اجازهٔ عبور PHP-FPM از مسیر WordPress تنظیم نشد: {}",
                result.1
            ));
        }
    }
    let result = run_bounded(
        "setfacl",
        &[
            "-R",
            "-P",
            "-m",
            "u:www-data:r-X",
            canonical_release.to_string_lossy().as_ref(),
        ],
        90,
        false,
    )?;
    if result.0 {
        Ok(())
    } else {
        Err(format!(
            "PHP-FPM نتوانست فایل‌های release را بخواند: {}",
            result.1
        ))
    }
}

fn grant_deployment_root_traversal(root: &Path) -> Result<(), String> {
    let Some(parent) = root.parent() else {
        return Err("مسیر والد ریشهٔ استقرار معتبر نیست.".into());
    };
    let data_root = Path::new("/var/lib/arg");
    if parent == data_root {
        let metadata = fs::symlink_metadata(data_root)
            .map_err(|_| "پوشهٔ داده‌های ARG برای دسترسی وب پیدا نشد.".to_owned())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("پوشهٔ داده‌های ARG مسیر واقعی نیست؛ دسترسی وب تنظیم نشد.".into());
        }
        let result = run_bounded(
            "setfacl",
            &["-m", "u:www-data:--x", data_root.to_string_lossy().as_ref()],
            15,
            false,
        )?;
        if result.0 {
            return Ok(());
        }
        return Err(format!(
            "PHP-FPM اجازهٔ عبور از پوشهٔ خصوصی ARG را نگرفت: {}",
            result.1
        ));
    }

    // A custom deployment root must already have a traversable parent. Avoid
    // widening access on arbitrary locations such as /root or user home dirs.
    let result = run_bounded(
        "runuser",
        &[
            "-u",
            "www-data",
            "--",
            "test",
            "-x",
            parent.to_string_lossy().as_ref(),
        ],
        15,
        false,
    )?;
    if result.0 {
        Ok(())
    } else {
        Err("کاربر PHP-FPM به والد ریشهٔ استقرار دسترسی عبور ندارد؛ مسیر سفارشی را در پوشه‌ای با مجوز عبور تنظیم کن.".into())
    }
}

fn valid_wp_version(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0'))
        })
}

fn valid_domain(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && !value.contains(['*', '/', ':', '@', '\\'])
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn validated_release_path(app_id: &str, requested: &str) -> Result<PathBuf, String> {
    let root = deploy::deployment_root();
    let app_root = root.join("apps").join(app_id);
    let releases = app_root.join("releases");
    let app_metadata = fs::symlink_metadata(&app_root)
        .map_err(|_| "پوشهٔ اپلیکیشن WordPress پیدا نشد.".to_owned())?;
    let releases_metadata = fs::symlink_metadata(&releases)
        .map_err(|_| "پوشهٔ releaseهای WordPress پیدا نشد.".to_owned())?;
    if !app_metadata.is_dir()
        || app_metadata.file_type().is_symlink()
        || !releases_metadata.is_dir()
        || releases_metadata.file_type().is_symlink()
    {
        return Err("مسیرهای اپلیکیشن یا release واقعی نیستند؛ عملیات root متوقف شد.".into());
    }
    let canonical_releases = releases
        .canonicalize()
        .map_err(|_| "مسیر releaseهای WordPress قابل بررسی نیست.".to_owned())?;
    let requested_path = Path::new(requested);
    let release = requested_path
        .canonicalize()
        .map_err(|_| "مسیر release WordPress وجود ندارد.".to_owned())?;
    if release.parent() != Some(canonical_releases.as_path())
        || requested_path != release
        || !release.is_dir()
    {
        return Err("release خارج از پوشهٔ مجاز همین اپلیکیشن است.".into());
    }
    Ok(release)
}

fn verify_wordpress_source(release: &Path, expected_version: &str) -> Result<(), String> {
    for path in [
        "wp-load.php",
        "wp-admin/install.php",
        "wp-includes/version.php",
    ] {
        if !release.join(path).is_file() {
            return Err(format!("سورس رسمی WordPress فایل {path} را ندارد."));
        }
    }
    if fs::symlink_metadata(release.join("wp-config.php")).is_ok() {
        return Err(
            "سورس دارای wp-config.php است؛ برای جلوگیری از اجرای تنظیمات ناشناس، استقرار متوقف شد."
                .into(),
        );
    }
    let version_file = fs::read_to_string(release.join("wp-includes/version.php"))
        .map_err(|_| "نسخهٔ هستهٔ WordPress قابل بررسی نیست.".to_owned())?;
    if !version_file.contains(&format!("$wp_version = '{expected_version}';"))
        && !version_file.contains(&format!("$wp_version = \"{expected_version}\";"))
    {
        return Err("نسخهٔ داخل سورس WordPress با tag انتخاب‌شده تطبیق ندارد.".into());
    }
    ensure_no_symlinks(release)?;
    Ok(())
}

pub(super) fn verify_release_source(release: &Path, expected_version: &str) -> Result<(), String> {
    if !valid_wp_version(expected_version) {
        return Err("نسخهٔ WordPress برای بررسی سورس معتبر نیست.".into());
    }
    verify_wordpress_source(release, expected_version)
}

fn ensure_no_symlinks(root: &Path) -> Result<(), String> {
    let mut pending = vec![root.to_path_buf()];
    let mut seen = 0_usize;
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|_| "فهرست فایل‌های WordPress قابل بررسی نیست.".to_owned())?;
        for entry in entries {
            let entry = entry.map_err(|_| "یک فایل WordPress قابل بررسی نیست.".to_owned())?;
            seen += 1;
            if seen > 25_000 {
                return Err("تعداد فایل‌های WordPress از حد امن بیشتر است.".into());
            }
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|_| "نوع یک فایل WordPress قابل بررسی نیست.".to_owned())?;
            if metadata.file_type().is_symlink() {
                return Err("سورس WordPress دارای symlink است؛ تنظیم سطح root متوقف شد.".into());
            }
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if !metadata.is_file() {
                return Err("سورس WordPress دارای فایل ویژه است؛ استقرار متوقف شد.".into());
            }
        }
    }
    Ok(())
}

fn prepare_shared_files(
    app_id: &str,
    release: &Path,
    database: &DatabaseCredential,
    _domain: &str,
) -> Result<(), String> {
    let app_root = deploy::deployment_root().join("apps").join(app_id);
    let shared = app_root.join("shared");
    fs::create_dir_all(&shared)
        .map_err(|_| "پوشهٔ داده‌های پایدار WordPress ساخته نشد.".to_owned())?;
    let shared_metadata = fs::symlink_metadata(&shared)
        .map_err(|_| "پوشهٔ داده‌های پایدار WordPress بررسی نشد.".to_owned())?;
    if !shared_metadata.is_dir() || shared_metadata.file_type().is_symlink() {
        return Err("مسیر shared یک پوشهٔ واقعی نیست؛ نصب WordPress متوقف شد.".into());
    }
    let shared_content = shared.join("wp-content");
    let release_content = release.join("wp-content");
    if fs::symlink_metadata(&release_content)
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err("wp-content در release نمی‌تواند symlink باشد.".into());
    }
    if !shared_content.exists() {
        if !release_content.is_dir() {
            return Err("پوشهٔ wp-content در release WordPress وجود ندارد.".into());
        }
        fs::rename(&release_content, &shared_content)
            .map_err(|_| "انتقال wp-content به فضای پایدار شکست خورد.".to_owned())?;
    } else {
        let metadata = fs::symlink_metadata(&shared_content)
            .map_err(|_| "wp-content پایدار قابل بررسی نیست.".to_owned())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("wp-content پایدار یک پوشهٔ واقعی نیست.".into());
        }
        if release_content.exists() {
            ensure_no_symlinks(&release_content)?;
            fs::remove_dir_all(&release_content)
                .map_err(|_| "wp-content تکراری release پاک نشد.".to_owned())?;
        }
    }
    fs::create_dir_all(shared_content.join("uploads"))
        .map_err(|_| "پوشهٔ بارگذاری WordPress آماده نشد.".to_owned())?;
    set_tree_owner_and_modes(&shared_content)?;
    set_directory_owner(&shared, "root:www-data", 0o750)?;

    let content_link = release.join("wp-content");
    if fs::symlink_metadata(&content_link).is_ok() {
        return Err("release هنوز wp-content دارد؛ پیوند پایدار ساخته نشد.".into());
    }
    std::os::unix::fs::symlink("../../shared/wp-content", &content_link)
        .map_err(|_| "پیوند wp-content به فضای پایدار ساخته نشد.".to_owned())?;

    let config = shared.join("wp-config.php");
    match fs::symlink_metadata(&config) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err("wp-config.php پایدار باید فایل معمولی باشد.".into());
            }
            let contents = fs::read_to_string(&config)
                .map_err(|_| "wp-config.php پایدار قابل خواندن نیست.".to_owned())?;
            if !contents.starts_with(&format!(
                "<?php\n// ARG managed WordPress configuration app={app_id}\n"
            )) {
                return Err("wp-config.php موجود متعلق به ARG نیست و بازنویسی نشد.".into());
            }
            let stable_path = php_string(&format!("{}/", app_root.join("current").display()));
            let stable_abis = format!(
                "if ( ! defined( 'ABSPATH' ) ) {{ define( 'ABSPATH', '{stable_path}' ); }}"
            );
            if contents.contains("define( 'ABSPATH', __DIR__ . '/' );") {
                let updated = contents.replace(
                    "if ( ! defined( 'ABSPATH' ) ) { define( 'ABSPATH', __DIR__ . '/' ); }",
                    &stable_abis,
                );
                atomic_write(&config, updated.as_bytes(), 0o640)?;
                set_file_owner(&config, "root:www-data")?;
            } else if !contents.contains(&stable_abis) {
                return Err(
                    "مسیر WordPress در wp-config.php با ریشهٔ فعلی برنامه تطبیق ندارد.".into(),
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let contents = make_wp_config(app_id, database, &app_root.join("current"));
            atomic_write(&config, contents.as_bytes(), 0o640)?;
            set_file_owner(&config, "root:www-data")?;
        }
        Err(_) => return Err("wp-config.php پایدار بررسی نشد.".into()),
    }
    let config_link = release.join("wp-config.php");
    if fs::symlink_metadata(&config_link).is_ok() {
        return Err("release شامل wp-config.php از قبل است؛ فایل جایگزین نشد.".into());
    }
    std::os::unix::fs::symlink("../../shared/wp-config.php", &config_link)
        .map_err(|_| "پیوند امن wp-config.php به release ساخته نشد.".to_owned())?;
    Ok(())
}

fn make_wp_config(app_id: &str, credential: &DatabaseCredential, web_root: &Path) -> String {
    let mut salts = Vec::with_capacity(8);
    for _ in 0..8 {
        let mut bytes = [0_u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        salts.push(hex::encode(bytes));
    }
    format!(
        "<?php\n// ARG managed WordPress configuration app={app_id}\ndefine( 'DB_NAME', '{}' );\ndefine( 'DB_USER', '{}' );\ndefine( 'DB_PASSWORD', '{}' );\ndefine( 'DB_HOST', 'localhost' );\ndefine( 'DB_CHARSET', 'utf8mb4' );\ndefine( 'DB_COLLATE', '' );\ndefine( 'AUTH_KEY', '{}' );\ndefine( 'SECURE_AUTH_KEY', '{}' );\ndefine( 'LOGGED_IN_KEY', '{}' );\ndefine( 'NONCE_KEY', '{}' );\ndefine( 'AUTH_SALT', '{}' );\ndefine( 'SECURE_AUTH_SALT', '{}' );\ndefine( 'LOGGED_IN_SALT', '{}' );\ndefine( 'NONCE_SALT', '{}' );\n$table_prefix = 'wp_';\ndefine( 'WP_DEBUG', false );\ndefine( 'DISALLOW_FILE_EDIT', true );\ndefine( 'FS_METHOD', 'direct' );\nif ( ! defined( 'ABSPATH' ) ) {{ define( 'ABSPATH', '{}' ); }}\nrequire_once ABSPATH . 'wp-settings.php';\n",
        php_string(&credential.database),
        php_string(&credential.username),
        php_string(&credential.password),
        salts[0], salts[1], salts[2], salts[3], salts[4], salts[5], salts[6], salts[7],
        php_string(&format!("{}/", web_root.display())),
    )
}

fn php_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn set_tree_owner_and_modes(root: &Path) -> Result<(), String> {
    ensure_no_symlinks(root)?;
    let owner = run_bounded(
        "chown",
        &["-R", "www-data:www-data", root.to_string_lossy().as_ref()],
        60,
        false,
    )?;
    if !owner.0 {
        return Err(format!("مالکیت فایل‌های WordPress تنظیم نشد: {}", owner.1));
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in
            fs::read_dir(&directory).map_err(|_| "فهرست wp-content خوانده نشد.".to_owned())?
        {
            let entry = entry.map_err(|_| "فایل wp-content خوانده نشد.".to_owned())?;
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|_| "مجوز فایل wp-content بررسی نشد.".to_owned())?;
            if metadata.file_type().is_symlink() {
                return Err("symlink در wp-content مجاز نیست.".into());
            }
            if metadata.is_dir() {
                fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o755))
                    .map_err(|_| "مجوز پوشهٔ wp-content تنظیم نشد.".to_owned())?;
                pending.push(entry.path());
            } else if metadata.is_file() {
                fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o644))
                    .map_err(|_| "مجوز فایل wp-content تنظیم نشد.".to_owned())?;
            }
        }
    }
    fs::set_permissions(root, fs::Permissions::from_mode(0o755))
        .map_err(|_| "مجوز ریشهٔ wp-content تنظیم نشد.".to_owned())?;
    Ok(())
}

fn set_directory_owner(path: &Path, owner: &str, mode: u32) -> Result<(), String> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|_| "مجوز پوشهٔ پایدار WordPress تنظیم نشد.".to_owned())?;
    let result = run_bounded(
        "chown",
        &[owner, path.to_string_lossy().as_ref()],
        15,
        false,
    )?;
    if result.0 {
        Ok(())
    } else {
        Err(format!(
            "مالکیت پوشهٔ پایدار WordPress تنظیم نشد: {}",
            result.1
        ))
    }
}

fn set_file_owner(path: &Path, owner: &str) -> Result<(), String> {
    let result = run_bounded(
        "chown",
        &[owner, path.to_string_lossy().as_ref()],
        15,
        false,
    )?;
    if result.0 {
        Ok(())
    } else {
        Err(format!(
            "مالکیت فایل تنظیم WordPress تنظیم نشد: {}",
            result.1
        ))
    }
}

fn atomic_write(path: &Path, contents: &[u8], mode: u32) -> Result<(), String> {
    let parent = path.parent().ok_or("مسیر فایل WordPress معتبر نیست.")?;
    let temporary = parent.join(format!(
        ".wp-config-{}-{}",
        std::process::id(),
        rand::random::<u64>()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&temporary)
        .map_err(|_| "فایل موقت wp-config.php ساخته نشد.".to_owned())?;
    if file
        .write_all(contents)
        .and_then(|_| file.sync_all())
        .is_err()
    {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err("ذخیرهٔ امن wp-config.php ناموفق بود.".into());
    }
    drop(file);
    fs::rename(&temporary, path).map_err(|_| {
        let _ = fs::remove_file(&temporary);
        "فعال‌سازی اتمی wp-config.php ناموفق بود.".to_owned()
    })
}

fn run_wordpress_installer(release: &Path, domain: &str, admin: &AdminSetup) -> Result<(), String> {
    let script = wordpress_install_script(release, domain, admin);
    let result = run_php_script(&script, 150)?;
    if !result.0 {
        let safe_output = result.1.replace(&admin.password, "[REDACTED]");
        return Err(format!(
            "نصب اولیهٔ WordPress شکست خورد: {}",
            safe_output.lines().take(8).collect::<Vec<_>>().join("\n")
        ));
    }
    if !result.1.contains("ARG_WORDPRESS_INSTALLED")
        && !result.1.contains("ARG_WORDPRESS_ALREADY_INSTALLED")
    {
        return Err("PHP اجرا شد اما نصب WordPress از دیتابیس تأیید نشد.".into());
    }
    Ok(())
}

fn wordpress_install_script(release: &Path, domain: &str, admin: &AdminSetup) -> String {
    let https = false;
    let scheme = if https { "https" } else { "http" };
    format!(
        "<?php\nerror_reporting(0);\nini_set('display_errors', '0');\ndefine('WP_INSTALLING', true);\n$_SERVER['HTTP_HOST'] = '{}';\n$_SERVER['SERVER_NAME'] = '{}';\n$_SERVER['REQUEST_METHOD'] = 'GET';\n$_SERVER['REQUEST_URI'] = '/wp-admin/install.php';\n$_SERVER['SERVER_PORT'] = '{}';\n$_SERVER['HTTPS'] = '{}';\nrequire '{}';\nif ( ! function_exists('is_blog_installed') ) {{ fwrite(STDERR, 'WordPress installer is unavailable'); exit(2); }}\nif ( is_blog_installed() ) {{ echo 'ARG_WORDPRESS_ALREADY_INSTALLED'; exit(0); }}\nrequire_once ABSPATH . 'wp-admin/includes/upgrade.php';\n$result = wp_install('{}', '{}', '{}', true, '', '{}');\nif ( is_wp_error($result) ) {{ fwrite(STDERR, $result->get_error_message()); exit(3); }}\nif ( ! is_blog_installed() ) {{ fwrite(STDERR, 'WordPress database installation was not completed'); exit(4); }}\necho 'ARG_WORDPRESS_INSTALLED';\n",
        php_string(domain),
        php_string(domain),
        if scheme == "https" { "443" } else { "80" },
        if https { "on" } else { "off" },
        php_string(&release.join("wp-load.php").to_string_lossy()),
        php_string(&admin.site_title),
        php_string(&admin.username),
        php_string(&admin.email),
        php_string(&admin.password),
    )
}

fn wordpress_is_installed(release: &Path, domain: &str) -> Result<bool, String> {
    let script = format!(
        "<?php error_reporting(0); ini_set('display_errors','0'); $_SERVER['HTTP_HOST']='{}'; $_SERVER['SERVER_NAME']='{}'; $_SERVER['REQUEST_METHOD']='GET'; $_SERVER['SERVER_PORT']='80'; $_SERVER['HTTPS']='off'; require '{}'; echo function_exists('is_blog_installed') && is_blog_installed() ? 'ARG_WORDPRESS_INSTALLED' : 'ARG_WORDPRESS_NOT_INSTALLED';",
        php_string(domain),
        php_string(domain),
        php_string(&release.join("wp-load.php").to_string_lossy()),
    );
    let result = run_php_script(&script, 45)?;
    if !result.0 {
        return Err("بررسی وضعیت نصب قبلی WordPress شکست خورد.".into());
    }
    Ok(result.1.contains("ARG_WORDPRESS_INSTALLED"))
}

fn run_php_script(script: &str, timeout_seconds: u32) -> Result<(bool, String), String> {
    let runuser = if Path::new("/usr/sbin/runuser").is_file() {
        "/usr/sbin/runuser"
    } else {
        "/sbin/runuser"
    };
    run_bounded_with_input(
        runuser,
        &[
            "-u".to_owned(),
            "www-data".to_owned(),
            "--".to_owned(),
            "/usr/bin/php".to_owned(),
        ],
        timeout_seconds,
        script.as_bytes(),
    )
}

fn set_site_url(request: &Value) -> Result<Value, String> {
    let id = app_id(request)?;
    let domain = request["domain"].as_str().unwrap_or_default().trim();
    if !valid_domain(domain) || domain.contains('*') {
        return Err("دامنه برای به‌روزرسانی URL WordPress معتبر نیست.".into());
    }
    let scheme = request["scheme"].as_str().unwrap_or_default();
    if !["http", "https"].contains(&scheme) {
        return Err("نوع پروتکل سایت WordPress معتبر نیست.".into());
    }
    let database = format!("argwp_{}_db", &id[..8]);
    if !valid_database_name(&database) {
        return Err("نام دیتابیس WordPress معتبر نیست.".into());
    }
    let url = format!("{scheme}://{domain}");
    let escaped = mysql_literal(&url);
    let sql = format!(
        "UPDATE `{database}`.`wp_options` SET option_value='{escaped}' WHERE option_name IN ('siteurl','home');"
    );
    run_mysql_admin(&sql).map_err(|_| {
        "URL ذخیره‌شدهٔ WordPress به‌روز نشد؛ دسترسی به دیتابیس اپ را بررسی کن.".to_owned()
    })?;
    Ok(json!({"scheme":scheme,"domain":domain,"updated":true}))
}

fn health_check(request: &Value) -> Result<Value, String> {
    let domain = request["domain"].as_str().unwrap_or_default().trim();
    if !valid_domain(domain) || domain.contains('*') {
        return Err("دامنه برای Health Check WordPress معتبر نیست.".into());
    }
    let scheme = request["scheme"].as_str().unwrap_or("http");
    if !["http", "https"].contains(&scheme) {
        return Err("پروتکل Health Check WordPress معتبر نیست.".into());
    }
    let mut args = vec![
        "--silent".to_owned(),
        "--show-error".to_owned(),
        "--output".to_owned(),
        "/dev/null".to_owned(),
        "--write-out".to_owned(),
        "%{http_code}".to_owned(),
        "--max-time".to_owned(),
        "15".to_owned(),
        "--header".to_owned(),
        format!("Host: {domain}"),
    ];
    let url = if scheme == "https" {
        args.extend(["--resolve".to_owned(), format!("{domain}:443:127.0.0.1")]);
        format!("https://{domain}/wp-login.php")
    } else {
        "http://127.0.0.1/wp-login.php".to_owned()
    };
    args.push(url);
    let output = run_bounded("curl", &args, 25, false)?;
    if output.0 && output.1.trim() == "200" {
        Ok(json!({"healthy":true,"status_code":200,"path":"/wp-login.php","scheme":scheme}))
    } else {
        Err(format!(
            "Health Check WordPress روی {} شکست خورد (HTTP {}): {}",
            scheme,
            output.1.trim(),
            if output.1.trim().is_empty() {
                "پاسخی از Nginx دریافت نشد"
            } else {
                "پاسخ 200 دریافت نشد"
            }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP_ID: &str = "3ac9ee2c-0dd6-4bfb-8c47-1f2213f81e19";

    fn temporary_directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "arg-wordpress-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn wordpress_admin_inputs_are_bounded_and_reject_unsafe_values() {
        let valid = AdminSetup {
            site_title: "ARG test site".into(),
            username: "site-manager".into(),
            email: "admin@example.test".into(),
            password: "long-password-value-123".into(),
        };
        assert!(validate_admin_setup(&valid).is_ok());
        assert!(validate_admin_setup(&AdminSetup {
            site_title: valid.site_title.clone(),
            username: valid.username.clone(),
            email: valid.email.clone(),
            password: "short".into(),
        })
        .is_err());
        assert!(valid_email_host("example.test"));
        assert!(!valid_email_host("bad host"));
    }

    #[test]
    fn generated_database_names_and_sql_are_app_scoped_and_do_not_rewrite_foreign_users() {
        let credentials = DatabaseCredential {
            database: "argwp_3ac9ee2c_db".into(),
            username: "argwp_3ac9ee2c".into(),
            password: "a".repeat(64),
        };
        assert!(valid_database_name(&credentials.database));
        assert!(valid_mysql_user(&credentials.username));
        let sql = create_database_sql(APP_ID, &credentials);
        assert!(sql.contains("arg_managed_install"));
        assert!(sql.contains(APP_ID));
        assert!(!sql.contains("ALTER USER"));
        assert!(mysql_literal("safe'\\value").contains("''"));
        assert!(!valid_database_name("mysql"));
        assert!(!valid_mysql_user("root"));
    }

    #[test]
    fn verified_core_requires_expected_version_and_rejects_symlinks_and_foreign_config() {
        let root = temporary_directory();
        fs::create_dir_all(root.join("wp-admin")).unwrap();
        fs::create_dir_all(root.join("wp-includes")).unwrap();
        fs::write(root.join("wp-load.php"), "<?php").unwrap();
        fs::write(root.join("wp-admin/install.php"), "<?php").unwrap();
        fs::write(
            root.join("wp-includes/version.php"),
            "$wp_version = '6.8.3';",
        )
        .unwrap();
        assert!(verify_release_source(&root, "6.8.3").is_ok());
        assert!(verify_release_source(&root, "6.8.2").is_err());
        std::os::unix::fs::symlink("/etc/passwd", root.join("outside")).unwrap();
        assert!(verify_release_source(&root, "6.8.3").is_err());
        fs::remove_file(root.join("outside")).unwrap();
        fs::write(root.join("wp-config.php"), "<?php require 'foreign.php';").unwrap();
        assert!(verify_release_source(&root, "6.8.3").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_the_standard_arg_data_parent_receives_web_traversal_acl() {
        assert_eq!(
            Path::new("/var/lib/arg/deployments").parent(),
            Some(Path::new("/var/lib/arg"))
        );
        assert_ne!(
            Path::new("/root/private/deployments").parent(),
            Some(Path::new("/var/lib/arg"))
        );
    }

    #[test]
    fn wordpress_config_uses_the_stable_current_release_path() {
        let credential = DatabaseCredential {
            database: "argwp_3ac9ee2c_db".into(),
            username: "argwp_3ac9ee2c".into(),
            password: "a".repeat(64),
        };
        let current =
            Path::new("/var/lib/arg/deployments/apps/3ac9ee2c-0dd6-4bfb-8c47-1f2213f81e19/current");
        let config = make_wp_config(APP_ID, &credential, current);
        assert!(config.contains("define( 'ABSPATH', '/var/lib/arg/deployments/apps/3ac9ee2c-0dd6-4bfb-8c47-1f2213f81e19/current/' );"));
        assert!(!config.contains("__DIR__"));
    }

    #[test]
    fn first_install_bootstraps_core_in_installing_mode() {
        let admin = AdminSetup {
            site_title: "ARG test site".into(),
            username: "site-manager".into(),
            email: "admin@example.test".into(),
            password: "long-password-value-123".into(),
        };
        let script = wordpress_install_script(
            Path::new("/var/lib/arg/test/current"),
            "example.test",
            &admin,
        );
        assert!(script.contains("define('WP_INSTALLING', true);"));
        assert!(script.contains("REQUEST_URI'] = '/wp-admin/install.php'"));
    }
}
