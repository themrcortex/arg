# 02 — Authentication, authorization, and HTTP security audit

## Audit scope and result

- Inspected revision: `2ee2f4aac042702e2cfda63e92ca7ba40f71aac7` on `audit/development-hardening`, based on development SHA `eec273cde037fa57edc1b5ab042fda7acc26a32c`.
- No production host, user account, or live database was contacted. API checks were source-based and regression tests use in-memory SQLite / local test state.
- The Axum router has 229 registered path templates and 297 method-handler registrations. The matrix below lists every method-handler pair discovered directly from `src/routes.rs`; it is a source inventory, not a claim that a live deployment was externally penetration-tested.
- All API responses receive `Cache-Control: no-store, private, max-age=0` from router middleware. `workspace_session_guard` applies only to recognized Federation workspace sessions and enforces session expiry plus method/path scope; ordinary local sessions still pass handler checks.

## Authentication controls reviewed

| Control | Observed behavior | Evidence / test |
|---|---|---|
| Password verification | Argon2 verify; setup enforces 12–1024 byte password and login rejects oversized credentials. | `src/main.rs` `setup`, `login`; login regression tests below. |
| Session token | Random token is issued after successful login; only its hash is stored; expiry is clamped to 1–72 hours; session check requires an enabled user and refreshes `last_seen` at most once/minute. | `require_user`, `login`. |
| Session revocation | Logout deletes current session; password change deletes other sessions; root password reset deletes all sessions; terminal WebSocket checks admin session every five seconds and stops shell/helper process when invalid. | `logout`, `change_password`, root reset, `run_terminal`. |
| Session fixation | Login creates a fresh random token; no client-provided value becomes the session. | `login`. |
| Root terminal | Admin session, fresh password check, per-admin 8/10-minute failed-attempt throttle, hashed one-time ticket bound to user and login session, TTL 60 seconds. | `authorize_root_terminal`, `consume_root_terminal_ticket`, `terminal_socket`. |
| Login throttling | 8 failures per username/client IP plus 20 failures per client IP per 10 minutes. `X-Forwarded-For` is considered only for loopback TCP peers; public peer supplied XFF is ignored. | `login_throttle_keys`, `login_client_ip`; tests cover per-IP isolation, proxy trust boundary, legacy lockout. |
| API keys | High-entropy `arg_live_` token; only hash stored; one-time display; scopes limited to `apps:read` and `deployments:read`; HTTPS, 60 req/min, 1–365 day expiry, max 10 active/admin. | `src/api_keys.rs`; unit tests cover scope, expiry, revocation, rate limit. |
| Current API-key owner authority | Key authorization requires owner still `role=admin AND enabled=1`; demotion/disable immediately revokes; re-promotion does not reactivate. | `dbce1df`; `api_keys::revoke_if_owner_ineligible`; regression test. |
| CSRF / Origin | `check_origin` requires exact scheme + Host for cookie-authenticated writes. TLS marker is set only on TLS listener and stripped on plain HTTP listener. | `check_origin`, `secure_transport`, `clear_trusted_transport_marker`. |
| TLS boundary | With TLS configured, plain HTTP listener is loopback-only; trusted TLS listener sets private transport marker; forwarded protocol is trusted only for loopback peers. | `http_listener_addr`, server construction; transport tests. |
| WebSocket | Terminal requires matching Origin authority/Host/scheme; admin session required. Root mode additionally consumes one-time ticket. | `terminal_socket`. |
| API caching | `no_store` covers all `/api` responses, but not static assets. | `d10c425`; `cache_policy_covers_api_routes_without_changing_static_assets`. |
| CORS | No permissive CORS layer or wildcard allow-origin policy found in the Axum router. | `src/routes.rs`; no CORS middleware/dependency. |
| Error responses | Handlers return status + JSON error envelope; no global stack trace response layer was found. `unwrap_or` fallback uses remain and are not assumed safe/unsafe without contextual review. | Source inspection; dynamic error-injection remains incomplete. |

## Findings and remediations

### AUTH-001 — API keys survived owner privilege loss

- **Severity / status / confidence:** P1 / CONFIRMED / high.
- **Baseline evidence:** `api_keys::authorize` verified key hash, revocation, expiry, and scope but did not join the owner user; a key could retain access after owner demotion or disablement.
- **Fix:** authorization now requires current owner `role=admin AND enabled=1`; the user-update path revokes active keys in the same account update flow.
- **Regression test:** `api_keys_are_revoked_when_owner_loses_administrator_access` covers demotion, disablement, and no reactivation.
- **Commit:** `dbce1df`; final report revision `2ee2f4a`.
- **Remaining risk:** public API remains read-only; leaked keys still require explicit revocation or expiry.

### AUTH-002 — Authenticated API responses lacked a uniform no-store policy

