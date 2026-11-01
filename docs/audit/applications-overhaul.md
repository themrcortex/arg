# Applications overhaul audit

## Baseline

- Branch: `development`
- Starting revision: `7975c3fae94836556baa551ff9359f826bb05029`
- Existing local untracked directories were left untouched.
- Scope: application registry, deployment jobs/releases, Node and Laravel runners, application source browsing, workspaces, Nginx linkage, helper operations, monitoring, backups, and the React Applications UI.

## Architecture map

| Concern | Existing implementation | Notes |
|---|---|---|
| HTTP routes | `src/routes.rs` | Registers app CRUD, source browsing, deployment, runtime, Nginx, monitoring, backup, volume, S3, Compose and workspace endpoints. Admin checks remain handler-specific. |
| Registry and ownership | `src/apps.rs` | SQLite `applications` and `application_services`; owner is the local server UUID. Registry create/update/delete uses version checks, domain/port validation and an operation mutex. |
| Deployment | `src/deploy.rs` | Persistent jobs, steps, logs and releases; bounded global queue; per-app active-job uniqueness; idempotency keys; cancellation, retry, startup recovery, event stream and rollback. |
| Runtime adapters | `src/node.rs`, `src/laravel.rs`, `src/runtimes.rs` | Node build uses transient systemd-run sandbox; long-running Node apps use ARG-managed DynamicUser units. Laravel has separate web, PHP-FPM and Artisan/service paths. |
| Detection | `src/buildpacks.rs`, `src/node.rs`, `src/laravel.rs` | Manifest analysis produces a plan; ambiguous framework selection is explicit. Runtime installation is handled separately through the package/runtime manager. |
| Source and files | `src/app_code.rs`, `src/deploy.rs`, `src/ssh_keys.rs` | Source tree reads the active release; archive and Git sources enter deployment staging. Private GitHub SSH keys are held by the server-side key manager. |
| Domains and web | `src/nginx_domains.rs`, `src/nginx.rs`, `src/laravel.rs`, `src/node.rs` | Application domain linkage and runtime-specific Nginx generation go through manager/helper operations. Certificates use the existing site SSL path, not a separate app certificate store. |
| Persistent data | `src/app_storage.rs`, `src/app_s3.rs`, `src/app_backups.rs`, `src/app_snapshots.rs` | Volumes and S3 bindings are separate resources. Volume quota is reported as unenforced; application code snapshots exclude runtime/database data and are explicitly non-restorable. |
| Monitoring and logs | `src/app_monitoring.rs`, runtime log handlers | Systemd/cgroup metrics are sampled; job logs persist in SQLite; Laravel and Node runtime logs come from their runtime sources. Request-level metrics require app instrumentation and are not inferred. |
| Multi-server | `src/federation.rs`, remote workspace proxy and capability records | App APIs can be proxied to a selected node. Node-specific capability/version mismatch must be surfaced; the local registry is not a shared cross-node database. |
| UI | `frontend/src/ApplicationsPage.tsx` and app-specific panels | React/TypeScript page owns list/detail flows and API calls; deployment job details persist and refresh through API/SSE. Production assets are committed in `frontend/dist/`. |
| Persistence/migrations | module `migrate` functions called during startup | Additive SQLite migrations are the existing convention. Workspaces/environments already exist, but assignment scopes a record; it does not yet give each environment an independent runtime, secret set, deployment history, or process. |

## Existing capability inventory

**Implemented in code:** application CRUD/archive/restore/purge; framework planning; Git/ZIP/directory deployment sources; SSH deploy keys; bounded deployment queue; persisted job state and logs; Node static/SSR builds; Laravel operations and environment support; release activation/rollback; domain conflict checks and managed Nginx configuration; process service controls; database/volume/S3 links; monitoring samples and alerts; backup/snapshot records; application-scoped file browsing; admin checks and audit events on sensitive writes.

**Partial or explicitly limited:** Production/Staging are workspace metadata rather than isolated execution environments; application volumes currently report quota as unenforced; snapshots are not restores; no general encrypted per-environment secret manager exists; no generic Worker/Cron manager for arbitrary apps; no verified request-rate/HTTP error metrics without instrumentation; no provider-independent DNS-01 certificate flow; no full interactive application terminal isolated to an app identity; Git support is narrower than the requested public/private provider matrix; Docker Compose links are metadata around externally managed containers, not an app-specific Compose deployment engine.

