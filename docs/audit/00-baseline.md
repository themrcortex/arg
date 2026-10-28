# 00 — Baseline and architecture map

## Audit baseline

- Repository: `themrcortex/arg`
- Reference branch: `development`
- Baseline commit: `eec273cde037fa57edc1b5ab042fda7acc26a32c` (`feat: add custom pre-deploy commands and live job output`)
- Baseline date: 2026-10-28 UTC
- Version at baseline: `0.7.3-dev.1` in `Cargo.toml` and `frontend/package.json`. The request's observed `0.6.9-dev.12` does not match this checkout.
- Audit branch: `audit/development-hardening`, created from the recorded development commit.
- Initial working tree: only `generated_images/` was untracked. It is pre-existing local data and is not part of the audit commits. It has not been opened, changed, or removed.
- No production VPS, live database, user account, or external service was contacted.

## Architecture map

| Area | Current implementation | Main boundaries |
|---|---|---|
| Rust/Axum API | `src/main.rs`, `src/routes.rs`, feature modules under `src/` | Router registration is centralized; most handlers are grouped by product feature. `main.rs` still owns boot, account/session, terminal, helpers, several API handlers, and global state. |
| Frontend | React 19, TypeScript, Vite, Tailwind, Radix, Lucide under `frontend/src/` | Hash routing in `navigation.ts` and `App.tsx`; page APIs in `frontend/src/lib/api.ts`; production assets are committed in `frontend/dist/` and embedded by Rust. |
| Database | SQLite through `rusqlite` | Shared application state is `Arc<Mutex<Connection>>`; module migrations run from startup. Database lock/transaction behavior is in the later database phase. |
| Authentication | `main.rs` | Argon2 password verification, hashed session tokens, role checks, cookie sessions, same-origin helper, root-terminal tickets. Session and CSRF coverage is not considered complete until phase 2. |
| Federation | `federation.rs`, `fleet.rs`, `private_networks.rs`, plus workspace proxy routes | Pairing credentials are stored separately from the database; workspace proxy requests carry scoped expiring sessions. Private-network policy endpoints persist drafts and explicitly report no enforcement. |
| Privileged helper | `main.rs` helper socket server and action dispatch; external update/UFW/port helpers under `scripts/` and `/usr/local/lib/arg/` at install time | Unix socket peer credentials are checked using `SO_PEERCRED`; requests are JSON and action-allowlisted. Per-action authorization/path/TOCTOU coverage is audited in phase 3. |
| Terminal | `/api/terminal` WebSocket in `main.rs`; workspace terminal forwarding in `federation.rs` | Ordinary and root terminals use different session/ticket flows; PTY ownership and cancellation are reviewed separately. |
| Files/storage | `files.rs`, `storage_mounts.rs`, `app_code.rs`, `app_storage.rs`, `app_s3.rs`, `s3/mod.rs`, `app_backups.rs`, `app_snapshots.rs` | File operations route through the helper; object storage uses S3-compatible client code and its own transfer paths. S3 network pinning is a later phase. |
| Nginx/SSL/DNS | `nginx.rs`, `nginx_domains.rs`, `dns.rs`, `dns_vault.rs` | Nginx operations route through manager requests/helper paths; DNS secrets are separated into a vault module. |
| Docker/apps/runtime | `docker.rs`, `app_compose.rs`, `apps.rs`, `buildpacks.rs`, `runtimes.rs`, `node.rs`, `laravel.rs` | Application ownership, runtime-specific services, Compose links, deployment jobs, and release lifecycle are split across modules. |
| Deployment/rollback | `deploy.rs`, `node.rs`, runtime modules | Durable SQLite jobs/steps/logs, source acquisition, build, custom argv pre-deploy commands, activation and rollback. Every child process and cancellation path is included in phase 5. |
| Updates/install | `install.sh`, `scripts/update-helper.sh`, `scripts/bump-version.sh`, update functions in `main.rs` | Builds are resource-aware; active binary swap and rollback are managed separately from API serving. CI currently tests system scripts but not a clean/upgrade VPS installation. |
| CI | `.github/workflows/ci.yml`, `.github/workflows/docs-pages.yml` | GitHub Actions has Rust test/check/release build, frontend checks/build/embed validation, docs checks/build/content checks, and script checks. Rust formatting, Clippy, dependency audit, secret scanning, and E2E are not configured in CI. |

