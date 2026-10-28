use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PurgeRequest {
    expected_version: i64,
    confirmation_name: String,
}

pub(super) async fn purge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<PurgeRequest>,
) -> ApiResult {
    check_origin(&headers)?;
    if !apps::valid_uuid(&id) {
        return Err(error(StatusCode::BAD_REQUEST, "شناسهٔ اپلیکیشن معتبر نیست."));
    }

    let _operation_guard = apps::APP_REGISTRY_OPERATION_LOCK.lock().await;
    let _execution_guard = laravel::APP_EXECUTION_LOCK.lock().await;
    let (actor, owner, name, source_type, job_ids) = {
        let db = state.lock().unwrap();
        let actor = require_admin(&db, &headers)?.username;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let app: Option<(String, i64, String)> = db
            .query_row(
                "SELECT name,version,source_type FROM applications WHERE id=?1 AND owner_server_uuid=?2",
                params![id, owner],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(db_error)?;
        let Some((name, version, source_type)) = app else {
            return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."));
        };
        if version != input.expected_version {
            return Err(error(
                StatusCode::CONFLICT,
                "اطلاعات اپلیکیشن تغییر کرده است؛ صفحه را تازه کن و دوباره بررسی کن.",
            ));
        }
        if input.confirmation_name.trim() != name {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "برای تأیید حذف کامل، نام اپلیکیشن را دقیق وارد کن.",
            ));
        }
        let deployment_running: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM deployment_jobs WHERE application_id=?1 AND state IN ('queued','validating','running','verifying'))",
                [&id],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        if deployment_running {
            return Err(error(
                StatusCode::CONFLICT,
                "یک استقرار در حال اجراست؛ ابتدا صبر کن یا آن را لغو کن.",
            ));
        }
        let laravel_running: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM laravel_operations WHERE application_id=?1 AND state IN ('queued','running'))",
                [&id],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        if laravel_running {
            return Err(error(
                StatusCode::CONFLICT,
                "یک عملیات Laravel در حال اجراست؛ پس از پایان آن دوباره حذف را انجام بده.",
            ));
        }
        let mut statement = db
            .prepare("SELECT id FROM deployment_jobs WHERE application_id=?1")
            .map_err(db_error)?;
        let job_ids = statement
            .query_map([&id], |row| row.get::<_, String>(0))
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;

        // Check restrictive foreign keys before the helper starts deleting files or services.
        let tx = db.unchecked_transaction().map_err(db_error)?;
        tx.execute(
            "DELETE FROM deployment_releases WHERE application_id=?1",
            [&id],
        )
        .map_err(|_| {
            error(
                StatusCode::CONFLICT,
                "releaseهای اپلیکیشن به‌صورت امن قابل حذف نیستند؛ چیزی از سیستم پاک نشد.",
            )
        })?;
        tx.execute("DELETE FROM deployment_jobs WHERE application_id=?1", [&id])
            .map_err(|_| {
                error(
                    StatusCode::CONFLICT,
                    "سوابق استقرار به‌صورت امن قابل حذف نیستند؛ چیزی از سیستم پاک نشد.",
                )
            })?;
        tx.execute(
            "DELETE FROM applications WHERE id=?1 AND owner_server_uuid=?2",
            params![id, owner],
        )
        .map_err(|_| {
            error(
                StatusCode::CONFLICT,
                "داده‌های وابسته اجازهٔ حذف کامل نمی‌دهند؛ چیزی از سیستم پاک نشد.",
            )
        })?;
        tx.rollback().map_err(db_error)?;
        (actor, owner, name, source_type, job_ids)
    };

    let helper = privileged_request_async(json!({
        "action":"application-purge",
        "application_id":id,
        "source_type":source_type,
        "job_ids":job_ids,
    }))
    .await?;
    if helper["ok"] != true {
        let detail = helper["error"]
            .as_str()
            .unwrap_or("پاک‌سازی منابع تحت مدیریت ARG کامل نشد.");
        return Err(error(StatusCode::BAD_GATEWAY, detail));
    }

    let removed_sites = helper["data"]["nginx_sites"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let removed_containers = helper["data"]["docker_containers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    if federation::local_server_uuid(&db).map_err(db_error)? != owner {
        return Err(error(
            StatusCode::CONFLICT,
            "هویت VPS هنگام حذف اپلیکیشن تغییر کرد؛ رکورد برای بررسی باقی ماند.",
        ));
    }
    let tx = db.transaction().map_err(db_error)?;
    // These older deployment tables use restrictive foreign keys instead of cascade.
    tx.execute(
        "DELETE FROM deployment_releases WHERE application_id=?1",
        [&id],
    )
    .map_err(db_error)?;
    tx.execute("DELETE FROM deployment_jobs WHERE application_id=?1", [&id])
        .map_err(db_error)?;
    let deleted = tx
        .execute(
            "DELETE FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND version=?3",
            params![id, owner, input.expected_version],
        )
        .map_err(db_error)?;
    if deleted != 1 {
        return Err(error(
            StatusCode::CONFLICT,
            "رکورد در میانهٔ عملیات تغییر کرد؛ منابع پاک‌سازی شدند اما رکورد برای بررسی باقی ماند.",
        ));
    }
    for site in &removed_sites {
        if let Some(site_name) = site.as_str() {
            tx.execute("DELETE FROM nginx_site_metadata WHERE name=?1", [site_name])
                .map_err(db_error)?;
            tx.execute(
                "DELETE FROM nginx_site_config_state WHERE name=?1",
                [site_name],
            )
            .map_err(db_error)?;
            tx.execute(
                "DELETE FROM nginx_site_runtime WHERE site_name=?1",
                [site_name],
            )
            .map_err(db_error)?;
            tx.execute(
                "DELETE FROM nginx_site_actions WHERE site_name=?1",
                [site_name],
            )
            .map_err(db_error)?;
        }
    }
    audit(
        &tx,
        &actor,
        "application.purge",
        &format!(
            "id={id}; name={name}; nginx_sites={}; docker_containers={}; managed_deployment_tree=true",
            removed_sites.len(),
            removed_containers.len()
        ),
    );
    tx.commit().map_err(db_error)?;
    Ok(Json(json!({
        "deleted":true,
        "application_id":id,
        "removed":{
            "deployment_tree_and_installed_project_dependencies":helper["data"]["deployment_tree_removed"] == true,
            "application_services":true,
            "deployment_archives":helper["data"]["uploaded_archives_removed"].clone(),
            "arg_managed_nginx_sites":removed_sites,
            "arg_labeled_docker_containers":removed_containers,
            "database_records":true
        },
        "preserved":[
            "runtime packages shared by the VPS",
            "S3 buckets and remote objects",
            "Docker images and named volumes",
            "external databases not recorded as ARG-provisioned for this app",
            "Nginx sites not created by ARG for this application",
            "unlabeled or externally managed containers"
        ]
    })))
}

