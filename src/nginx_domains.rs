use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkInput {
    site_name: String,
    confirmed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UnlinkInput {
    confirmed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DomainsInput {
    domains: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConfigInput {
    content: String,
    #[serde(default)]
    expected_hash: Option<String>,
}

fn valid_site(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 180
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS app_nginx_sites(site_name TEXT PRIMARY KEY,application_id TEXT NOT NULL UNIQUE REFERENCES applications(id) ON DELETE CASCADE,created_by TEXT NOT NULL,created_at INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS app_nginx_sites_app ON app_nginx_sites(application_id);")
}

pub(super) async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let (name, domain, framework, linked_site) = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let app: Option<(String, Option<String>, String)> = db
            .query_row(
                "SELECT name,domain,framework FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
                params![app_id, owner],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(db_error)?;
        let Some((name, domain, framework)) = app else {
            return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن فعال پیدا نشد."));
        };
        let linked_site: Option<String> = db
            .query_row(
                "SELECT site_name FROM app_nginx_sites WHERE application_id=?1",
                [&app_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?;
        (name, domain, framework, linked_site)
    };
    let inventory = match nginx::nginx_manager_request_async(json!({"action":"nginx-list"})).await {
        Ok(value) => value,
        Err((_, body)) => {
            return Ok(Json(
                json!({"application_id":app_id,"application_name":name,"domain":domain,"framework":framework,"site":null,"configured":false,"warning":body["error"].as_str().unwrap_or("وضعیت Nginx در دسترس نیست.")}),
            ))
        }
    };
    let sites = inventory["sites"].as_array().cloned().unwrap_or_default();
    let owned = sites
        .iter()
        .filter(|site| site["application_id"].as_str() == Some(app_id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let selected = owned
        .iter()
        .find(|site| site["enabled"] == true)
        .or_else(|| {
            linked_site
                .as_deref()
                .and_then(|name| sites.iter().find(|site| site["name"] == name))
        })
        .or_else(|| owned.first());
    let Some(selected) = selected else {
        return Ok(Json(
            json!({"application_id":app_id,"application_name":name,"domain":domain,"framework":framework,"site":null,"configured":false,"warning":null}),
        ));
    };
    let site_name = selected["name"].as_str().unwrap_or_default();
    let detail =
        nginx::nginx_manager_request_async(json!({"action":"nginx-get","name":site_name})).await?;
    let content = detail["site"]["content"].as_str().unwrap_or_default();
    let mut site = selected.clone();
    site["content"] = json!(content);
    site["content_hash"] = json!(hex::encode(Sha256::digest(content.as_bytes())));
    Ok(Json(
        json!({"application_id":app_id,"application_name":name,"domain":domain,"framework":framework,"site":site,"configured":true,"managed":selected["application_id"].as_str()==Some(app_id.as_str()),"domains":nginx::nginx_server_names(content)}),
    ))
}

pub(super) async fn ensure(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    let result = ensure_for_application(&state, &app_id, &actor).await?;
    Ok(Json(result))
}

/// Provision a validated, disabled Nginx virtual host for a registered app.
/// The site is activated only after its runtime/release has passed deployment.
pub(super) async fn ensure_for_application(
    state: &AppState,
    app_id: &str,
    actor: &str,
) -> ApiResultValue {
    let app = {
        let db = state.lock().unwrap();
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        db.query_row(
            "SELECT domain,framework,listen_port,static_output_path,static_spa_fallback FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
            params![app_id, owner],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<u16>>(2)?, row.get::<_, String>(3)?, row.get::<_, bool>(4)?)),
        ).optional().map_err(db_error)?.ok_or_else(|| error(StatusCode::NOT_FOUND,"اپلیکیشن فعال پیدا نشد."))?
    };
    let Some(domain) = app.0.as_deref() else {
        return Ok(
            json!({"configured":false,"message":"دامنه‌ای ثبت نشده؛ دامنه را در تب دامنه و Nginx وارد کن."}),
        );
    };
    let name = site_name_for(app_id, &app.1);
    let content = initial_config(app_id, domain, &app.1, app.2, &app.3, app.4)?;
    let existing =
        nginx::nginx_manager_request_async(json!({"action":"nginx-get","name":name})).await;
    let (content, created) = match existing {
        Ok(value) => {
            let current = value["site"]["content"].as_str().unwrap_or_default();
            if !nginx::nginx_managed_application_id(current).is_some_and(|id| id == app_id) {
                return Err(error(
                    StatusCode::CONFLICT,
                    "فایل Nginx رزروشدهٔ اپ وجود دارد ولی مالکیت آن تأیید نشد؛ فایل تغییر نکرد.",
                ));
            }
            (current.to_owned(), false)
        }
        Err((_, body))
            if body["error"]
                .as_str()
                .is_some_and(|message| message.contains("پیدا نشد")) =>
        {
            nginx::nginx_manager_request_async(
                json!({"action":"nginx-create","name":name,"content":content}),
            )
            .await?;
            (content, true)
        }
        Err(problem) => return Err(problem),
    };
    {
        let db = state.lock().unwrap();
        db.execute("INSERT INTO nginx_site_metadata(name,description) VALUES (?1,?2) ON CONFLICT(name) DO NOTHING", params![name,format!("ARG application {app_id}")]).map_err(db_error)?;
        nginx::store_nginx_snapshot(&db, &name, &content, None).map_err(db_error)?;
        db.execute("INSERT OR IGNORE INTO app_nginx_sites(site_name,application_id,created_by,created_at) VALUES (?1,?2,?3,?4)",params![name,app_id,actor,now()]).map_err(db_error)?;
        if app.1 == "laravel" {
            let socket = format!("/run/php/arg-app-{}.sock", &app_id[..8]);
            db.execute(
                "INSERT INTO laravel_app_sites(application_id,site_name,domain,php_fpm_socket,enabled,created_at,updated_at) VALUES (?1,?2,?3,?4,1,?5,?5) ON CONFLICT(application_id) DO UPDATE SET site_name=excluded.site_name,domain=excluded.domain,php_fpm_socket=excluded.php_fpm_socket,enabled=1,updated_at=excluded.updated_at",
                params![app_id,name,domain,socket,now()],
            ).map_err(db_error)?;
        }
        audit(
            &db,
            actor,
            "app.nginx.ensure",
            &format!("app={app_id}; site={name}; created={created}"),
        );
    }
    Ok(
        json!({"configured":true,"created":created,"site":{"name":name,"enabled":false,"content":content,"content_hash":hex::encode(Sha256::digest(content.as_bytes())),"application_id":app_id},"domains":nginx::nginx_server_names(&content),"message":"کانفیگ Nginx ساخته و با nginx -t بررسی شد؛ تا آماده‌شدن release غیرفعال می‌ماند."}),
    )
}

type ApiResultValue = Result<Value, (StatusCode, Json<Value>)>;

fn site_name_for(app_id: &str, framework: &str) -> String {
    match framework {
        "laravel" => format!("arg-app-{}", &app_id[..8]),
        "node" | "next" | "nuxt" => format!("arg-node-{}", &app_id[..12]),
        "static" | "react" | "vue" | "vite" => format!("arg-static-{}", &app_id[..12]),
        _ => format!("arg-managed-{}", &app_id[..12]),
    }
}

fn initial_config(
    app_id: &str,
    domain: &str,
    framework: &str,
    port: Option<u16>,
    _output: &str,
    spa: bool,
) -> Result<String, (StatusCode, Json<Value>)> {
    if !apps::valid_domain(domain) || domain.contains('*') {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "دامنه برای کانفیگ Nginx معتبر نیست.",
        ));
    }
    Ok(match framework {
        "laravel" => {
            let socket = format!("/run/php/arg-app-{}.sock", &app_id[..8]);
            laravel::app_nginx_config(app_id,domain,&socket,None)
        }
        "node" | "next" | "nuxt" => {
            let port = port.filter(|port| *port >= 1024).ok_or_else(|| error(StatusCode::CONFLICT,"برای اپلیکیشن Node باید پورت loopback رزرو شود."))?;
            nginx::node_proxy_config(app_id,domain,port)
        }
        "static" | "react" | "vue" | "vite" => {
            let root = std::env::var_os("ARG_DEPLOY_ROOT").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from("/var/lib/arg/deployments")).join("apps").join(app_id).join("current");
            let root_text = root.to_str().filter(|path| path.starts_with('/') && !path.contains("..") && path.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-/".contains(&byte))).ok_or_else(|| error(StatusCode::BAD_REQUEST,"مسیر انتشار برای ساخت امن کانفیگ Nginx معتبر نیست."))?;
            nginx::static_site_config(app_id,domain,std::path::Path::new(root_text),spa)
        }
        _ => format!("# ARG managed Application site app={app_id}\nserver {{\n    listen 80;\n    server_name {domain};\n    server_tokens off;\n    location / {{ return 503; }}\n    location ~ /\\.(?!well-known(?:/|$)) {{ deny all; }}\n}}\n"),
    })
}

pub(super) async fn update_domains(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<DomainsInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if input.domains.len() > 20
        || input
            .domains
            .iter()
            .any(|domain| !apps::valid_domain(domain.trim()) || domain.trim().contains('*'))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "حداکثر ۲۰ دامنهٔ معتبر و بدون wildcard قابل ثبت است.",
        ));
    }
    let mut domains = input
        .domains
        .iter()
        .map(|domain| domain.trim().trim_end_matches('.').to_ascii_lowercase())
        .collect::<Vec<_>>();
    let original_count = domains.len();
    domains.sort();
    domains.dedup();
    if domains.len() != original_count {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "دامنهٔ تکراری قابل ثبت نیست.",
        ));
    }
    // Keep the first item supplied by the user as the primary name.
    let primary = input
        .domains
        .first()
        .map(|domain| domain.trim().trim_end_matches('.').to_ascii_lowercase());
    if let Some(primary) = primary.as_deref() {
        if let Some(index) = domains.iter().position(|domain| domain == primary) {
            domains.swap(0, index);
        }
    }
    let owner = {
        let db = state.lock().unwrap();
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL)",params![app_id,owner],|row|row.get(0)).map_err(db_error)?;
        if !exists {
            return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن فعال پیدا نشد."));
        }
        let mut statement = db.prepare("SELECT id,domain FROM applications WHERE owner_server_uuid=?1 AND deleted_at IS NULL AND id<>?2 AND domain IS NOT NULL").map_err(db_error)?;
        let rows = statement
            .query_map(params![owner, app_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(db_error)?;
        for row in rows {
            let (other_id, other_domain) = row.map_err(db_error)?;
            if domains
                .iter()
                .any(|domain| apps::domain_patterns_overlap(domain, &other_domain))
            {
                return Err(error(
                    StatusCode::CONFLICT,
                    &format!(
                        "دامنهٔ {} از قبل به اپلیکیشن {other_id} اختصاص یافته است.",
                        domains
                            .iter()
                            .find(|domain| apps::domain_patterns_overlap(domain, &other_domain))
                            .map(String::as_str)
                            .unwrap_or(&other_domain)
                    ),
                ));
            }
        }
        owner
    };
    let current = get(State(state.clone()), headers.clone(), Path(app_id.clone())).await?;
    let current_value = current.0;
    let previous_primary = current_value["domain"].as_str().map(str::to_owned);
    let site = current_value["site"].clone();
    let site_name = site["name"].as_str().map(str::to_owned);
    if let Some(site_name) = site_name.as_deref() {
        let inventory = nginx::nginx_manager_request_async(json!({"action":"nginx-list"})).await?;
        for other in inventory["sites"].as_array().into_iter().flatten() {
            if other["name"].as_str() == Some(site_name)
                || other["application_id"].as_str() == Some(app_id.as_str())
            {
                continue;
            }
            let other_name = other["name"].as_str().unwrap_or("سایت موجود");
            for existing in other["server_names"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if domains
                    .iter()
                    .any(|domain| nginx::server_name_overlaps(existing, domain))
                {
                    return Err(error(
                        StatusCode::CONFLICT,
                        &format!(
                            "دامنهٔ {} قبلاً در سایت Nginx «{other_name}» استفاده شده است.",
                            domains
                                .iter()
                                .find(|domain| nginx::server_name_overlaps(existing, domain))
                                .map(String::as_str)
                                .unwrap_or(existing)
                        ),
                    ));
                }
            }
        }
        let detail =
            nginx::nginx_manager_request_async(json!({"action":"nginx-get","name":site_name}))
                .await?;
        let old_content = detail["site"]["content"].as_str().unwrap_or_default();
        let next_content = replace_server_names(old_content, &domains)?;
        nginx::nginx_manager_request_async(
            json!({"action":"nginx-save","name":site_name,"content":next_content}),
        )
        .await?;
        let update_result: ApiResultValue = (|| {
            let db = state.lock().unwrap();
            let tx = db.unchecked_transaction().map_err(db_error)?;
            let changed=tx.execute("UPDATE applications SET domain=?1,version=version+1,updated_at=?2 WHERE id=?3 AND owner_server_uuid=?4 AND deleted_at IS NULL",params![primary,now(),app_id,owner]).map_err(db_error)?;
            if changed == 0 {
                Err(error(StatusCode::CONFLICT, "اپلیکیشن هم‌زمان تغییر کرد."))
            } else {
                tx.execute(
                    "UPDATE laravel_app_sites SET domain=?1,updated_at=?2 WHERE application_id=?3",
                    params![primary, now(), app_id],
                )
                .map_err(db_error)?;
                nginx::store_nginx_snapshot(&tx, site_name, &next_content, None)
                    .map_err(db_error)?;
                audit(
                    &tx,
                    &actor,
                    "app.nginx.domains.update",
                    &format!("app={app_id}; domains={}", domains.join(",")),
                );
                tx.commit().map_err(db_error)?;
                Ok(json!({"updated":true}))
            }
        })();
        if let Err(problem) = update_result {
            let _ = nginx::nginx_manager_request_async(
                json!({"action":"nginx-save","name":site_name,"content":old_content}),
            )
            .await;
            return Err(problem);
        }
        return Ok(Json(
            json!({"domains":domains,"domain":primary,"site_name":site_name,"message":"فهرست دامنه‌ها ذخیره شد؛ nginx -t موفق بود و کانفیگ فعال با reload امن اعمال شد."}),
        ));
    }
    {
        let db = state.lock().unwrap();
        db.execute("UPDATE applications SET domain=?1,version=version+1,updated_at=?2 WHERE id=?3 AND owner_server_uuid=?4 AND deleted_at IS NULL",params![primary,now(),app_id,owner]).map_err(db_error)?;
        db.execute(
            "UPDATE laravel_app_sites SET domain=?1,updated_at=?2 WHERE application_id=?3",
            params![primary, now(), app_id],
        )
        .map_err(db_error)?;
    }
    if primary.is_some() {
        let configured = match ensure_for_application(&state, &app_id, &actor).await {
            Ok(configured) => configured,
            Err(problem) => {
                let db = state.lock().unwrap();
                let _=db.execute("UPDATE applications SET domain=?1,version=version+1,updated_at=?2 WHERE id=?3 AND owner_server_uuid=?4 AND deleted_at IS NULL",params![previous_primary,now(),app_id,owner]);
                let _ = db.execute(
                    "UPDATE laravel_app_sites SET domain=?1,updated_at=?2 WHERE application_id=?3",
                    params![previous_primary, now(), app_id],
                );
                return Err(problem);
            }
        };
        if configured["configured"] != true {
            let db = state.lock().unwrap();
            let _=db.execute("UPDATE applications SET domain=?1,version=version+1,updated_at=?2 WHERE id=?3 AND owner_server_uuid=?4 AND deleted_at IS NULL",params![previous_primary,now(),app_id,owner]);
            let _ = db.execute(
                "UPDATE laravel_app_sites SET domain=?1,updated_at=?2 WHERE application_id=?3",
                params![previous_primary, now(), app_id],
            );
            return Err(error(
                StatusCode::BAD_GATEWAY,
                configured["message"]
                    .as_str()
                    .unwrap_or("ساخت کانفیگ Nginx ناموفق بود."),
            ));
        }
        let site_name = configured["site"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let original_content = configured["site"]["content"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let content = if domains.len() > 1 {
            match replace_server_names(&original_content, &domains) {
                Ok(content) => content,
                Err(problem) => {
                    rollback_new_domain(
                        &state,
                        &app_id,
                        &owner,
                        previous_primary.as_deref(),
                        &site_name,
                        &original_content,
                        configured["created"] == true,
                    )
                    .await;
                    return Err(problem);
                }
            }
        } else {
            original_content.clone()
        };
        if domains.len() > 1 {
            if let Err(problem) = nginx::nginx_manager_request_async(
                json!({"action":"nginx-save","name":site_name,"content":content}),
            )
            .await
            {
                rollback_new_domain(
                    &state,
                    &app_id,
                    &owner,
                    previous_primary.as_deref(),
                    &site_name,
                    &original_content,
                    configured["created"] == true,
                )
                .await;
                return Err(problem);
            }
        }
        let snapshot_result: ApiResultValue = (|| {
            let db = state.lock().unwrap();
            nginx::store_nginx_snapshot(&db, &site_name, &content, None).map_err(db_error)?;
            audit(
                &db,
                &actor,
                "app.nginx.domains.update",
                &format!("app={app_id}; domains={}", domains.join(",")),
            );
            Ok(json!({"stored":true}))
        })();
        if let Err(problem) = snapshot_result {
            rollback_new_domain(
                &state,
                &app_id,
                &owner,
                previous_primary.as_deref(),
                &site_name,
                &original_content,
                configured["created"] == true,
            )
            .await;
            return Err(problem);
        }
        return Ok(Json(
            json!({"domains":domains,"domain":primary,"site_name":configured["site"]["name"],"message":"دامنه ذخیره و کانفیگ Nginx ساخته شد؛ سایت تا آماده‌شدن release غیرفعال است."}),
        ));
    }
    Ok(Json(
        json!({"domains":[],"domain":null,"message":"دامنهٔ اپ پاک شد؛ فایل Nginx حفظ شد تا سرویس یا استقرار موجود ناخواسته از کار نیفتد. سایت را در تب Nginx جداگانه غیرفعال کن."}),
    ))
}

async fn rollback_new_domain(
    state: &AppState,
    app_id: &str,
    owner: &str,
    previous_primary: Option<&str>,
    site_name: &str,
    original_content: &str,
    site_was_created: bool,
) {
    if site_was_created {
        if nginx::nginx_managed_application_id(original_content).as_deref() == Some(app_id) {
            if nginx::nginx_manager_request_async(json!({"action":"nginx-delete","name":site_name}))
                .await
                .is_ok()
            {
                let db = state.lock().unwrap();
                let _ = db.execute(
                    "DELETE FROM app_nginx_sites WHERE application_id=?1 AND site_name=?2",
                    params![app_id, site_name],
                );
                let _ = db.execute("DELETE FROM nginx_site_metadata WHERE name=?1", [site_name]);
                let _ = db.execute(
                    "DELETE FROM nginx_site_config_state WHERE name=?1",
                    [site_name],
                );
            }
        }
    } else {
        let _ = nginx::nginx_manager_request_async(
            json!({"action":"nginx-save","name":site_name,"content":original_content}),
        )
        .await;
    }
    let db = state.lock().unwrap();
    let _=db.execute("UPDATE applications SET domain=?1,version=version+1,updated_at=?2 WHERE id=?3 AND owner_server_uuid=?4 AND deleted_at IS NULL",params![previous_primary,now(),app_id,owner]);
    let _ = db.execute(
        "UPDATE laravel_app_sites SET domain=?1,updated_at=?2 WHERE application_id=?3",
        params![previous_primary, now(), app_id],
    );
}

fn replace_server_names(
    content: &str,
    domains: &[String],
) -> Result<String, (StatusCode, Json<Value>)> {
    if content.len() > 512 * 1024 || content.contains('\0') {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "کانفیگ Nginx نامعتبر یا بزرگ‌تر از 512 KB است.",
        ));
    }
    let replacement = if domains.is_empty() {
        "_".to_owned()
    } else {
        domains.join(" ")
    };
    let mut count = 0usize;
    let updated = content
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with("server_name ") && trimmed.trim_end().ends_with(';') {
                count += 1;
                let indent = &line[..line.len() - trimmed.len()];
                format!("{indent}server_name {replacement};")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if content.ends_with('\n') { "\n" } else { "" };
    if count == 0 {
        return Err(error(
            StatusCode::CONFLICT,
            "در کانفیگ server_name ساده و قابل ویرایش پیدا نشد؛ کانفیگ را بررسی کن.",
        ));
    }
    Ok(updated)
}

pub(super) async fn save_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<ConfigInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    if input.content.is_empty() || input.content.len() > 512 * 1024 || input.content.contains('\0')
    {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "کانفیگ باید غیرخالی و حداکثر 512 KB باشد.",
        ));
    }
    let current = get(State(state.clone()), headers.clone(), Path(app_id.clone())).await?;
    let value = current.0;
    let site = value["site"].clone();
    let site_name = site["name"]
        .as_str()
        .ok_or_else(|| error(StatusCode::CONFLICT, "ابتدا برای اپ کانفیگ Nginx بساز."))?;
    let old = site["content"].as_str().unwrap_or_default();
    let hash = hex::encode(Sha256::digest(old.as_bytes()));
    if input
        .expected_hash
        .as_deref()
        .is_some_and(|expected| expected != hash)
    {
        return Err(error(
            StatusCode::CONFLICT,
            "کانفیگ Nginx در جای دیگری تغییر کرده؛ صفحه را تازه کن و دوباره ویرایش کن.",
        ));
    }
    let configured = nginx::nginx_server_names(&input.content)
        .into_iter()
        .collect::<HashSet<_>>();
    let expected = nginx::nginx_server_names(old)
        .into_iter()
        .collect::<HashSet<_>>();
    if configured != expected {
        return Err(error(
            StatusCode::CONFLICT,
            "دامنه‌ها را از بخش مدیریت دامنه تغییر بده تا دامنهٔ ثبت‌شدهٔ اپ با Nginx هماهنگ بماند.",
        ));
    }
    nginx::nginx_manager_request_async(
        json!({"action":"nginx-save","name":site_name,"content":input.content}),
    )
    .await?;
    let db = state.lock().unwrap();
    nginx::store_nginx_snapshot(&db, site_name, &input.content, None).map_err(db_error)?;
    audit(
        &db,
        &actor,
        "app.nginx.config.update",
        &format!("app={app_id}; site={site_name}"),
    );
    Ok(Json(
        json!({"saved":true,"site_name":site_name,"content_hash":hex::encode(Sha256::digest(input.content.as_bytes())),"message":"کانفیگ ذخیره شد؛ nginx -t موفق بود و در صورت فعال‌بودن امن reload شد."}),
    ))
}