## Findings

### APP-001 — Corrupt application labels are silently returned as an empty list

- Severity: P2 / Medium
- Status: CONFIRMED
- Confidence: high
- Location: `src/apps.rs`, `read_apps`, labels JSON decoding
- Evidence: a failed `serde_json::from_str::<Vec<String>>` was converted with `unwrap_or_default()`, so a malformed stored value looked like an application with no labels and concealed the damaged metadata.
- Fix: return a typed SQLite conversion error and add `application_list_reports_corrupt_labels_instead_of_silently_dropping_them`.
- Verification: the regression test passes; malformed metadata now fails the list read explicitly instead of silently changing the apparent record.

### APP-002 — Application environments are not execution isolation boundaries

- Severity: P1 / High product gap; no exploit established
- Status: CONFIRMED
- Evidence: app records can be assigned to workspace environment IDs, while deployment and runtime ownership/configuration are still primarily keyed by application ID. Existing wording in the UI says environment assignment does not copy or isolate configuration.
- Impact: do not represent the current environment selector as independent Production/Staging secrets, processes, domains or deployment histories.
- Plan: retain existing IDs and defaults; introduce only additive environment-scoped records after cross-module ownership and cleanup rules are specified and migration-tested. The Phase 9/10 review confirmed no environment-scoped runtime or secret store is wired into deployment.

### APP-003 — Queue/process robustness needs deployment-specific integration validation

- Severity: P2 / Medium
- Status: AUDIT_REQUIRED
- Evidence: persistent leases and startup recovery exist; unit tests cover interruption and child-process cancellation. This workspace has no isolated systemd/Nginx VPS to validate real unit lifecycle, cancellation of grandchildren, or rollback after an actual service failure.
- Plan: keep claims bounded to covered behavior and mark host-level scenarios BLOCKED until an isolated Linux test host is available.

No P0/P1 security exploit was established by this phase. The points above must not be treated as proof that every helper path, provider, or remote-node scenario is safe; those require their phase-specific tests.

### APP-004 — Build plan can disagree with the selected runtime package manager

- Severity: P2 / Medium
- Status: CONFIRMED
- Confidence: high
- Location: `src/buildpacks.rs`, `build_commands` and `detect`; `src/node.rs`, `choose_package_manager`
- Evidence: the runtime rejected multiple lockfiles and enforced `packageManager`, while the saved Build Plan selected the first lockfile by fixed priority, suggested Yarn Berry-only `--immutable`, and proposed `run build` even when `scripts.build` was absent. Manifest reads also followed symlinks and converted parse failures into an unannotated Custom plan.
- Implemented: Build Plan now reuses the Node runtime package-manager decision, produces install/build commands only for a matching supported manager and defined build script, warns on malformed/oversized/symlinked manifests, and exposes the selected project root in the UI for monorepos.
- Tests: matching npm/Yarn Classic; conflicting lockfiles; missing build script; Next.js/Nuxt; manual monorepo subdirectory; malformed, oversized and symlinked manifests; Python, static and unknown projects.
- Remaining risk: a Build Plan is still analysis-only. Native PHP and provider-independent runtime adapters are not implemented by this change.

### APP-005 — Application deployment does not provide a Native PHP runtime adapter

- Severity: P1 / High product gap
- Status: CONFIRMED
- Confidence: high
- Location: `src/apps.rs`, `src/buildpacks.rs`, `src/deploy.rs`, `src/laravel.rs`
- Evidence: the application framework registry and deploy dispatcher have Laravel and Node paths, but no Native PHP application runner with PHP-FPM/document-root configuration. WordPress templates remain explicitly draft-only.
- Impact: do not claim the requested Native PHP deployment matrix case is supported end to end.
- Plan: add a PHP-FPM adapter with a non-root identity, shared-storage policy and Nginx validation only after its lifecycle/rollback behavior can be integration-tested on a disposable host.