### Frontend route inventory

The navigation uses hash routes. Current page IDs are `dashboard`, `system`, `nodes`, `apps`, `ssh-keys`, `maintenance`, `mysql`, `network`, `ports`, `firewall`, `nginx`, `docker`, `files`, `s3`, `dns`, `terminal`, `packages`, `services`, `users`, `audit`, `settings`, `account`, and `bug-report`. Some management pages also have internal tabs/subroutes.

### Backend route inventory

The following 229 path templates are extracted from the Axum router at the baseline commit. They record registered paths and methods; they do not by themselves prove the handler's authorization. The full authorization matrix is a phase 2 deliverable.

| Methods | Path template |
|---|---|
| `GET` | `/api/state` |
| `POST` | `/api/setup` |
| `POST` | `/api/login` |
| `POST` | `/api/logout` |
| `POST` | `/api/account/password` |
| `POST` | `/api/account/username` |
| `GET` | `/api/account/sessions` |
| `DELETE` | `/api/account/sessions/{id}` |
| `POST` | `/api/account/sessions/revoke-others` |
| `GET` | `/api/account/capabilities` |
| `GET,PUT` | `/api/account/dashboard-preferences` |
| `GET` | `/api/terminal` |
| `POST` | `/api/terminal/root/authorize` |
| `GET` | `/api/dashboard` |
| `GET` | `/api/storage/volumes` |
| `GET` | `/api/maintenance/system` |
| `GET` | `/api/maintenance/doctor` |
| `GET,POST` | `/api/ssh-keys` |
| `GET,POST` | `/api/ssh-keys/trusted-hosts/github` |
| `DELETE,PUT` | `/api/ssh-keys/{id}` |
| `GET,POST` | `/api/admin/api-keys` |
| `DELETE` | `/api/admin/api-keys/{id}` |
| `GET` | `/api/v1/openapi.json` |
| `GET` | `/api/v1/apps` |
| `GET` | `/api/v1/deployments` |
| `GET` | `/api/security/posture` |
| `GET,POST` | `/api/extensions/trust-keys` |
| `DELETE` | `/api/extensions/trust-keys/{id}` |
| `POST` | `/api/extensions/validate` |
| `GET,POST` | `/api/maintenance/backups` |
| `GET,PUT` | `/api/maintenance/backup-policy` |
| `POST` | `/api/maintenance/backups/upload` |
| `POST` | `/api/maintenance/backups/{id}/restore` |
| `GET` | `/api/maintenance/backups/{id}/download` |
| `POST` | `/api/maintenance/action` |
| `GET,POST` | `/api/nodes` |
| `PUT` | `/api/nodes/role` |
| `PUT` | `/api/nodes/public-name` |
| `POST` | `/api/nodes/pairing-code` |
| `DELETE,PUT` | `/api/nodes/controllers/{id}` |
| `GET` | `/api/nodes/{id}/status` |
| `GET,POST` | `/api/nodes/{id}/update` |
| `GET` | `/api/nodes/{id}/update/progress` |
| `GET` | `/api/nodes/{id}/update/logs` |
| `POST` | `/api/nodes/{id}/update/cancel` |
| `POST` | `/api/nodes/{id}/update/rollback` |
| `GET` | `/api/nodes/{id}/services` |
| `POST` | `/api/nodes/{id}/services/action` |
| `POST` | `/api/nodes/{id}/rotate` |
| `GET` | `/api/nodes/{workspace}/workspace-terminal` |
| `ANY` | `/api/nodes/{workspace}/proxy/{*path}` |
| `DELETE` | `/api/nodes/{id}` |
| `POST` | `/api/federation/pair` |
| `GET` | `/api/federation/status` |
| `GET,POST` | `/api/federation/update` |
| `GET` | `/api/federation/update/progress` |
| `GET` | `/api/federation/update/logs` |
| `POST` | `/api/federation/update/cancel` |
| `POST` | `/api/federation/update/rollback` |
| `GET` | `/api/federation/services` |
| `POST` | `/api/federation/services/action` |
| `DELETE` | `/api/federation/controller` |
| `POST` | `/api/federation/controller/rotate` |
| `POST` | `/api/federation/workspace/renew` |
| `PUT` | `/api/federation/controller/name` |
| `GET` | `/api/s3/providers` |
| `GET` | `/api/s3/summary` |
| `GET` | `/api/s3/buckets/catalog` |
| `GET,POST` | `/api/s3/connections` |
| `DELETE,GET,PUT` | `/api/s3/connections/{id}` |
| `POST` | `/api/s3/connections/{id}/test` |
| `GET,POST` | `/api/s3/connections/{id}/buckets` |
| `POST` | `/api/s3/connections/{id}/buckets/create` |
| `GET` | `/api/s3/connections/{id}/objects` |
| `POST` | `/api/s3/connections/{id}/objects/delete` |
| `POST` | `/api/s3/connections/{id}/objects/upload` |
| `GET` | `/api/s3/connections/{id}/objects/download` |
| `POST` | `/api/s3/connections/{id}/transfer/local-to-bucket` |
| `POST` | `/api/s3/connections/{id}/transfer/bucket-to-local` |
| `DELETE` | `/api/s3/buckets/{id}` |
| `GET,PUT` | `/api/s3/buckets/{id}/usage` |
| `POST` | `/api/s3/buckets/{id}/usage/scan` |
| `GET,PUT` | `/api/s3/buckets/{id}/mount` |
| `POST` | `/api/s3/mounts/{id}/stop` |
| `GET` | `/api/history` |
| `GET,POST` | `/api/apps` |
| `GET,POST` | `/api/app-platform/workspaces` |
| `GET` | `/api/app-templates` |
| `POST` | `/api/app-templates/{id}/draft` |
| `GET,POST` | `/api/app-platform/workspaces/{id}/projects` |
| `GET,POST` | `/api/app-platform/projects/{id}/environments` |
| `GET,POST` | `/api/apps/{id}/build-plans` |
| `GET` | `/api/apps/{id}/source/tree` |
| `GET` | `/api/apps/{id}/source/file` |
| `GET,POST` | `/api/apps/{id}/snapshots` |
| `GET,POST` | `/api/apps/{id}/workflows` |
| `DELETE` | `/api/apps/{id}/workflows/{workflow_id}` |
| `GET,POST` | `/api/apps/{id}/containers` |
| `DELETE` | `/api/apps/{id}/containers/{container_id}` |
| `DELETE,GET,PUT` | `/api/apps/{id}/nginx-site` |
| `GET,POST` | `/api/apps/{id}/backups` |
| `DELETE` | `/api/apps/{id}/backups/{backup_id}` |
| `GET` | `/api/apps/{id}/backups/{backup_id}/download` |
| `GET` | `/api/postgres/status` |
| `PUT` | `/api/nodes/{id}/tags` |
| `GET` | `/api/fleet/groups` |
| `GET` | `/api/fleet/rollouts` |
| `POST` | `/api/fleet/rollouts/preview` |
| `GET,POST` | `/api/private-network/policies` |
| `DELETE` | `/api/private-network/policies/{id}` |
| `GET,POST` | `/api/dns/connections` |
| `DELETE` | `/api/dns/connections/{id}` |
| `GET,POST` | `/api/dns/connections/{id}/records` |
| `DELETE,PUT` | `/api/dns/connections/{id}/records/{record_id}` |
| `PUT` | `/api/apps/{id}/environment` |
| `GET,POST` | `/api/app-networks` |
| `GET,POST` | `/api/app-networks/{id}/members` |
| `DELETE` | `/api/app-networks/{id}/members/{app_id}` |
| `DELETE` | `/api/app-networks/{id}` |
| `GET,POST` | `/api/apps/{id}/volumes` |
| `GET,POST` | `/api/apps/{id}/s3-bindings` |
| `DELETE` | `/api/app-s3-bindings/{id}` |
| `POST` | `/api/app-volumes/{id}/scan` |
| `DELETE` | `/api/app-volumes/{id}` |
| `POST` | `/api/app-volumes/{id}/purge` |
| `DELETE,GET,PUT` | `/api/apps/{id}` |
| `POST` | `/api/apps/{id}/restore` |
| `POST` | `/api/apps/{id}/purge` |
| `GET` | `/api/apps/{id}/laravel` |
| `GET,POST` | `/api/apps/{id}/laravel/operations` |
| `GET,POST` | `/api/apps/{id}/laravel/web` |
| `POST` | `/api/apps/{id}/laravel/services` |
| `POST` | `/api/apps/{id}/laravel/database` |
| `POST,PUT` | `/api/apps/{id}/laravel/environment` |
| `GET` | `/api/apps/{id}/node` |
| `GET,PUT` | `/api/apps/{id}/node/runtime-settings` |
| `GET,PUT` | `/api/apps/{id}/node/service` |
| `PUT` | `/api/apps/{id}/node/environment` |
| `GET,PUT` | `/api/apps/{id}/node/build-environment` |
| `GET` | `/api/apps/{id}/node/service/logs` |
| `GET` | `/api/apps/{id}/monitoring` |
| `PUT` | `/api/apps/{id}/monitoring/rules` |
| `POST` | `/api/apps/{id}/monitoring/events/{event_id}/ack` |
| `POST` | `/api/apps/{id}/deploy` |
| `POST` | `/api/apps/{id}/deploy/archive` |
| `GET` | `/api/apps/{id}/deploy/jobs` |
| `POST` | `/api/apps/{id}/deploy/rollback` |
| `GET` | `/api/deploy/jobs/{id}` |
| `GET` | `/api/deploy/jobs/{id}/logs` |
| `GET` | `/api/deploy/jobs/{id}/events` |
| `POST` | `/api/deploy/jobs/{id}/cancel` |
| `POST` | `/api/deploy/jobs/{id}/retry` |
| `GET,POST` | `/api/fast-actions` |
| `DELETE,PUT` | `/api/fast-actions/{id}` |
| `POST` | `/api/fast-actions/{id}/run` |
| `GET` | `/api/network/traffic` |
| `GET,PUT` | `/api/monitoring/alerts` |
| `GET,PUT` | `/api/monitoring/config` |
| `GET,PUT` | `/api/settings/port` |
| `PUT` | `/api/settings/tls/port` |
| `POST` | `/api/settings/tls/port/confirm` |
| `GET,PUT` | `/api/settings/timezone` |
| `GET,PUT` | `/api/settings/tls` |
| `GET,POST` | `/api/update` |
| `POST` | `/api/update/cancel` |
| `POST` | `/api/update/rollback` |
| `GET` | `/api/update/progress` |
| `GET` | `/api/update/logs` |
| `GET` | `/api/packages` |
| `GET` | `/api/packages/runtimes` |
| `POST` | `/api/packages/preview` |
| `POST` | `/api/packages/action` |
| `GET` | `/api/runtimes` |
| `GET` | `/api/services` |
| `POST` | `/api/services/action` |
| `GET` | `/api/docker` |
| `POST` | `/api/docker/action` |
| `GET` | `/api/mysql/status` |
| `GET,POST` | `/api/mysql/access` |
| `POST` | `/api/mysql/install` |
| `POST` | `/api/mysql/service` |
| `POST` | `/api/mysql/catalog` |
| `POST` | `/api/mysql/query` |
| `GET` | `/api/mysql/metrics` |
| `GET,PUT` | `/api/mysql/settings` |
| `POST` | `/api/mysql/databases` |
| `DELETE` | `/api/mysql/databases/{name}` |
| `POST` | `/api/mysql/users/action` |
| `POST` | `/api/mysql/users/privileges` |
| `GET,POST` | `/api/mysql/presets` |
| `DELETE` | `/api/mysql/presets/{id}` |
| `POST` | `/api/mysql/users/password` |
| `POST` | `/api/mysql/users` |
| `GET` | `/api/files` |
| `POST` | `/api/files/disk-scan` |
| `GET` | `/api/files/content` |
| `GET` | `/api/files/download` |
| `POST` | `/api/files/action` |
| `POST` | `/api/network/probe` |
| `GET,POST` | `/api/users` |
| `PUT` | `/api/users/{id}` |
| `GET` | `/api/audit` |
| `GET,POST` | `/api/nginx/sites` |
| `POST` | `/api/nginx/sites/{name}/rename` |
| `PUT` | `/api/nginx/sites/{name}/metadata` |
| `GET,POST` | `/api/nginx/sites/{name}/actions` |
| `DELETE,PUT` | `/api/nginx/sites/{name}/actions/{id}` |
| `POST` | `/api/nginx/sites/{name}/actions/{id}/run` |
| `POST` | `/api/nginx/sites/{name}/logs` |
| `GET` | `/api/nginx/sites/{name}/traffic` |
| `GET,PUT` | `/api/nginx/sites/{name}/runtime` |
| `GET` | `/api/nginx/sites/{name}/repository` |
| `POST` | `/api/nginx/sites/{name}/sync` |
| `POST` | `/api/nginx/sites/{name}/ssl` |
| `GET` | `/api/nginx/domains/check` |
| `GET` | `/api/nginx/status` |
| `GET,PUT` | `/api/nginx/firewall` |
| `GET` | `/api/nginx/files` |
| `POST` | `/api/nginx/action` |
| `DELETE,GET,PUT` | `/api/nginx/sites/{name}` |
| `PUT` | `/api/nginx/sites/{name}/enabled` |
| `GET,PUT` | `/api/nginx/sites/{name}/maintenance` |
| `POST` | `/api/nginx/sites/{name}/maintenance/secret` |
| `POST` | `/api/nginx/sites/{name}/permissions/fix` |
| `GET,PUT` | `/api/firewall` |
| `POST` | `/api/firewall/install` |
| `GET,POST` | `/api/firewall/rules` |
| `DELETE,PUT` | `/api/firewall/rules/{id}` |
| `POST` | `/api/firewall/rules/external` |