- **Severity / status / confidence:** P2 / CONFIRMED / high.
- **Evidence:** only selected routes previously set no-store headers; other authenticated APIs did not explicitly prevent intermediary/browser storage.
- **Fix:** router-level middleware marks all `/api` responses private and non-cacheable; static assets remain cacheable.
- **Regression test:** `cache_policy_covers_api_routes_without_changing_static_assets`.
- **Commit:** `d10c425`.
- **Remaining risk:** custom reverse proxies should honor `Cache-Control`; their configuration was not externally inspected.

### AUTH-003 — Shared username throttle enabled account-wide lockout

- **Severity / status / confidence:** P2 / CONFIRMED / high.
- **Evidence:** baseline stored failures under a single username key, so an unauthenticated client could exhaust that key and block the administrator from another IP.
- **Fix:** username threshold is isolated per client IP, with a separate per-IP cap. Public peers cannot choose the IP through XFF.
- **Regression tests:** `login_throttles_are_scoped_to_the_client_ip_and_only_trust_forwarded_ip_from_loopback`; `legacy_account_wide_login_lockout_does_not_block_another_client`.
- **Commit:** `c310ce8`.
- **Remaining risk:** distributed attempts can still consume password verification resources; old throttle rows are not periodically pruned and are tracked for database/performance phases.

### AUTH-004 — App-volume scan POST omitted Origin validation

- **Severity / status / confidence:** P2 / CONFIRMED / high.
- **Evidence:** `/api/app-volumes/{id}/scan` is a bodyless POST which invokes the privileged volume scanner after cookie admin auth but did not originally check Origin. SameSite=Strict does not block a same-site cross-origin request from a sibling subdomain.
- **Fix:** reject absent or mismatched Origin before DB lookup/helper invocation; admin and application ownership checks remain.
- **Regression test:** `volume_scan_rejects_missing_and_cross_origin_requests` expects 403 for absent/hostile Origin and confirms same-origin continues to auth (401 without session).
- **Commit:** `2ee2f4a`.
- **Remaining risk:** scan is bounded to 100,000 entries; a valid admin may still request repeated scans. Scan rate limiting is tracked in phase 10.

### Verified controls / non-findings

- `/api/state` exposes installation state before setup and only includes user data after session validation. `/api/setup` is protected by bootstrap token and loopback/tunnel policy.
- Public OpenAPI contains endpoint metadata only; public API data routes require bearer scopes and HTTPS.
- No permissive CORS configuration was found.
- Federation workspace middleware recognizes only recorded proxy session hashes and checks expiry/scope; local handler authorization remains in force.
- Terminal WebSocket revalidates the local admin session every five seconds and stops the child/helper process when the session ends.

## API authorization matrix

Labels describe the guard in the handler or named delegated helper. All routes also pass through the global workspace-session guard when the presented cookie is a Federation workspace session. The matrix records browser Origin requirements separately from credentials.

