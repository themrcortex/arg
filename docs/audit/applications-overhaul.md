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