### APP-006 — Private repository support is limited to GitHub SSH Deploy Keys

- Severity: P1 / High product gap
- Status: CONFIRMED
- Confidence: high
- Location: `src/deploy.rs`, `src/ssh_keys.rs`, `frontend/src/ApplicationsPage.tsx`
- Evidence: public HTTPS URLs are allowlisted for GitHub, GitLab, Bitbucket and Codeberg; private SSH URLs and host-key retrieval are restricted to GitHub. There is no encrypted Git-token credential vault, provider OAuth integration, webhook secret/replay handler, push-to-deploy, or auto-deploy branch policy.
- Existing protections: SSH key files are restricted to owner-only permissions; Deploy Keys are repository-bound and may be application-bound; host keys are fetched over HTTPS and checked by OpenSSH; the public key must be explicitly installed by an administrator.
- Tests: the phase 5 security regression checks that a Deploy Key cannot cross its repository or application binding.
- Plan: add provider credential types only with a secret-vault-backed lifecycle, provider-specific host verification, revocation and audit behavior. Do not put credentials in Git URLs or job logs.

### APP-007 — Custom deployment pipeline covers fewer stages than the requested pipeline model

- Severity: P1 / High product gap
- Status: CONFIRMED
- Confidence: high
- Location: `src/deploy.rs`, `src/node.rs`
- Evidence: the current source deployment has runtime-specific install/build work and up to eight custom argv-only pre-deploy commands. It does not persist independently configurable pre-build, post-build, post-deploy, migration policy, per-step retry policy or command-specific timeouts. Node build and custom commands use a bounded systemd sandbox (2 GiB memory, 200% CPU, 20 minutes) and deployment logs are bounded/redacted.
- Impact: the existing pre-deploy editor is not the full ten-stage command pipeline requested in the spec.
- Plan: evolve the existing job-step schema additively, validate each command at enqueue time, use an allowlisted runtime adapter, and preserve the active release on failure.

### APP-008 — Laravel service rollback could silently leave systemd in the wrong state

- Severity: P2 / Medium
- Status: CONFIRMED; fixed
- Confidence: high
- Location: `src/laravel.rs`, `laravel_service_helper` and `restore_service_unit`
- Evidence: after `systemctl enable --now` failed or a unit failed to stay active, rollback ignored file restoration and systemd errors. An existing unit that was enabled but inactive was restored with `disable --now`, changing its autostart policy.
- Implemented: capture the previous enabled state; atomically restore the old unit; reload systemd; restore active/inactive and enabled/disabled state; return recovery failures to the API error. If the first `daemon-reload` fails, the previous configuration is restored as well.
- Tests: `laravel_service_rollback_preserves_enabled_inactive_unit_state` and `laravel_service_rollback_reports_systemd_recovery_failures` cover the prior state and visible recovery failure.
- Remaining risk: a real unit start/stop and recovery against systemd remains blocked without a disposable Linux host.

### APP-009 — Node systemd unit writes could be blocked by stale temporary files

- Severity: P2 / Medium
- Status: CONFIRMED; fixed
- Confidence: high
- Location: `src/node.rs`, `write_managed_unit`
- Evidence: every helper process used the same PID-only temporary path in `/etc/systemd/system`. A helper killed during a write could leave the path behind, and PID reuse would make a later service install fail with `AlreadyExists`. Concurrent unit writes in the same helper also shared the path.
- Implemented: each atomic replacement now uses a `create_new` temporary name with a per-write random suffix; cleanup behavior on write or rename failure remains intact.
- Test: `node_unit_write_ignores_stale_pid_only_temp_files` leaves the previous naming scheme's temp file in place and verifies that a fresh unit write succeeds without modifying it.
- Remaining risk: systemd acceptance and live process health still require an isolated host.

### APP-010 — Environment updates could misread existing values or silently discard a damaged file