| Method | Route | Handler | Authorization | Origin / transport |
|---|---|---|---|---|
| `GET` | `/api/state` | `state_handler` | Public install state; optional authenticated user summary | No state change |
| `POST` | `/api/setup` | `setup` | Bootstrap token + loopback eligibility | Exact Origin required; setup token required for initial admin |
| `POST` | `/api/login` | `login` | Public login; password verification before session creation | Exact Origin; HTTPS off-loopback; 8 per-user/IP + 20 per-IP / 10 min |
| `POST` | `/api/logout` | `logout` | Authenticated user session | Same-origin Origin required before write |
| `POST` | `/api/account/password` | `change_password` | Authenticated user session | Same-origin Origin required before write |
| `POST` | `/api/account/username` | `change_username` | Authenticated user session | Same-origin Origin required before write |
| `GET` | `/api/account/sessions` | `list_account_sessions` | Authenticated user session | Read route; Origin not required |
| `DELETE` | `/api/account/sessions/{id}` | `revoke_account_session` | Authenticated user session | Same-origin Origin required before write |
| `POST` | `/api/account/sessions/revoke-others` | `revoke_other_account_sessions` | Authenticated user session | Same-origin Origin required before write |
| `GET` | `/api/account/capabilities` | `account_capabilities` | Authenticated user session | Read route; Origin not required |
| `GET` | `/api/account/dashboard-preferences` | `dashboard_preferences::read` | Authenticated user session | Read route; Origin not required |
| `PUT` | `/api/account/dashboard-preferences` | `dashboard_preferences::save` | Authenticated user session | Same-origin Origin required before write |
| `GET` | `/api/terminal` | `terminal_socket` | Admin session; root mode also requires fresh-password, session-bound one-use root ticket | WebSocket Origin authority and scheme must match Host |
| `POST` | `/api/terminal/root/authorize` | `authorize_root_terminal` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/dashboard` | `dashboard` | Authenticated user session | Read route; Origin not required |
| `GET` | `/api/storage/volumes` | `storage_volumes` | Authenticated user session | Read route; Origin not required |
| `GET` | `/api/maintenance/system` | `maintenance::system_info` | Admin session | Read route; Origin not required |
| `GET` | `/api/maintenance/doctor` | `doctor::read` | Admin session | Read route; Origin not required |
| `GET` | `/api/ssh-keys` | `ssh_keys::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/ssh-keys` | `ssh_keys::create` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/ssh-keys/trusted-hosts/github` | `ssh_keys::host_keys` | Admin session | Read route; Origin not required |
| `POST` | `/api/ssh-keys/trusted-hosts/github` | `ssh_keys::refresh_host_keys` | Admin session | Same-origin Origin required before write |
| `PUT` | `/api/ssh-keys/{id}` | `ssh_keys::rename` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/ssh-keys/{id}` | `ssh_keys::delete` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/admin/api-keys` | `api_keys::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/admin/api-keys` | `api_keys::create` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/admin/api-keys/{id}` | `api_keys::revoke` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/v1/openapi.json` | `api_keys::openapi` | Public API schema metadata only | No credential or user data |
| `GET` | `/api/v1/apps` | `api_keys::apps_read` | Bearer API key + `apps:read`; current key owner must remain enabled admin | HTTPS; read-only; 60 req/min |
| `GET` | `/api/v1/deployments` | `api_keys::deployments_read` | Bearer API key + `deployments:read`; current key owner must remain enabled admin | HTTPS; read-only; 60 req/min |
| `GET` | `/api/security/posture` | `security_posture::inventory` | Admin session | Read route; Origin not required |
| `GET` | `/api/extensions/trust-keys` | `extensions::list_trust_keys` | Admin session | Read route; Origin not required |
| `POST` | `/api/extensions/trust-keys` | `extensions::add_trust_key` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/extensions/trust-keys/{id}` | `extensions::revoke_trust_key` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/extensions/validate` | `extensions::validate_manifest` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/maintenance/backups` | `maintenance::list_backups` | Admin session | Read route; Origin not required |
| `POST` | `/api/maintenance/backups` | `maintenance::create_backup` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/maintenance/backup-policy` | `maintenance::get_backup_policy` | Admin session | Read route; Origin not required |
| `PUT` | `/api/maintenance/backup-policy` | `maintenance::save_backup_policy` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/maintenance/backups/upload` | `maintenance::upload_backup` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/maintenance/backups/{id}/restore` | `maintenance::restore_backup` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/maintenance/backups/{id}/download` | `maintenance::download_backup` | Admin session | Read route; Origin not required |
| `POST` | `/api/maintenance/action` | `maintenance::run_action` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/nodes` | `federation::list_nodes` | Local admin session; paired Node scope/capability checked before remote operation where applicable | Read route; Origin not required |
| `POST` | `/api/nodes` | `federation::add_node` | Local admin session; paired Node scope/capability checked before remote operation where applicable | Same-origin required for write |
| `PUT` | `/api/nodes/role` | `federation::update_role` | Local admin session; paired Node scope/capability checked before remote operation where applicable | Same-origin required for write |
| `PUT` | `/api/nodes/public-name` | `federation::update_public_name` | Local admin session; outbound sync only to peers with `server.public_name.sync` | Same-origin required for write |
| `POST` | `/api/nodes/pairing-code` | `federation::create_pairing_code` | Local admin session; paired Node scope/capability checked before remote operation where applicable | Same-origin required for write |
| `DELETE` | `/api/nodes/controllers/{id}` | `federation::revoke_controller_admin` | Local admin session; paired Node scope/capability checked before remote operation where applicable | Same-origin required for write |
| `PUT` | `/api/nodes/controllers/{id}` | `federation::rename_controller_admin` | Local admin session; paired Node scope/capability checked before remote operation where applicable | Same-origin required for write |
| `GET` | `/api/nodes/{id}/status` | `federation::node_status` | Local admin session; `node.status.read` on paired Node | Read route; Origin not required |
| `GET` | `/api/nodes/{id}/update` | `federation::node_update_check` | Local admin session; `node.services.manage` or compatible workspace scopes | Read route; Origin not required |
| `POST` | `/api/nodes/{id}/update` | `federation::node_update_run` | Local admin session; `node.services.manage` or compatible workspace scopes | Same-origin required for write |
| `GET` | `/api/nodes/{id}/update/progress` | `federation::node_update_progress` | Local admin session; `node.services.manage` or compatible workspace scopes | Read route; Origin not required |
| `GET` | `/api/nodes/{id}/update/logs` | `federation::node_update_logs` | Local admin session; `node.services.manage` or compatible workspace scopes | Read route; Origin not required |
| `POST` | `/api/nodes/{id}/update/cancel` | `federation::node_update_cancel` | Local admin session; `node.services.manage` or compatible workspace scopes | Same-origin required for write |
| `POST` | `/api/nodes/{id}/update/rollback` | `federation::node_update_rollback` | Local admin session; `node.services.manage` or compatible workspace scopes | Same-origin required for write |
| `GET` | `/api/nodes/{id}/services` | `federation::node_services` | Local admin session; `node.services.read` on paired Node | Read route; Origin not required |
| `POST` | `/api/nodes/{id}/services/action` | `federation::node_service_action` | Local admin session; `node.services.manage` on paired Node | Same-origin required for write |
| `POST` | `/api/nodes/{id}/rotate` | `federation::rotate_node_credential` | Local admin session; paired Node credential rotation scope | Same-origin required for write |
| `GET` | `/api/nodes/{workspace}/workspace-terminal` | `federation::proxy_workspace_terminal` | Local admin session + Node pairing, capability and expiring workspace session; `workspace.read` for reads and `workspace.write` for writes; remote root mode requires remote root-terminal authorization | Exact Origin on writes / matching WebSocket Origin; HTTPS-only endpoint client; redirects disabled |
| `ANY` | `/api/nodes/{workspace}/proxy/{*path}` | `federation::proxy_workspace_api` | Local admin session + Node pairing, capability and expiring workspace session; `workspace.read` for reads and `workspace.write` for writes | Exact Origin on writes / matching WebSocket Origin; HTTPS-only endpoint client; redirects disabled |
| `DELETE` | `/api/nodes/{id}` | `federation::remove_node` | Local admin session; paired Node scope/capability checked before remote operation where applicable | Same-origin required for write |
| `POST` | `/api/federation/pair` | `federation::accept_pair` | One-time pairing code issued by enabled admin; approved scopes attached to pairing | Trusted TLS transport; code expires in 5 minutes and is consumed once |
| `GET` | `/api/federation/status` | `federation::status` | Federation bearer + `node.status.read` | Bearer authentication; no browser cookie authority |
| `GET` | `/api/federation/update` | `federation::remote_update_check` | Federation bearer + `node.services.manage` | Bearer authentication; no browser cookie authority |
| `POST` | `/api/federation/update` | `federation::remote_update_run` | Federation bearer + `node.services.manage` | Bearer authentication; no browser cookie authority |
| `GET` | `/api/federation/update/progress` | `federation::remote_update_progress` | Federation bearer + `node.services.manage` | Bearer authentication; no browser cookie authority |
| `GET` | `/api/federation/update/logs` | `federation::remote_update_logs` | Federation bearer + `node.services.manage` | Bearer authentication; no browser cookie authority |
| `POST` | `/api/federation/update/cancel` | `federation::remote_update_cancel` | Federation bearer + `node.services.manage` | Bearer authentication; no browser cookie authority |
| `POST` | `/api/federation/update/rollback` | `federation::remote_update_rollback` | Federation bearer + `node.services.manage` | Bearer authentication; no browser cookie authority |
| `GET` | `/api/federation/services` | `federation::services` | Federation bearer + `node.services.read` | Bearer authentication; no browser cookie authority |
| `POST` | `/api/federation/services/action` | `federation::service_action` | Federation bearer + `node.services.manage` | Bearer authentication; no browser cookie authority |
| `DELETE` | `/api/federation/controller` | `federation::revoke_controller` | Federation bearer + `controller.self_revoke` scope | Self-revocation only |
| `POST` | `/api/federation/controller/rotate` | `federation::rotate_controller_credential` | Federation bearer + `server.credential.rotate` scope | Trusted TLS transport; credential material excluded from audit |
| `POST` | `/api/federation/workspace/renew` | `federation::renew_workspace_session` | Federation bearer + `workspace.session.renew` | Bearer authentication; no browser cookie authority |
| `PUT` | `/api/federation/controller/name` | `federation::sync_controller_public_name` | Federation bearer + `server.public_name.sync` scope | Bearer credential; no cookie authority |
| `GET` | `/api/s3/providers` | `s3::list_providers` | Admin session | Read route; Origin not required |
| `GET` | `/api/s3/summary` | `s3::summary` | Authenticated user session | Read route; Origin not required |
| `GET` | `/api/s3/buckets/catalog` | `s3::bucket_catalog` | Admin session | Read route; Origin not required |
| `GET` | `/api/s3/connections` | `s3::list_connections` | Admin session | Read route; Origin not required |
| `POST` | `/api/s3/connections` | `s3::create_connection` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/s3/connections/{id}` | `s3::get_connection` | Admin session | Read route; Origin not required |
| `PUT` | `/api/s3/connections/{id}` | `s3::update_connection` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/s3/connections/{id}` | `s3::delete_connection` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/s3/connections/{id}/test` | `s3::test_connection` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/s3/connections/{id}/buckets` | `s3::list_buckets` | Admin session | Read route; Origin not required |
| `POST` | `/api/s3/connections/{id}/buckets` | `s3::attach_bucket` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/s3/connections/{id}/buckets/create` | `s3::create_bucket` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/s3/connections/{id}/objects` | `s3::list_objects` | Admin session | Read route; Origin not required |
| `POST` | `/api/s3/connections/{id}/objects/delete` | `s3::delete_object` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/s3/connections/{id}/objects/upload` | `s3::upload_object` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/s3/connections/{id}/objects/download` | `s3::download_object` | Admin session | Read route; Origin not required |
| `POST` | `/api/s3/connections/{id}/transfer/local-to-bucket` | `s3::transfer_local_to_bucket` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/s3/connections/{id}/transfer/bucket-to-local` | `s3::transfer_bucket_to_local` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/s3/buckets/{id}` | `s3::detach_bucket` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/s3/buckets/{id}/usage` | `s3::bucket_usage` | Authenticated user session | Read route; Origin not required |
| `PUT` | `/api/s3/buckets/{id}/usage` | `s3::set_bucket_quota` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/s3/buckets/{id}/usage/scan` | `s3::scan_bucket_usage` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/s3/buckets/{id}/mount` | `s3::get_bucket_mount` | Admin session | Read route; Origin not required |
| `PUT` | `/api/s3/buckets/{id}/mount` | `s3::configure_bucket_mount` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/s3/mounts/{id}/stop` | `s3::stop_bucket_mount` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/history` | `history` | Authenticated user session | Read route; Origin not required |
| `GET` | `/api/apps` | `apps::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/apps` | `apps::create` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/app-platform/workspaces` | `app_workspaces::list_workspaces` | Admin session | Read route; Origin not required |
| `POST` | `/api/app-platform/workspaces` | `app_workspaces::create_workspace` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/app-templates` | `app_templates::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/app-templates/{id}/draft` | `app_templates::create_draft` | Delegates to `apps::create`; admin session and app input checks there | Same-origin Origin enforced by delegated creation handler |
| `GET` | `/api/app-platform/workspaces/{id}/projects` | `app_workspaces::list_projects` | Admin session | Read route; Origin not required |
| `POST` | `/api/app-platform/workspaces/{id}/projects` | `app_workspaces::create_project` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/app-platform/projects/{id}/environments` | `app_workspaces::list_environments` | Admin session | Read route; Origin not required |
| `POST` | `/api/app-platform/projects/{id}/environments` | `app_workspaces::create_environment` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/build-plans` | `buildpacks::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/apps/{id}/build-plans` | `buildpacks::analyze` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/source/tree` | `app_code::tree` | Admin session through `app_release`; active release and app-owner scope checked | Read-only; sensitive/hidden paths filtered; canonical path confined to release root |
| `GET` | `/api/apps/{id}/source/file` | `app_code::file` | Admin session through `app_release`; active release and app-owner scope checked | Read-only; sensitive/hidden paths filtered; canonical path confined to release root |
| `GET` | `/api/apps/{id}/snapshots` | `app_snapshots::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/apps/{id}/snapshots` | `app_snapshots::create` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/workflows` | `workflow_drafts::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/apps/{id}/workflows` | `workflow_drafts::create` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/apps/{id}/workflows/{workflow_id}` | `workflow_drafts::delete` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/containers` | `app_compose::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/apps/{id}/containers` | `app_compose::link` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/apps/{id}/containers/{container_id}` | `app_compose::unlink` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/nginx-site` | `nginx_domains::get` | Admin session | Read route; Origin not required |
| `PUT` | `/api/apps/{id}/nginx-site` | `nginx_domains::link` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/apps/{id}/nginx-site` | `nginx_domains::unlink` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/backups` | `app_backups::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/apps/{id}/backups` | `app_backups::create` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/apps/{id}/backups/{backup_id}` | `app_backups::delete` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/backups/{backup_id}/download` | `app_backups::download` | Admin session | Read route; Origin not required |
| `GET` | `/api/postgres/status` | `postgres::status` | Admin session | Read route; Origin not required |
| `PUT` | `/api/nodes/{id}/tags` | `fleet::set_tags` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/fleet/groups` | `fleet::list_groups` | Admin session | Read route; Origin not required |
| `GET` | `/api/fleet/rollouts` | `fleet::list_rollouts` | Admin session | Read route; Origin not required |
| `POST` | `/api/fleet/rollouts/preview` | `fleet::preview_rollout` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/private-network/policies` | `private_networks::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/private-network/policies` | `private_networks::create` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/private-network/policies/{id}` | `private_networks::delete` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/dns/connections` | `dns::list_connections` | Admin session | Read route; Origin not required |
| `POST` | `/api/dns/connections` | `dns::connect` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/dns/connections/{id}` | `dns::disconnect` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/dns/connections/{id}/records` | `dns::list_records` | Admin session via `connection_token`; owner-server scoped DNS connection | Provider read request; no mutation |
| `POST` | `/api/dns/connections/{id}/records` | `dns::create_record` | Admin session | Same-origin Origin required before write |
| `PUT` | `/api/dns/connections/{id}/records/{record_id}` | `dns::update_record` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/dns/connections/{id}/records/{record_id}` | `dns::delete_record` | Admin session | Same-origin Origin required before write |
| `PUT` | `/api/apps/{id}/environment` | `app_workspaces::assign_environment` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/app-networks` | `app_networks::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/app-networks` | `app_networks::create` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/app-networks/{id}/members` | `app_networks::list_members` | Admin session | Read route; Origin not required |
| `POST` | `/api/app-networks/{id}/members` | `app_networks::attach` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/app-networks/{id}/members/{app_id}` | `app_networks::detach` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/app-networks/{id}` | `app_networks::remove` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/volumes` | `app_storage::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/apps/{id}/volumes` | `app_storage::create` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/s3-bindings` | `app_s3::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/apps/{id}/s3-bindings` | `app_s3::create` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/app-s3-bindings/{id}` | `app_s3::remove` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/app-volumes/{id}/scan` | `app_storage::scan` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/app-volumes/{id}` | `app_storage::detach` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/app-volumes/{id}/purge` | `app_storage::purge` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}` | `apps::get` | Admin session | Read route; Origin not required |
| `PUT` | `/api/apps/{id}` | `apps::update` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/apps/{id}` | `apps::delete` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/apps/{id}/restore` | `apps::restore` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/apps/{id}/purge` | `app_cleanup::purge` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/laravel` | `laravel::inspect` | Admin session | Read route; Origin not required |
| `GET` | `/api/apps/{id}/laravel/operations` | `laravel::operations` | Admin session | Read route; Origin not required |
| `POST` | `/api/apps/{id}/laravel/operations` | `laravel::start_operation` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/laravel/web` | `laravel::web_status` | Admin session | Read route; Origin not required |
| `POST` | `/api/apps/{id}/laravel/web` | `laravel::configure_web` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/apps/{id}/laravel/services` | `laravel::configure_service` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/apps/{id}/laravel/database` | `laravel::provision_database` | Admin session | Same-origin Origin required before write |
| `PUT` | `/api/apps/{id}/laravel/environment` | `laravel::save_environment` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/apps/{id}/laravel/environment` | `laravel::generate_key` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/node` | `node::inspect` | Admin session | Read route; Origin not required |
| `GET` | `/api/apps/{id}/node/runtime-settings` | `node::get_runtime_settings` | Admin session | Read route; Origin not required |
| `PUT` | `/api/apps/{id}/node/runtime-settings` | `node::save_runtime_settings` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/node/service` | `node::service` | Admin session | Read route; Origin not required |
| `PUT` | `/api/apps/{id}/node/service` | `node::set_service` | Admin session | Same-origin Origin required before write |
| `PUT` | `/api/apps/{id}/node/environment` | `node::save_environment` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/node/build-environment` | `node::get_build_environment` | Admin session | Read route; Origin not required |
| `PUT` | `/api/apps/{id}/node/build-environment` | `node::save_build_environment` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/node/service/logs` | `node::logs` | Admin session | Read route; Origin not required |
| `GET` | `/api/apps/{id}/monitoring` | `app_monitoring::read` | Admin session | Read route; Origin not required |
| `PUT` | `/api/apps/{id}/monitoring/rules` | `app_monitoring::save_rules` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/apps/{id}/monitoring/events/{event_id}/ack` | `app_monitoring::acknowledge_event` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/apps/{id}/deploy` | `deploy::create` | Delegates to `insert_job`; admin session and application ownership checked there | Same-origin Origin + validated Idempotency-Key in `insert_job` |
| `POST` | `/api/apps/{id}/deploy/archive` | `deploy::upload_archive` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/apps/{id}/deploy/jobs` | `deploy::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/apps/{id}/deploy/rollback` | `deploy::rollback` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/deploy/jobs/{id}` | `deploy::get` | Admin session | Read route; Origin not required |
| `GET` | `/api/deploy/jobs/{id}/logs` | `deploy::logs` | Admin session | Read route; Origin not required |
| `GET` | `/api/deploy/jobs/{id}/events` | `deploy::events` | Admin session | Read route; Origin not required |
| `POST` | `/api/deploy/jobs/{id}/cancel` | `deploy::cancel` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/deploy/jobs/{id}/retry` | `deploy::retry` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/fast-actions` | `fast_actions::list` | Admin session | Read route; Origin not required |
| `POST` | `/api/fast-actions` | `fast_actions::create` | Admin session | Same-origin Origin required before write |
| `PUT` | `/api/fast-actions/{id}` | `fast_actions::update` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/fast-actions/{id}` | `fast_actions::delete` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/fast-actions/{id}/run` | `fast_actions::run` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/network/traffic` | `network_traffic_volume` | Authenticated user session | Read route; Origin not required |
| `GET` | `/api/monitoring/alerts` | `alert_settings` | Authenticated user session | Read route; Origin not required |
| `PUT` | `/api/monitoring/alerts` | `update_alert_settings` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/monitoring/config` | `get_monitoring_config` | Authenticated user session | Read route; Origin not required |
| `PUT` | `/api/monitoring/config` | `put_monitoring_config` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/settings/port` | `get_panel_port` | Authenticated user session | Read route; Origin not required |
| `PUT` | `/api/settings/port` | `change_panel_port` | Admin session | Same-origin Origin required before write |
| `PUT` | `/api/settings/tls/port` | `change_panel_tls_port` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/settings/tls/port/confirm` | `confirm_panel_tls_port_change` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/settings/timezone` | `get_panel_timezone` | Authenticated user session | Read route; Origin not required |
| `PUT` | `/api/settings/timezone` | `change_panel_timezone` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/settings/tls` | `get_panel_tls_settings` | Authenticated user session | Read route; Origin not required |
| `PUT` | `/api/settings/tls` | `update_panel_tls_settings` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/update` | `panel_update_status` | Authenticated user session | Read route; Origin not required |
| `POST` | `/api/update` | `schedule_panel_update` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/update/cancel` | `cancel_panel_update` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/update/rollback` | `rollback_panel_update` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/update/progress` | `panel_update_progress` | Authenticated user session | Read route; Origin not required |
| `GET` | `/api/update/logs` | `panel_update_logs` | Authenticated user session | Read route; Origin not required |
| `GET` | `/api/packages` | `list_packages` | Admin session | Read route; Origin not required |
| `GET` | `/api/packages/runtimes` | `list_runtime_packs` | Admin session | Read route; Origin not required |
| `POST` | `/api/packages/preview` | `preview_package_install` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/packages/action` | `run_apt_operation` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/runtimes` | `runtimes::inventory` | Admin session | Read route; Origin not required |
| `GET` | `/api/services` | `list_system_services` | Admin session | Read route; Origin not required |
| `POST` | `/api/services/action` | `run_service_operation` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/docker` | `docker::docker_status` | Admin session | Read route; Origin not required |
| `POST` | `/api/docker/action` | `docker::docker_operation` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/mysql/status` | `mysql::status` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `GET` | `/api/mysql/access` | `mysql::access_status` | Admin session | Read route; Origin not required |
| `POST` | `/api/mysql/access` | `mysql::unlock` | MySQL module helper; admin session plus session-bound grant for database operations | MySQL reauthentication/session scope checked in module |
| `POST` | `/api/mysql/install` | `mysql::install` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `POST` | `/api/mysql/service` | `mysql::service_action` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `POST` | `/api/mysql/catalog` | `mysql::catalog` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `POST` | `/api/mysql/query` | `mysql::query` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `GET` | `/api/mysql/metrics` | `mysql::metrics` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `GET` | `/api/mysql/settings` | `mysql::settings` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `PUT` | `/api/mysql/settings` | `mysql::update_setting` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `POST` | `/api/mysql/databases` | `mysql::create_database` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `DELETE` | `/api/mysql/databases/{name}` | `mysql::delete_database` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `POST` | `/api/mysql/users/action` | `mysql::user_action` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `POST` | `/api/mysql/users/privileges` | `mysql::update_user_privileges` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `GET` | `/api/mysql/presets` | `mysql::presets` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `POST` | `/api/mysql/presets` | `mysql::save_preset` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `DELETE` | `/api/mysql/presets/{id}` | `mysql::delete_preset` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `POST` | `/api/mysql/users/password` | `mysql::reset_password` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `POST` | `/api/mysql/users` | `mysql::create_user` | Admin session + one-time password-confirmed MySQL grant bound to current session | Same-origin Origin is required for writes; no Origin requirement for reads |
| `GET` | `/api/files` | `files::list_files` | Admin session | Read route; Origin not required |
| `POST` | `/api/files/disk-scan` | `files::scan_disk` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/files/content` | `files::read_file` | Admin session | Read route; Origin not required |
| `GET` | `/api/files/download` | `files::download_file` | Admin session | Read route; Origin not required |
| `POST` | `/api/files/action` | `files::file_action` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/network/probe` | `port_probe` | Authenticated user session | Same-origin Origin required before write |
| `GET` | `/api/users` | `users` | Admin session | Read route; Origin not required |
| `POST` | `/api/users` | `add_user` | Admin session | Same-origin Origin required before write |
| `PUT` | `/api/users/{id}` | `update_user` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/audit` | `audit_log` | Admin session | Read route; Origin not required |
| `GET` | `/api/nginx/sites` | `nginx::nginx_sites` | Admin session | Read route; Origin not required |
| `POST` | `/api/nginx/sites` | `nginx::create_nginx_site` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/nginx/sites/{name}/rename` | `nginx::rename_nginx_site` | Admin session | Same-origin Origin required before write |
| `PUT` | `/api/nginx/sites/{name}/metadata` | `nginx::update_nginx_site_metadata` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/nginx/sites/{name}/actions` | `nginx::list_site_actions` | Admin session | Read route; Origin not required |
| `POST` | `/api/nginx/sites/{name}/actions` | `nginx::create_site_action` | Admin session | Same-origin Origin required before write |
| `PUT` | `/api/nginx/sites/{name}/actions/{id}` | `nginx::update_site_action` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/nginx/sites/{name}/actions/{id}` | `nginx::delete_site_action` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/nginx/sites/{name}/actions/{id}/run` | `nginx::run_site_action` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/nginx/sites/{name}/logs` | `nginx::api_nginx_site_logs` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/nginx/sites/{name}/traffic` | `nginx::api_nginx_site_traffic` | Admin session | Read route; Origin not required |
| `GET` | `/api/nginx/sites/{name}/runtime` | `nginx::api_nginx_site_runtime` | Admin session | Read route; Origin not required |
| `PUT` | `/api/nginx/sites/{name}/runtime` | `nginx::api_nginx_site_runtime` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/nginx/sites/{name}/repository` | `nginx::api_nginx_site_repository` | Admin session | Read route; Origin not required |
| `POST` | `/api/nginx/sites/{name}/sync` | `nginx::sync_nginx_site` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/nginx/sites/{name}/ssl` | `nginx::issue_nginx_site_ssl` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/nginx/domains/check` | `nginx::check_nginx_domains` | Admin session | Read route; Origin not required |
| `GET` | `/api/nginx/status` | `nginx::nginx_status` | Admin session | Read route; Origin not required |
| `GET` | `/api/nginx/firewall` | `nginx::get_nginx_firewall_setting` | Admin session | Read route; Origin not required |
| `PUT` | `/api/nginx/firewall` | `nginx::update_nginx_firewall_setting` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/nginx/files` | `nginx::api_nginx_browse_files` | Admin session | Read route; Origin not required |
| `POST` | `/api/nginx/action` | `nginx::nginx_action` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/nginx/sites/{name}` | `nginx::get_nginx_site` | Admin session | Read route; Origin not required |
| `PUT` | `/api/nginx/sites/{name}` | `nginx::save_nginx_site` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/nginx/sites/{name}` | `nginx::delete_nginx_site` | Admin session | Same-origin Origin required before write |
| `PUT` | `/api/nginx/sites/{name}/enabled` | `nginx::toggle_nginx_site` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/nginx/sites/{name}/maintenance` | `nginx::nginx_site_maintenance_status` | Admin session | Read route; Origin not required |
| `PUT` | `/api/nginx/sites/{name}/maintenance` | `nginx::nginx_site_maintenance` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/nginx/sites/{name}/maintenance/secret` | `nginx::nginx_site_maintenance_secret` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/nginx/sites/{name}/permissions/fix` | `nginx::fix_nginx_site_permissions` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/firewall` | `firewall` | Authenticated user session | Read route; Origin not required |
| `PUT` | `/api/firewall` | `update_firewall_state` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/firewall/install` | `install_firewall` | Admin session | Same-origin Origin required before write |
| `GET` | `/api/firewall/rules` | `managed_firewall_rules` | Authenticated user session | Read route; Origin not required |
| `POST` | `/api/firewall/rules` | `add_firewall_rule` | Admin session | Same-origin Origin required before write |
| `PUT` | `/api/firewall/rules/{id}` | `update_firewall_rule` | Admin session | Same-origin Origin required before write |
| `DELETE` | `/api/firewall/rules/{id}` | `delete_firewall_rule` | Admin session | Same-origin Origin required before write |
| `POST` | `/api/firewall/rules/external` | `delete_external_firewall_rule` | Admin session | Same-origin Origin required before write |

## Phase 2 verification

- `cargo check --locked --all-targets`: PASS.
- `cargo test --locked --all-targets`: PASS, 216 tests.
- Focused regressions passed: API-key owner revocation; API no-store path scope; per-IP login throttling/proxy trust; legacy lockout isolation; and volume-scan Origin validation.
- `cargo fmt --all -- --check` could not run in the current isolated Docker image because the `cargo-fmt` component is absent. The recorded baseline formatter check had reported formatting differences; formatting is therefore not claimed as passing here.
- Clippy was not rerun in this phase; the baseline `clippy -D warnings` failure is recorded in `00-baseline.md` and remains open for the quality-gates phase.
- No database schema migration was required.
- Inventory size at this revision: 229 path templates, 297 method-handler registrations.
