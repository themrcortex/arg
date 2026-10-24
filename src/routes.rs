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
        .route("/api/dashboard", get(dashboard))
        .route("/api/history", get(history))
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
            "/api/nginx/sites/{name}/logs",
            post(nginx::api_nginx_site_logs),
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