- Severity: P2 / Medium
- Status: CONFIRMED; fixed
- Confidence: high
- Location: `src/node.rs`, `read_environment`/`write_environment`; `src/laravel.rs`, environment update and APP_KEY generation
- Evidence: Node parsing removed double quotes but left single quotes as part of the value. Both runtime paths treated failed UTF-8 reads or malformed lines as an empty environment during updates, so a later edit could rewrite a partial file and drop existing values.
- Implemented: Node environment parsing now decodes single- and double-quoted values and preserves `=` in values. Node and Laravel updates use a bounded, strict reader that rejects invalid UTF-8, malformed lines, invalid names and oversized files before any rewrite. Laravel APP_KEY generation uses the same fail-closed reader.
- Tests: Node quote/equal parsing and malformed-file rejection; Laravel single-quoted/export parsing and corrupted-file rejection. The test verifies original bytes remain unchanged on parse failure.
- Remaining risk: ENV values remain plain text in VPS-local files (Node 0600 with the service's access group; Laravel 0600), not encrypted at rest. There is an encrypted ARG vault primitive, but application environment management does not use it (`features.environment_secrets=false`). Adding encrypted persistence requires a tested key/decryption lifecycle for systemd/runtime processes and backup/restore.

### APP-011 — Laravel managed workers remained pinned to an older release after deployment

- Severity: P1 / High
- Status: CONFIRMED; fixed
- Confidence: high
- Location: `src/laravel.rs`, Laravel service unit generation and `laravel_refresh_services_helper`; `src/deploy.rs`, `refresh_laravel_runtime`
- Evidence: generated units used the canonical `releases/<id>` path as `WorkingDirectory`. Deployment changed the app's `current` symlink and restarted active units, but the unit still started Artisan from the old release.
- Implemented: Laravel worker units now use the stable app `current` path for working directory and writable cache. During deploy, the helper validates the active release, regenerates ARG-owned service units for the selected PHP runtime, reloads systemd and restarts active workers. Failed unit refresh restores the previous unit and reports any recovery failure.
- Tests: unit rendering asserts no release ID is embedded; a legacy release-pinned unit is migrated to `current` and active workers restart; a failed restart restores the previous unit.
- Remaining risk: actual systemd re-resolution of the symlink, process shutdown and Laravel worker behavior require a disposable VPS.

### APP-012 — Worker management is Laravel-specific and has no generic jobs supervisor

- Severity: P1 / High product gap
- Status: CONFIRMED
- Confidence: high
- Location: `src/laravel.rs`, Laravel service endpoints; application UI service controls
- Evidence: persistent services are limited to fixed Laravel Queue, Scheduler, Horizon, Reverb and Octane commands. There is no arbitrary per-app worker definition, replica control, retry/backoff editor, cron history, scheduler job history, or generic runtime worker supervisor.
- Impact: do not claim that arbitrary Node/Python/custom worker processes or per-worker advanced settings are managed.
- Plan: add an additive, app-scoped managed-process schema and helper capabilities after ownership, resource policy, log retention and deletion/recovery behavior are integration-tested. Existing Laravel units remain supported.

### APP-013 — Node runtime journal output was returned without redacting app secrets

- Severity: P2 / Medium
- Status: CONFIRMED; fixed
- Confidence: high
- Location: `src/node.rs`, `helper_service_status`
- Evidence: the helper returned raw `journalctl` output. Unlike Laravel operation output, it did not redact known values from the app's `.env`, secret assignments or URL user-info.
- Implemented: journal output is now passed through bounded sanitization before the helper response. It masks configured secret values, common secret assignment markers and HTTP(S) URL credentials while preserving the journal cursor and retention metadata.
- Test: `node_runtime_journal_logs_redact_env_values_assignments_and_url_credentials` verifies those redaction cases.
- Remaining risk: redaction cannot identify arbitrary secrets not stored in `.env` or printed in an unrecognized encoding; apps should not write credentials to logs.

### APP-014 — No application-scoped terminal is implemented

- Severity: P1 / High product gap
- Status: CONFIRMED
- Confidence: high
- Location: `src/routes.rs`, `/api/terminal`; application runtime APIs
- Evidence: terminal sessions are server/workspace scoped. No application terminal route binds a PTY to an app identity, environment and working directory. The existing app file browser is not an execution terminal.
- Impact: do not describe the global terminal as isolated to a selected app. An app terminal must not be implemented by reusing root shell access.
- Plan: add a short-lived app scope token and a restricted per-app identity/working directory only after OS user mapping, environment-secret exposure rules, PTY cleanup and websocket permission checks are tested on a disposable host.

### APP-015 — Application monitoring covers only web service CPU/RAM and storage

- Severity: P1 / High product gap
- Status: CONFIRMED
- Confidence: high
- Location: `src/app_monitoring.rs`, `collect_batch` and read API
- Evidence: the sampler selects only `node-web` and `laravel-web` units, records systemd CPU/RAM and app-volume usage, and evaluates inactive/CPU/memory thresholds. API output explicitly returns `network_bytes` and `http_latency_ms` as null. It does not measure per-worker metrics, request rates, error rates, restart counts, queue metrics or certificate expiry, and its events are not sent through the notification system.
- Impact: the charts must remain limited to measured values; do not infer request health or report zeros for unavailable metrics.
- Plan: add metrics only where their source and attribution are real (dedicated unit/cgroup or app instrumentation), then add bounded history, alert deduplication and notification delivery tests.

## Reusable components

- Existing `applications`/`application_services` registry and additive migrations.
- `deployment_jobs`, `deployment_job_steps`, `deployment_job_logs`, and `deployment_releases` as the durable job/release foundation.
- Existing idempotency and one-active-job-per-app constraints.
- `buildpacks`/runtime detection, Node and Laravel adapters, managed systemd units, Nginx manager, helper allowlist, monitoring sampler, audit utility and app storage abstractions.
- Shared dialogs, API helper, request epochs, log timeline, monitoring panels, existing workspace tabs and application detail panels.

## Compatible migration approach

1. Preserve current application UUIDs, owner UUIDs, release paths, active `current` links, SSH key references, database and volume links.
2. Add environment-scoped tables/columns only when a tested backfill can map every existing app to its existing default environment without changing runtime behavior.
3. Keep existing API fields and route behavior during transition; add explicit capability/version fields before remote operations depend on new fields.
4. Back up SQLite using the existing online backup path before schema migration; use a transaction for metadata backfill and retain the previous active release.
5. Never migrate plaintext environment values into SQLite; secrets require an independently tested encryption key lifecycle first.
6. Destructive cleanup must remain opt-in, ownership-scoped and blocked when mounts or non-ARG resources are present.

## Phase 1 and 2 exit state

The static architecture and capability audit is recorded here. Phase 2 contains the APP-001 correction and regression test. No live VPS, real Git provider, DNS, certificate authority, or external database was contacted. Host-level lifecycle tests remain blocked until an isolated Debian/Ubuntu environment is available.

### Quality Gate 1 (phases 1 and 2)

| Check | Result | Evidence |
|---|---|---|
| `cargo fmt --all -- --check` | Pass | Workspace formatter applied in standalone commit `f122af9`; check subsequently passed. |
| `cargo check --locked` | Pass | Completed successfully after formatting. |
| `cargo test --locked` | Pass | 270 tests passed after the APP-001 regression was added. |
| Frontend `npm run check` | Pass | TypeScript check completed. |
| Frontend `npm run test:offline` | Pass | 10 tests passed. |
| Frontend `npm run build` | Pass | Vite production build completed; reports the existing >500 kB chunk advisory. |
| Migration/CRUD and privilege regressions | Partial | Existing additive migration and authorization suites passed; no separate live upgrade database or host process was available. |
| systemd/Nginx/SSL/real deployment | Blocked | This workspace is not an isolated VPS; no host mutation was performed. |

The frontend bundle-size warning is not a build failure. Do not treat blocked host-level scenarios as verified.

## Phase 3 and 4 review

### Phase 3 — Persistent deployment coordination

The current implementation has durable `deployment_jobs`, ordered `deployment_job_steps`, persisted logs and releases; idempotency is unique per application/key; a partial unique index prevents concurrent active jobs per application; the queue is capped at 64; cancellation, retries, startup recovery, release activation and rollback have existing tests. The worker intentionally processes one job at a time globally, which is safe but limits throughput. Job status remains compatibility-oriented (`queued`, `validating`, `running`, `verifying`, terminal), with detailed stage state in the steps table; it is not the full expanded enum from the proposal. No schema rewrite was made in this phase.

### Phase 4 — Runtime detection and Build Plan

The detector already supports Laravel, Next, Nuxt, React/Vite, Vue/Vite, Node, Python, static and manual Custom selection. The plan now respects the same package-manager rules as the Node runtime, avoids suggesting a missing build script, refuses to follow symlinked manifests, reports invalid manifests, and accepts an explicit project root for monorepos. Dockerfile/Compose plans remain analysis-only.

Native PHP execution, complete framework-specific adapters, and a create-application wizard spanning source through deploy remain unimplemented. These are recorded as product gaps rather than active-looking controls.

### Quality Gate 2 (phases 3 and 4)

| Check | Result | Evidence |
|---|---|---|
| Rust format/check/tests | Pass | `cargo fmt --all -- --check`, `cargo check --locked`, and `cargo test --locked`; 276 tests passed. |
| Frontend TypeScript | Pass | `npm run check`. |
| Frontend offline tests | Pass | 10 tests passed. |
| Frontend production build | Pass | Vite build completed with the existing large-chunk advisory. |
| Actual systemd, Nginx, SSL and service health | Blocked | No disposable VPS was available; production hosts were not contacted or modified. |

The project-root field uses the existing bounded relative-directory API. The selected path is stored with the Build Plan and remains analysis-only. No runtime deployment was triggered by these tests.

## Phase 5 and 6 review

### Phase 5 — Source and Git

Existing source methods are Git, ZIP archive and a restricted VPS directory. Public HTTPS Git supports four allowlisted providers; private Git supports GitHub SSH Deploy Keys only. Branch, tag or commit is entered as a bounded reference string and the fetched commit is recorded. Archive imports cap upload size at 256 MiB, expanded release size at 2 GiB and entries at 20,000; ZIP slip, symlinks and special files are rejected. Existing deploy key files and host-key cache use restrictive file modes and key/repository/app scoping.

There is no webhook/push-to-deploy, auto-deploy policy, repository history/diff browser, provider token manager or support for private GitLab/Bitbucket/Codeberg. These are open APP-006 work; no mock buttons were added.

### Phase 6 — Build and custom commands

Node install/build is selected from a supported lockfile and runs in a systemd transient unit with `DynamicUser`, `ProtectSystem=strict`, `ProtectHome`, `NoNewPrivileges`, bounded memory/CPU/tasks/runtime, and no application secrets in the build environment. The pre-deploy command editor tokenizes into argv and does not invoke a shell; it is bounded to eight commands, 512 bytes each, 4 KiB total and 32 argv entries. Build and deploy output has bounded retention and common credential redaction.

The requested complete multi-stage configurable pipeline remains blocked by APP-007. Do not claim custom pre-deploy is equivalent to pre-build/post-build/post-deploy lifecycle hooks.

### Quality Gate 3 (phases 5 and 6)

| Check | Result | Evidence |
|---|---|---|
| SSH key scope regression | Pass | `deploy_keys_are_bound_to_their_repository_and_application` rejects both cross-app and cross-repository reuse. |
| Rust format/check/tests | Pass | `cargo fmt --all -- --check`, `cargo check --locked`, and `cargo test --locked`; 277 tests passed. |
| Frontend TypeScript/offline tests/build | Pass | TypeScript check, 10 offline tests and production build all passed. |
| Real provider clone, sandboxed build, timeout/cancel on systemd | Blocked | No disposable VPS and no isolated fake Git provider were available. No external repository was contacted. |

## Phase 7 and 8 review

### Phase 7 — Runtime and managed process behavior

Node SSR services use one ARG-managed systemd unit per app, `DynamicUser`, read-only release and environment mounts, a dedicated writable data mount, loopback bind addresses, restart-on-failure, bounded tasks/CPU/RAM, group-wide termination and a real HTTP health check before the helper reports success. Laravel queue/scheduler/optional Horizon/Reverb/Octane units run as the restricted `arg` account with fixed Artisan commands, loopback ports for network services and a 120-second graceful stop. The actual systemd lifecycle cannot be verified in this workspace.

The phase found and fixed APP-008 and APP-009. Laravel unit recovery now retains the prior enabled state, reports restoration failures, and also recovers if the first `daemon-reload` fails. Node unit replacement temp names are unique per write, so abandoned temp files no longer block later installs.

### Phase 8 — Domain, Nginx and SSL

Application deployment checks the actual domain resolution, prepares the application service, then creates or validates the ARG-managed Nginx virtual host. If no matching certificate exists, the explicit “complete website setup” flow invokes the existing Nginx SSL operation. That operation checks site/domain ownership and conflicts, backs up the Nginx file, invokes bounded Certbot with the Nginx plugin, validates `nginx -t`, reloads Nginx and restores the site config on failure. Certbot owns renewal configuration. Nginx site writes use atomic replacement and validate before reload. Existing site configs are preserved during deployment when another site already owns a hostname.

No VPS, ACME server, DNS zone or live certificate was contacted. Certbot issuance, renewal timer behavior, external HTTPS reachability and rollback under a real Nginx failure remain blocked. The current code verifies Certbot completion plus Nginx syntax/reload; it does not perform a separate post-apply public TLS probe. DNS-01 and wildcard certificates are not supported by this HTTP-01 flow.

### Quality Gate 4 (phases 7 and 8)

| Check | Result | Evidence |
|---|---|---|
| Targeted runtime regressions | Pass | `node_unit_write_ignores_stale_pid_only_temp_files`; both Laravel service rollback tests. |
| Rust format/check/tests | Pass | `cargo fmt --all -- --check`, `cargo check --locked`, `cargo test --locked`; 280 tests passed. |
| Frontend TypeScript | Pass | `npm run check`. |
| Frontend offline tests | Pass | 10 tests passed. |
| Frontend production build | Pass | Vite production build completed; the existing >500 kB main-chunk advisory remains. |
| Live systemd, Nginx, DNS, ACME, renewal and HTTPS | Blocked | No isolated VPS is attached; no production host or live provider was changed or contacted. |

## Phase 9 and 10 review

### Phase 9 — Environment variables and secret handling

Node and Laravel store each app's persistent environment in a per-app `shared/.env` file, enforce size/key/value bounds, use atomic replacement, restrict mode, avoid returning values, and redact common secret assignments in operation logs. Laravel's APP_KEY generation is one-time and now refuses to edit a file it cannot parse safely. Node runtime loads its environment from the app's shared file; browser-build variables are separately restricted to public prefixes.

The phase found and fixed APP-010: parse existing single-quoted values correctly and reject unreadable/corrupt environment files before saving so a routine update cannot silently erase unrelated values.

Application ENV is not encrypted at rest and does not have history/restore or compare support. `secret_vault` is not integrated with app ENV. A transparent migration to encrypted-at-rest secrets is deferred until runtime decryption, systemd credentials, backup/restore and key rotation can be tested together; plaintext values are not copied into the database.

### Phase 10 — Production/Staging isolation

Projects and environments are persistent workspace metadata, seeded additively, with ownership and protected-default constraints. Applications can be assigned to an environment, but deployment job, release, environment file, process, port and domain ownership remain keyed to the application. Therefore Production/Staging do not have isolated runtime state and environment labels must not be presented as an isolation boundary. APP-002 is confirmed as a product gap. No schema change was made because a safe backfill requires an environment-to-runtime mapping and independent persistent storage.

Promotion, per-environment domains/ports/secrets, DB/queue/storage isolation, staging auth and independent rollback are not complete. No UI capability is claimed for these behaviors.

### Quality Gate 5 (phases 9 and 10)

| Check | Result | Evidence |
|---|---|---|
| Targeted environment regressions | Pass | Quote/equal parsing, invalid UTF-8, malformed lines, and fail-closed preservation tests for Node and Laravel. |
| Rust format/check/tests | Pass | `cargo fmt --all -- --check`, `cargo check --locked`, `cargo test --locked`; 284 tests passed. |
| Frontend TypeScript | Pass | `npm run check`. |
| Frontend offline tests | Pass | 10 tests passed. |
| Frontend production build | Pass | Vite build completed; existing >500 kB main-chunk advisory remains. |
| Database migration and isolated environment runtime checks | Partial/Blocked | No schema migration was introduced. Full production/staging lifecycle, encrypted secret delivery and key recovery need an isolated VPS and were not claimed as verified. |

## Phase 11 and 12 review

### Phase 11 — Laravel deployment runner

The current Laravel adapter detects Laravel, validates the PHP/Composer runtime, installs dependencies from `composer.lock` without Composer plugins/scripts, preserves `.env` and storage outside releases, checks the `public` document root, manages PHP-FPM/Nginx through the restricted helper, supports fixed Artisan operations and requires explicit confirmation for migrations. Migration is not silently run as part of deployment; rollback of code does not roll back a database migration.

The phase found and fixed APP-011: Laravel workers now follow the new `current` release after every deploy, including migration of previously generated pinned-path unit files. Unit replacement is atomic and recovers the prior unit when reload/restart fails.

Automatic migration policy/preflight, schema compatibility detection, failed migration recovery UI, Horizon-specific integration tests and live PHP-FPM validation are not complete. These stay explicit limitations rather than implicit deploy behavior.

### Phase 12 — Persistent workers and jobs

Laravel Queue, Scheduler, Horizon, Reverb and Octane can be represented by independent ARG-owned systemd units and run independently of the browser. Queue and Horizon are prevented from being enabled simultaneously. Existing units restart after release changes and are deleted only through ownership-scoped cleanup. APP-012 records the boundary: the system has no generic worker/cron model, replica/backoff controls, persistent per-run scheduler history or arbitrary runtime workers. Service logs and process lifecycle need real host verification.

### Quality Gate 6 (phases 11 and 12)

| Check | Result | Evidence |
|---|---|---|
| Targeted worker refresh regressions | Pass | Stable `current` unit rendering; legacy unit migration/restart; restoration of the previous unit after a failed restart. |
| Rust format/check/tests | Pass | `cargo fmt --all -- --check`, `cargo check --locked`, `cargo test --locked`; 287 tests passed. |
| Frontend TypeScript | Pass | `npm run check`. |
| Frontend offline tests | Pass | 10 tests passed. |
| Frontend production build | Pass | Vite build completed; existing >500 kB main-chunk advisory remains. |
| Real Laravel, systemd, PHP-FPM, queue and server-restart scenarios | Blocked | No disposable PHP/systemd host is attached. The local tests exercise unit generation/recovery only, not OS behavior. |

## Phase 13 and 14 review

### Phase 13 — Monitoring and observability

The sampler runs every 60 seconds, processes up to 20 app candidates per pass, retains 30 days of samples and 365 days of alert lifecycle events. Current per-app measurements are managed web-unit state, CPU time delta, memory and volume usage. Unknown measurements stay null/unknown and do not resolve an existing threshold alert. Alerts cover inactive expected service, CPU and RAM; acknowledgement is persisted.

The implementation does not yet monitor arbitrary workers, HTTP request/latency/error rate, network bytes, certificate expiry, queue health, process restart counts or notification delivery. This is recorded as APP-015. Runtime Node journald was unredacted before this phase; APP-013 adds redaction before logs leave the helper.

### Phase 14 — Application terminal and isolation

There is no app-scoped terminal. Existing terminals are for the selected server/workspace and administrator. App browser/log features do not provide a restricted PTY. The requested app identity, limited shell, session-token binding, reconnect/cleanup and app-level authorization therefore remain unimplemented (APP-014). No root shell was exposed as a substitute.

### Quality Gate 7 (phases 13 and 14)

| Check | Result | Evidence |
|---|---|---|
| Runtime log redaction regression | Pass | `node_runtime_journal_logs_redact_env_values_assignments_and_url_credentials`. |
| Rust format/check/tests | Pass | `cargo fmt --all -- --check`, `cargo check --locked`, `cargo test --locked`; 288 tests passed. |
| Frontend TypeScript | Pass | `npm run check`. |
| Frontend offline tests | Pass | 10 tests passed. |
| Frontend production build | Pass | Vite build completed; existing >500 kB main-chunk advisory remains. |
| Host metrics, notification delivery, PTY isolation and cross-app access tests | Blocked | No isolated host/PTY service environment is attached; no terminal implementation was added to falsely claim app isolation. |