### Preliminary authorization map

This is an inventory, not a complete approval claim. Handler-level checks and Federation behavior will be traced endpoint by endpoint in phase 2.

| Surface | Observed control | Audit status |
|---|---|---|
| Cookie-authenticated endpoints | `require_user` or `require_admin` appears in handlers; login cookie is HttpOnly, SameSite=Strict, has an expiry, and is marked Secure when transport detection says HTTPS. | Not yet a full route-to-handler matrix. |
| Mutating browser requests | Many write handlers invoke `check_origin`, which requires exact `http(s)://Host`; other handlers rely on JSON/middleware or need individual review. | Audit required. |
| Public API keys | `/api/v1/*` and `/api/admin/api-keys*` are routed through `api_keys`; key scopes/hash/expiry/rate limiting have existing unit tests. | Review all reachable combinations in phase 2. |
| Federation workspace proxy | `workspace_session_guard` derives read/write/terminal/root-terminal scope from path/method and validates stored expiring proxy sessions. | Root-terminal scope edge cases are under review. |
| Privileged operations | Selected requests are forwarded through a protected Unix socket to allowlisted helper actions; `SO_PEERCRED` is used. | Per-action proof and race analysis pending phase 3. |

### Root/privileged execution paths

- Backend-to-helper transport: JSON request over `PRIVILEGED_SOCKET`; requests are size-bounded and dispatched by explicit action name in `execute_privileged_request`.
- Helper server: `run_privileged_helper` and `serve_privileged_connection` in `main.rs`; peer credentials are checked before dispatch.
- External bounded helper binaries: update helper, UFW helper, panel-port helper; installation paths under `/usr/local/lib/arg/`.
- Runtime command paths include systemd, Nginx, MySQL, Docker, Git, runtime tools, and certificate utilities. Phase 3/5/7 will inventory exact arguments and filesystem scope; no production commands were run in this audit.