pub(super) async fn link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<LinkInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !input.confirmed || !valid_site(&input.site_name) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "تأیید اتصال و نام معتبر سایت Nginx لازم است.",
        ));
    }
    let (actor, owner, domain) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let domain: Option<String> = db
            .query_row(
                "SELECT domain FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL",
                params![app_id, owner],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "اپلیکیشن فعال پیدا نشد."))?;
        let Some(domain) = domain.filter(|value| !value.trim().is_empty()) else {
            return Err(error(
                StatusCode::CONFLICT,
                "ابتدا دامنهٔ اپلیکیشن را ثبت کن تا مالکیت سایت Nginx قابل بررسی باشد.",
            ));
        };
        (
            actor,
            owner,
            domain.trim().trim_end_matches('.').to_ascii_lowercase(),
        )
    };
    let response =
        nginx::nginx_manager_request_async(json!({"action":"nginx-get","name":input.site_name}))
            .await?;
    let content = nginx::nginx_site_content(&response)?;
    let names = nginx::nginx_server_names(content)
        .into_iter()
        .map(|value| value.trim_end_matches('.').to_ascii_lowercase())
        .collect::<Vec<_>>();
    if !names.iter().any(|name| name == &domain) {
        return Err(error(
            StatusCode::CONFLICT,
            "دامنهٔ ثبت‌شده برای اپلیکیشن در server_name این سایت Nginx پیدا نشد؛ پیکربندی تغییر نکرد.",
        ));
    }
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let current_owner = federation::local_server_uuid(&db).map_err(db_error)?;
    if current_owner != owner {
        return Err(error(
            StatusCode::CONFLICT,
            "هویت VPS هنگام ثبت لینک تغییر کرد.",
        ));
    }
    let linked = db.execute(
        "INSERT INTO app_nginx_sites(site_name,application_id,created_by,created_at) VALUES(?1,?2,?3,?4) ON CONFLICT(application_id) DO UPDATE SET site_name=excluded.site_name,created_by=excluded.created_by,created_at=excluded.created_at",
        params![input.site_name, app_id, actor, now()],
    );
    if let Err(problem) = linked {
        return Err(
            if problem.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                error(
                    StatusCode::CONFLICT,
                    "این سایت Nginx قبلاً به اپلیکیشن دیگری متصل شده است.",
                )
            } else {
                db_error(problem)
            },
        );
    }
    audit(
        &db,
        &actor,
        "app.nginx.link",
        &format!("app={app_id}; site={}", input.site_name),
    );
    Ok(Json(
        json!({"linked":true,"application_id":app_id,"site_name":input.site_name,"domain":domain,"config_mutated":false}),
    ))
}

