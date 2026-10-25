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
            "/api/s3/buckets/{id}",
            axum::routing::delete(s3::detach_bucket),
        )
        .route("/api/history", get(history))
        .route("/api/fast-actions", get(fast_actions::list).post(fast_actions::create))
        .route("/api/fast-actions/{id}", put(fast_actions::update).delete(fast_actions::delete))
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
            "/api/update",
            get(panel_update_status).post(schedule_panel_update),
        )
        .route("/api/update/progress", get(panel_update_progress))
        .route("/api/update/logs", get(panel_update_logs))
        .route("/api/packages", get(list_packages))
        .route("/api/packages/action", post(run_apt_operation))
        .route("/api/services", get(list_system_services))
        .route("/api/services/action", post(run_service_operation))
        .route("/api/docker", get(docker::docker_status))
        .route("/api/docker/action", post(docker::docker_operation))
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
        .fallback(get(static_file))
        .layer(middleware::from_fn(update_guard))
        .with_state(state)
}