## Complexity and maintenance baseline

Largest source files by line count at baseline:

| File | Lines |
|---|---:|
| `src/main.rs` | 5,615 |
| `src/nginx.rs` | 3,813 |
| `src/deploy.rs` | 3,534 |
| `src/node.rs` | 3,080 |
| `src/laravel.rs` | 2,556 |
| `src/s3/mod.rs` | 2,523 |
| `src/federation.rs` | 2,482 |
| `frontend/src/pages.tsx` | 2,436 |
| `src/mysql.rs` | 1,510 |
| `src/files.rs` | 1,374 |
| `frontend/src/ApplicationsPage.tsx` | 1,154 |
| `frontend/src/App.tsx` | 1,188 |

The Rust dependency set is locked in `Cargo.lock`; frontend and documentation dependencies are locked by npm lockfiles. The dependency advisory status has not yet been checked. No dependency is labeled vulnerable without advisory evidence.

## Existing tests and pipeline

- Rust baseline: 203 unit tests (`cargo test --locked --all-targets`) passed at the baseline commit.
- Frontend scripts: `check` (TypeScript), `build` (TypeScript + Vite + service-worker stamp), `test:offline` (two Node tests). There is no dedicated React component/E2E test or lint script in `frontend/package.json`.
- Documentation scripts: `check`, `build` (which regenerates metadata/search index), and `scripts/tests/test-docs-content.sh` passed.
- CI workflow has separate Rust, frontend, documentation, and system-script jobs. It does not currently run `cargo fmt`, Clippy, dependency advisories, secret scanning, or browser E2E.
- Baseline warnings observed: Vite reports the main JavaScript chunk is slightly above 500 kB; docs Vite leaves local font URLs unresolved at build time (they are resolved at runtime). Both builds completed successfully.