pub(super) async fn unlink(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<UnlinkInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !input.confirmed {
        return Err(error(StatusCode::BAD_REQUEST, "تأیید قطع لینک لازم است."));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let changed = db.execute(
        "DELETE FROM app_nginx_sites WHERE application_id=?1 AND EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2)",
        params![app_id, owner],
    ).map_err(db_error)?;
    if changed == 0 {
        return Err(error(
            StatusCode::NOT_FOUND,
            "لینک Nginx این اپلیکیشن پیدا نشد.",
        ));
    }
    audit(&db, &actor, "app.nginx.unlink", &format!("app={app_id}"));
    Ok(Json(json!({"unlinked":true,"nginx_untouched":true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn site_names_are_path_safe() {
        assert!(valid_site("example.com"));
        assert!(!valid_site("../sites-enabled/x"));
        assert!(!valid_site(""));
    }

    #[test]
    fn application_nginx_names_and_scaffolds_match_runtime_adapters() {
        let app_id = "12345678-1234-4234-8234-123456789abc";
        assert_eq!(site_name_for(app_id, "laravel"), "arg-app-12345678");
        assert_eq!(site_name_for(app_id, "node"), "arg-node-12345678-123");
        assert_eq!(site_name_for(app_id, "react"), "arg-static-12345678-123");
        let proxy = initial_config(
            app_id,
            "app.example.test",
            "node",
            Some(31234),
            "auto",
            true,
        )
        .unwrap();
        assert!(
            proxy.contains("# Managed by ARG Node runner app=12345678-1234-4234-8234-123456789abc")
        );
        assert!(proxy.contains("proxy_pass http://127.0.0.1:31234;"));
        let placeholder =
            initial_config(app_id, "app.example.test", "custom", None, "auto", true).unwrap();
        assert!(placeholder
            .contains("# ARG managed Application site app=12345678-1234-4234-8234-123456789abc"));
        assert!(placeholder.contains("return 503;"));
        assert!(initial_config(
            app_id,
            "app.example.test; include /tmp/evil",
            "custom",
            None,
            "auto",
            true
        )
        .is_err());
    }

    #[test]
    fn app_domain_rewrite_updates_all_server_blocks_and_can_remove_names() {
        let input = "server {\n    server_name app.example.test;\n}\nserver {\n    listen 443 ssl;\n    server_name app.example.test;\n}\n";
        let domains = vec!["app.example.test".to_owned(), "www.example.test".to_owned()];
        let updated = replace_server_names(input, &domains).unwrap();
        assert_eq!(nginx::nginx_server_names(&updated), domains);
        assert_eq!(
            updated
                .matches("server_name app.example.test www.example.test;")
                .count(),
            2
        );
        assert!(replace_server_names(input, &[])
            .unwrap()
            .contains("server_name _;"));
        assert!(
            replace_server_names("server {\n server_name\n app.example.test;\n}", &domains)
                .is_err()
        );
    }
}