pub(super) fn execute_privileged_request(request: &Value) -> Value {
    let Some(app_id) = request["application_id"]
        .as_str()
        .filter(|id| apps::valid_uuid(id))
    else {
        return json!({"ok":false,"error":"شناسهٔ اپلیکیشن برای حذف معتبر نیست."});
    };
    let source_type = request["source_type"].as_str().unwrap_or_default();
    let job_ids = request["job_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if let Err(problem) = deploy::preflight_application_purge(app_id) {
        return json!({"ok":false,"error":problem});
    }
    if source_type == "docker" {
        if let Err(problem) = docker::preflight_application_containers(app_id) {
            return json!({"ok":false,"error":problem});
        }
    }
    let docker_containers = if source_type == "docker" {
        match docker::purge_application_containers(app_id) {
            Ok(containers) => containers,
            Err(problem) => {
                return json!({"ok":false,"error":format!("کانتینرهای برچسب‌خوردهٔ اپ حذف نشدند: {problem}")})
            }
        }
    } else {
        Vec::new()
    };
    if let Err(problem) = node::purge_application_service(app_id) {
        return json!({"ok":false,"error":format!("سرویس Node پاک نشد: {problem}")});
    }
    if let Err(problem) = laravel::purge_application_services_and_pool(app_id) {
        return json!({"ok":false,"error":format!("سرویس‌ها یا pool مربوط به Laravel پاک نشدند: {problem}")});
    }
    let nginx_sites = match nginx::purge_managed_application_sites(app_id) {
        Ok(sites) => sites,
        Err(problem) => {
            return json!({"ok":false,"error":format!("تنظیم Nginx تحت مدیریت ARG حذف نشد: {problem}")})
        }
    };
    let uploaded_archives_removed = match deploy::purge_application_uploads(&job_ids) {
        Ok(removed) => removed,
        Err(problem) => {
            return json!({"ok":false,"error":format!("ZIPهای مربوط به استقرار پاک نشدند: {problem}")})
        }
    };
    let tree_removed = match deploy::purge_application_tree(app_id) {
        Ok(removed) => removed,
        Err(problem) => {
            return json!({"ok":false,"error":format!("سورس یا وابستگی‌های نصب‌شده پاک نشدند: {problem}")})
        }
    };
    json!({"ok":true,"data":{"nginx_sites":nginx_sites,"docker_containers":docker_containers,"deployment_tree_removed":tree_removed,"uploaded_archives_removed":uploaded_archives_removed}})
}