## Baseline verification at `eec273c`

| Command/check | Result | Evidence/limits |
|---|---|---|
| `cargo check --locked --all-targets` | PASS | Docker Rust toolchain; no source changes. |
| `cargo test --locked --all-targets` | PASS | 203 passed, 0 failed. |
| `cargo fmt --all -- --check` | FAIL | Rustfmt ran after installing the matching component in the isolated toolchain. It reports formatting differences across existing modules; check-only was used, so no source was rewritten. |
| `cargo clippy --locked --all-targets -- -D warnings` | FAIL | Clippy ran on the matching 1.99.1 toolchain. Warnings promoted to errors: 51 in the binary target and 53 in the test target. Examples include `type_complexity`, `manual_is_multiple_of`, `redundant_pattern_matching`, and test `field_reassign_with_default`. No code was changed in baseline. |
| `npm run check` (`frontend`) | PASS | TypeScript check. |
| `npm run test:offline` | PASS | 2 passed, 0 failed. |
| `npm run build` (`frontend`) | PASS | Vite production build; chunk-size warning above. |
| `npm run check` (`docs-site`) | PASS | TypeScript check. |
| `npm run build` (`docs-site`) | PASS | Vite build; local font URLs unresolved at build time. |
| `bash ../scripts/tests/test-docs-content.sh` | PASS | 32 canonical pages, 64 localized articles, internal links and legacy prefixes checked. |
| `bash scripts/tests/test-update-selection.sh` | PASS | Update selection, low-memory settings and cleanup tests. |
| `bash -n install.sh` | PASS | Installer syntax. |

## Scope limitations and follow-up

This baseline is static and test-based. It does not claim a live VPS installation, external DNS/SSL issuance, remote Node pairing, actual S3 provider access, Docker deployment, or systemd mount lifecycle was tested. These require isolated test fixtures or a disposable non-production VM; no production host was accessed. Endpoint authorization, privileged operations, migrations, network SSRF, process cancellation, runtime pressure, and UI race behavior remain phase-specific audit work.
