use super::*;

pub(super) fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/state", get(state_handler))
        .route("/api/setup", post(setup))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/account/password", post(change_password))
        .route("/api/account/username", post(change_username))
        .route("/api/account/sessions", get(list_account_sessions))
        .route(
            "/api/account/sessions/{id}",
            axum::routing::delete(revoke_account_session),
        )
        .route(
            "/api/account/sessions/revoke-others",
            post(revoke_other_account_sessions),
        )
        .route("/api/terminal", get(terminal_socket))
        .route(
            "/api/terminal/root/authorize",
            post(authorize_root_terminal),
        )
        .route("/api/dashboard", get(dashboard))
        .route("/api/storage/volumes", get(storage_volumes))
        .route("/api/maintenance/system", get(maintenance::system_info))
        .route(
            "/api/maintenance/backups",
            get(maintenance::list_backups).post(maintenance::create_backup),
        )
        .route(
            "/api/maintenance/backups/upload",
            post(maintenance::upload_backup),
        )
        .route(
            "/api/maintenance/backups/{id}/restore",
            post(maintenance::restore_backup),
        )
        .route(
            "/api/maintenance/backups/{id}/download",
            get(maintenance::download_backup),
        )
        .route("/api/maintenance/action", post(maintenance::run_action))
        .route("/api/nodes", get(federation::list_nodes).post(federation::add_node))
        .route("/api/nodes/role", put(federation::update_role))
        .route("/api/nodes/public-name", put(federation::update_public_name))
        .route("/api/nodes/pairing-code", post(federation::create_pairing_code))
        .route("/api/nodes/controllers/{id}", axum::routing::delete(federation::revoke_controller_admin).put(federation::rename_controller_admin))
        .route("/api/nodes/{id}/status", get(federation::node_status))
        .route("/api/nodes/{id}/services", get(federation::node_services))
        .route("/api/nodes/{id}/services/action", post(federation::node_service_action))
        .route("/api/nodes/{id}/rotate", post(federation::rotate_node_credential))
        .route("/api/nodes/{workspace}/workspace-terminal", get(federation::proxy_workspace_terminal))
        .route("/api/nodes/{workspace}/proxy/{*path}", axum::routing::any(federation::proxy_workspace_api).layer(axum::extract::DefaultBodyLimit::max(512 * 1024 * 1024)))
        .route("/api/nodes/{id}", axum::routing::delete(federation::remove_node))
        .route("/api/federation/pair", post(federation::accept_pair))
        .route("/api/federation/status", get(federation::status))
        .route("/api/federation/services", get(federation::services))
        .route("/api/federation/services/action", post(federation::service_action))
        .route("/api/federation/controller", axum::routing::delete(federation::revoke_controller))
        .route("/api/federation/controller/rotate", post(federation::rotate_controller_credential))
        .route("/api/federation/workspace/renew", post(federation::renew_workspace_session))
        .route("/api/federation/controller/name", put(federation::sync_controller_public_name))
        .route("/api/s3/providers", get(s3::list_providers))
        .route("/api/s3/summary", get(s3::summary))
        .route(
            "/api/s3/connections",
            get(s3::list_connections).post(s3::create_connection),
        )
        .route(
            "/api/s3/connections/{id}",
            get(s3::get_connection)
                .put(s3::update_connection)
                .delete(s3::delete_connection),
        )
        .route("/api/s3/connections/{id}/test", post(s3::test_connection))
        .route(
            "/api/s3/connections/{id}/buckets",
            get(s3::list_buckets).post(s3::attach_bucket),
        )
        .route(
            "/api/s3/connections/{id}/buckets/create",
            post(s3::create_bucket),
        )
        .route("/api/s3/connections/{id}/objects", get(s3::list_objects))
        .route(
            "/api/s3/connections/{id}/objects/delete",
            post(s3::delete_object),
        )
        .route(
            "/api/s3/connections/{id}/objects/upload",
            post(s3::upload_object).layer(axum::extract::DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        .route(
            "/api/s3/connections/{id}/objects/download",
            get(s3::download_object),
        )
        .route(
            "/api/s3/connections/{id}/transfer/local-to-bucket",
            post(s3::transfer_local_to_bucket),
        )
        .route(
            "/api/s3/connections/{id}/transfer/bucket-to-local",
            post(s3::transfer_bucket_to_local),
        )
        .route(
            "/api/s3/buckets/{id}",
            axum::routing::delete(s3::detach_bucket),
        )
        .route("/api/s3/buckets/{id}/usage", axum::routing::get(s3::bucket_usage).put(s3::set_bucket_quota))
        .route("/api/s3/buckets/{id}/usage/scan", post(s3::scan_bucket_usage))
        .route(
            "/api/s3/buckets/{id}/mount",
            get(s3::get_bucket_mount).put(s3::configure_bucket_mount),
        )
        .route("/api/s3/mounts/{id}/stop", post(s3::stop_bucket_mount))
        .route("/api/history", get(history))
        .route("/api/apps", get(apps::list).post(apps::create))
        .route("/api/apps/{id}", get(apps::get).put(apps::update).delete(apps::delete))
        .route("/api/apps/{id}/restore", post(apps::restore))
        .route("/api/apps/{id}/deploy", post(deploy::create))
        .route("/api/apps/{id}/deploy/archive", post(deploy::upload_archive).layer(axum::extract::DefaultBodyLimit::max(256 * 1024 * 1024)))
        .route("/api/apps/{id}/deploy/jobs", get(deploy::list))
        .route("/api/apps/{id}/deploy/rollback", post(deploy::rollback))
        .route("/api/deploy/jobs/{id}", get(deploy::get))
        .route("/api/deploy/jobs/{id}/events", get(deploy::events))
        .route("/api/deploy/jobs/{id}/cancel", post(deploy::cancel))
        .route("/api/deploy/jobs/{id}/retry", post(deploy::retry))
        .route("/api/fast-actions", get(fast_actions::list).post(fast_actions::create))
        .route("/api/fast-actions/{id}", put(fast_actions::update).delete(fast_actions::delete))
        .route("/api/fast-actions/{id}/run", post(fast_actions::run))
        .route("/api/network/traffic", get(network_traffic_volume))
        .route(
            "/api/monitoring/alerts",
            get(alert_settings).put(update_alert_settings),
        )
        .route(
            "/api/monitoring/config",
            get(get_monitoring_config).put(put_monitoring_config),
        )
        .route(
            "/api/settings/port",
            get(get_panel_port).put(change_panel_port),
        )
        .route(
            "/api/settings/timezone",
            get(get_panel_timezone).put(change_panel_timezone),
        )
        .route(
            "/api/settings/tls",
            get(get_panel_tls_settings).put(update_panel_tls_settings),
        )
        .route(
            "/api/update",
            get(panel_update_status).post(schedule_panel_update),
        )
        .route("/api/update/cancel", post(cancel_panel_update))
        .route("/api/update/rollback", post(rollback_panel_update))
        .route("/api/update/progress", get(panel_update_progress))
        .route("/api/update/logs", get(panel_update_logs))
        .route("/api/packages", get(list_packages))
        .route("/api/packages/runtimes", get(list_runtime_packs))
        .route("/api/packages/action", post(run_apt_operation))
        .route("/api/services", get(list_system_services))
        .route("/api/services/action", post(run_service_operation))
        .route("/api/docker", get(docker::docker_status))
        .route("/api/docker/action", post(docker::docker_operation))
        .route("/api/mysql/status", get(mysql::status))
        .route("/api/mysql/access", get(mysql::access_status).post(mysql::unlock))
        .route("/api/mysql/install", post(mysql::install))
        .route("/api/mysql/service", post(mysql::service_action))
        .route("/api/mysql/catalog", post(mysql::catalog))
        .route("/api/mysql/query", post(mysql::query))
        .route("/api/mysql/metrics", get(mysql::metrics))
        .route("/api/mysql/settings", get(mysql::settings).put(mysql::update_setting))
        .route("/api/mysql/databases", post(mysql::create_database))
        .route("/api/mysql/databases/{name}", axum::routing::delete(mysql::delete_database))
        .route("/api/mysql/users/action", post(mysql::user_action))
        .route("/api/mysql/users/privileges", post(mysql::update_user_privileges))
        .route("/api/mysql/presets", get(mysql::presets).post(mysql::save_preset))
        .route("/api/mysql/presets/{id}", axum::routing::delete(mysql::delete_preset))
        .route("/api/mysql/users/password", post(mysql::reset_password))
        .route("/api/mysql/users", post(mysql::create_user))
        .route("/api/files", get(files::list_files))
        .route("/api/files/disk-scan", post(files::scan_disk))
        .route("/api/files/content", get(files::read_file))
        .route("/api/files/download", get(files::download_file))
        .route(
            "/api/files/action",
            post(files::file_action).layer(axum::extract::DefaultBodyLimit::max(3 * 1024 * 1024)),
        )
        .route("/api/network/probe", post(port_probe))
        .route("/api/users", get(users).post(add_user))
        .route("/api/users/{id}", put(update_user))
        .route("/api/audit", get(audit_log))
        .route(
            "/api/nginx/sites",
            get(nginx::nginx_sites).post(nginx::create_nginx_site),
        )
        .route(
            "/api/nginx/sites/{name}/rename",
            post(nginx::rename_nginx_site),
        )
        .route(
            "/api/nginx/sites/{name}/metadata",
            put(nginx::update_nginx_site_metadata),
        )
        .route(
            "/api/nginx/sites/{name}/actions",
            get(nginx::list_site_actions).post(nginx::create_site_action),
        )
        .route(
            "/api/nginx/sites/{name}/actions/{id}",
            put(nginx::update_site_action).delete(nginx::delete_site_action),
        )
        .route(
            "/api/nginx/sites/{name}/actions/{id}/run",
            post(nginx::run_site_action),
        )
        .route(
            "/api/nginx/sites/{name}/logs",
            post(nginx::api_nginx_site_logs),
        )
        .route(
            "/api/nginx/sites/{name}/traffic",
            get(nginx::api_nginx_site_traffic),
        )
        .route(
            "/api/nginx/sites/{name}/runtime",
            get(nginx::api_nginx_site_runtime).put(nginx::api_nginx_site_runtime),
        )
        .route(
            "/api/nginx/sites/{name}/repository",
            get(nginx::api_nginx_site_repository),
        )
        .route("/api/nginx/sites/{name}/sync", post(nginx::sync_nginx_site))
        .route(
            "/api/nginx/sites/{name}/ssl",
            post(nginx::issue_nginx_site_ssl),
        )
        .route("/api/nginx/domains/check", get(nginx::check_nginx_domains))
        .route("/api/nginx/status", get(nginx::nginx_status))
        .route("/api/nginx/firewall", get(nginx::get_nginx_firewall_setting).put(nginx::update_nginx_firewall_setting))
        .route("/api/nginx/files", get(nginx::api_nginx_browse_files))
        .route("/api/nginx/action", post(nginx::nginx_action))
        .route(
            "/api/nginx/sites/{name}",
            get(nginx::get_nginx_site)
                .put(nginx::save_nginx_site)
                .delete(nginx::delete_nginx_site),
        )
        .route(
            "/api/nginx/sites/{name}/enabled",
            put(nginx::toggle_nginx_site),
        )
        .route(
            "/api/nginx/sites/{name}/maintenance",
            get(nginx::nginx_site_maintenance_status).put(nginx::nginx_site_maintenance),
        )
        .route(
            "/api/nginx/sites/{name}/maintenance/secret",
            post(nginx::nginx_site_maintenance_secret),
        )
        .route("/api/nginx/sites/{name}/permissions/fix", post(nginx::fix_nginx_site_permissions))
        .route("/api/firewall", get(firewall).put(update_firewall_state))
        .route("/api/firewall/install", post(install_firewall))
        .route(
            "/api/firewall/rules",
            get(managed_firewall_rules).post(add_firewall_rule),
        )
        .route(
            "/api/firewall/rules/{id}",
            put(update_firewall_rule).delete(delete_firewall_rule),
        )
        .route("/api/firewall/rules/external", post(delete_external_firewall_rule))
        .fallback(get(static_file))
        .layer(middleware::from_fn_with_state(state.clone(), federation::workspace_session_guard))
        .layer(middleware::from_fn(update_guard))
        .with_state(state)
}
