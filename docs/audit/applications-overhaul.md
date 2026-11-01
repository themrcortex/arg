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

- Severity: P1 / High product limitation; no exploit established
- Status: AUDIT_REQUIRED
- Evidence: app records can be assigned to workspace environment IDs, while deployment and runtime ownership/configuration are still primarily keyed by application ID. Existing wording in the UI says environment assignment does not copy or isolate configuration.
- Impact: do not represent the current environment selector as independent Production/Staging secrets, processes, domains or deployment histories.
- Plan: retain existing IDs and defaults; introduce only additive environment-scoped records after cross-module ownership and cleanup rules are specified and migration-tested.

### APP-003 — Queue/process robustness needs deployment-specific integration validation

- Severity: P2 / Medium
- Status: AUDIT_REQUIRED
- Evidence: persistent leases and startup recovery exist; unit tests cover interruption and child-process cancellation. This workspace has no isolated systemd/Nginx VPS to validate real unit lifecycle, cancellation of grandchildren, or rollback after an actual service failure.
- Plan: keep claims bounded to covered behavior and mark host-level scenarios BLOCKED until an isolated Linux test host is available.

No P0/P1 security exploit was established by this phase. The points above must not be treated as proof that every helper path, provider, or remote-node scenario is safe; those require their phase-specific tests.

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
| `cargo test --locked` | Pass | 269 tests passed before the APP-001 regression was added; rerun after that change is recorded in the phase 2 commit/review. |
| Frontend `npm run check` | Pass | TypeScript check completed. |
| Frontend `npm run test:offline` | Pass | 10 tests passed. |
| Frontend `npm run build` | Pass | Vite production build completed; reports the existing >500 kB chunk advisory. |
| Migration/CRUD and privilege regressions | Partial | Existing additive migration and authorization suites passed; no separate live upgrade database or host process was available. |
| systemd/Nginx/SSL/real deployment | Blocked | This workspace is not an isolated VPS; no host mutation was performed. |

The frontend bundle-size warning is not a build failure. Do not treat blocked host-level scenarios as verified.
