# Application Platform Architecture

This document records the Phase 00 baseline and architecture decisions for extending ARG from host administration to application lifecycle management. It describes the current repository, the intended ownership model, and constraints that later phases must preserve. Planned capabilities are not claims about features already shipped.

## Repository baseline and additions

ARG is a single installable Rust/Axum service with a React/TypeScript/Vite interface embedded in its release binary. SQLite stores panel accounts, sessions, settings, audit records, metrics, and feature-specific metadata. Host operations that need elevated privileges cross a restricted Unix-socket helper boundary.

The original Phase 00 inventory covered `src/federation.rs`, `src/nginx.rs`, `src/docker.rs`, `src/s3/`, `src/files.rs`, `src/metrics.rs`, and `src/maintenance.rs`. Federation can pair ARG instances, read remote status and services, and proxy an authenticated workspace to a selected remote node. Nginx sites, Docker resources, and S3 buckets remain resources of their hosting server. Phases 03–08 have since added the Application registry, durable deployment engine, runtime manager, Laravel and Node.js runners, and static/SSR delivery; the detailed shipped boundary is recorded in the phase status below.

The frontend build is embedded from `frontend/dist`. SQLite bootstrap remains in `src/main.rs`; application, deployment, runtime, Laravel, and Node domain code is separated into `src/apps.rs`, `src/deploy.rs`, `src/runtimes.rs`, `src/laravel.rs`, and `src/node.rs`. Future schema changes must use additive, idempotent migrations and preserve existing user, session, node, S3, Nginx, Docker, and application data. The current Federation Node protocol is `arg-node/2`; protocol compatibility must be explicitly reviewed before changing it.

The panel remains useful on one VPS with no paired node and no Docker daemon. Optional dependencies must remain optional. Missing Docker, PHP, Node.js, S3, or network capabilities must produce an accurate unavailable state, not a fabricated healthy state.

## Architecture decision record: local control plane and remote servers

### Decision

Each ARG installation remains a complete standalone control plane for its own host. A paired ARG server may be managed remotely through authenticated, explicitly scoped Federation operations. The initial architecture does not require a separate agent package when ARG is already installed on the managed VPS.

The controller provides navigation and request routing; the host that owns a resource remains the source of truth for that resource. A controller stores pairing metadata and only the remote state needed for routing or a clearly marked last-known summary. It must not silently mirror a remote database or present stale cached values as live.

### Identity and display names

Every installation has an immutable `server_uuid`. The operating-system hostname is observed metadata, not a network identity and not something ARG changes as a side effect. A `public_name` is the administrator-selected name shared with paired controllers. A `local_label` is a controller-local alias and is never sent to peers. The UI must identify which name is being edited and show the target server before any write operation.

Existing pairings migrate additively without changing their node IDs or revoking valid credentials. Protocol-v2 pairing metadata now carries an optional stable server UUID and public name, and an authenticated endpoint synchronizes public-name changes to paired controllers. Older peers remain usable through a `legacy:<local-id>` workspace key until they report a stable UUID. Controller-local labels remain local and are never pushed to peers.

### Phase 01 implementation status

- New installations lazily persist a UUID in SQLite settings; existing pairings gain nullable identity fields without being recreated.
- A server's public name is configured independently of its OS hostname. The receiving controller keeps its own local label separately.
- Single-server and multi-server modes are UI preferences; changing modes does not delete pairings. Multi-server mode adds Local, a read-only All Servers overview, and selectable remote workspaces.
- Remote workspace selection is stored in `sessionStorage` and routed by stable server UUID. Existing numeric selections are migrated when the Node list is loaded; older peers use an explicit legacy key. Logout clears the active workspace, and switching contexts clears heavy cached data.
- Workspace proxy and terminal routes resolve only a valid UUID or an explicit legacy key. Admin Node-management routes continue using local numeric database IDs.
- A public-name change is pushed to paired servers over the existing authenticated HTTPS Federation channel; partial connectivity is reported to the administrator.
- The All Servers overview checks each remote on demand, labels failed checks unavailable, and opens an explicit server workspace before management. No operation is rerouted to a different server on failure.
- The Phase 01 Rust migration and route tests are present. Run them with `cargo test --locked` in an environment with the Rust toolchain.

### Phase 02 implementation status

- Federation roles are `standalone`, `member`, `controller-capable`, and `both`. Role changes require confirmation and are rejected while they would strand paired nodes or controllers.
- Pairing codes grant either a read-only or full scope set. The destination stores and checks scopes for each controller; the controller also checks the grant before routing a remote request.
- The destination applies a scope guard to remote workspace sessions. Reads, writes, terminal access, and root-terminal authorization have separate scopes. A legacy session is migrated to read-only scope instead of retaining its former unrestricted admin session.
- Capability version 1 is exchanged during pairing. Nodes without compatible workspace-proxy or terminal capabilities stay explicitly legacy/read-only and receive `Upgrade Required` for unsupported actions.
- Controller credentials can be rotated with a pending-secret retry protocol. Workspace sessions expire after 12 hours and are renewed before use. Revocation deletes the controller and its associated remote sessions.
- Remote calls use HTTPS with certificate verification, redirects disabled, bounded timeouts, DNS resolution checks, and pinned resolved addresses for HTTP requests. Private network targets require explicit opt-in; loopback, link-local, and metadata targets are rejected.
- The controller records `healthy`, `degraded`, `offline`, `untrusted`, and `incompatible` status plus the last check and a sanitized error. Remote writes are audited at the destination under the paired controller identity.
- The `development` branch passed Rust tests for role/scope checks, session permission routing, private-address classification, endpoint validation, and additive migrations. A real two-VPS TLS integration run is still required before claiming end-to-end coverage.

### Phase 03 implementation status

- Added a server-owned Application registry with UUID identity, framework/source metadata, labels, domain and port reservations, desired versus observed state, soft deletion, restore, and optimistic version checks.
- Added a separate `application_services` schema for future process/resource adapters. It is not an execution API yet.
- The Applications page supports local management and read-only aggregation across ready remote workspaces. Offline nodes are reported individually; mutations happen in the owner's workspace.
- Domain validation checks application records and live Nginx `server_name` values through the privileged manager. If Nginx is present but cannot be inspected, a domain write fails closed. TCP port reservations are checked against application records and current kernel listener tables.
- Registry records do not fetch source, build code, install runtimes, create secrets, or claim a process is running. `observed_state` remains `unknown` until a real adapter is implemented.
- Additive migration and validation tests pass with `cargo test --locked`; frontend type checking/build and documentation checks pass. A real multi-VPS integration test remains outstanding.

## Phase progress (2026-10-27)

| Phase | Status in `development` | Verification boundary |
| --- | --- | --- |
| 00 — Baseline and architecture | Complete | Inventory, ADR, compatibility rules, and test matrix are documented. |
| 01 — Server identity and workspace switcher | Complete | Automated migration/routing tests exist; a real multi-VPS run remains external validation. |
| 02 — Federation roles and security | Implemented | Automated scope, endpoint, migration, and authorization checks exist; two-VPS TLS/replay integration remains outstanding. |
| 03 — Application registry | Complete | CRUD, ownership, collision checks, migrations, and frontend checks are implemented; live multi-VPS aggregation still needs a paired-server run. |
| 04 — Durable deployment jobs | Complete | Persistent jobs, bounded source acquisition, release activation, cancellation, retry, retention, and rollback are implemented and covered by repository tests. |
| 05 — Runtime manager | Complete | Runtime inventory, safe APT previews, per-app preferences, and frontend controls are implemented; package installation still needs a disposable Debian/Ubuntu host test. |
| 06 — Laravel runner | Automated phase gate passed; host integration remains | Web/PHP-FPM, database provisioning, Artisan, optional services, and release refresh/rollback are implemented. Rust tests, release build, frontend type check, and frontend build pass. A disposable-host Nginx/MySQL/systemd integration run remains outstanding. |
| 07 — Node.js runner | Automated phase gate passed; host integration remains | Lockfile-based npm/pnpm/Yarn Classic build, bounded systemd build unit, private runtime environment, per-app DynamicUser service, loopback/HTTP checks, guarded Nginx proxy, and deployment recovery are implemented. Repository tests and builds pass; a disposable-host Node/systemd/Nginx integration run remains outstanding. |
| 08 — Static and SSR delivery | Automated phase gate passed; host validation remains | Auto/manual React, Vue, Vite, Next, and Nuxt handling, static Nginx output, fallback and safe caching, restricted SSR, and rollback synchronization are implemented. Repository tests/builds pass; disposable-host Nginx/systemd validation remains. |
| 09 — Application networks | Repository implementation added; host test remains | Internal Docker bridge creation and membership for ARG-labeled Docker apps, host-local Node endpoints, ownership checks, and delete-in-use protection are implemented. A real Docker network/isolation test remains. |
| 10 — Persistent app data | Repository implementation added; host test remains | Stable per-app Node data directories, bounded on-demand usage scans, retained detach, and explicitly confirmed purge are implemented. Quotas are informational; filesystem enforcement is not configured. |
| 11 — App S3 bindings | Metadata and mount references implemented | App-scoped Bucket/prefix/purpose records reuse existing S3 connections and block deleting referenced Buckets. ARG does not distribute credentials or enforce provider ACLs; configure least-privilege access at the S3 provider. |
| 12 — Application logs | Persistent journal reads and cursor resume implemented | Node unit logs can be read in bounded pages and resumed from a journal cursor. Retention follows the host-wide journald policy; per-app retention controls are not implemented. |
| 13 — Application monitoring | Repository implementation and automated phase gate passed; host test remains | Bounded systemd service sampling, CPU/RAM/state history, per-app rules/alerts, admin APIs, and a collapsed panel are implemented. HTTP/network metrics and disposable-host validation remain open. |
| 14 — Cross-VPS app network | Design documented; not implemented | Requires a separate overlay data plane and real two-host route/revocation tests. Federation HTTPS alone does not satisfy this phase. |
| 15 — App terminal/files/source | Planned | Must use app-scoped non-root identities and rooted file access; global server/root tools do not satisfy this phase. |
| 16 — App migration | Planned | Requires durable preflight, verified transfer/restore, fencing, explicit cutover, rollback, and two-VPS tests. |
| 17 — Webhooks and backups | Planned | Requires signed/replay-safe hooks and encrypted backup restore drills; upload-only is not backup verification. |
| 18 — Release hardening | Planned | Requires host and multi-host matrices, threat review, performance evidence, and operational runbooks. |

“Complete” here means the repository implementation and its automated phase checks are present. It does not claim that external multi-VPS, certificate-authority, APT, MySQL, or systemd integration has been exercised on a production VPS.

### Phase 04 implementation status

- Deployment jobs persist state and step history, use leases/heartbeats for recovery, and expose bounded progress/history rather than relying on a browser request lifetime.
- Git, ZIP, and approved directory sources have explicit validation and bounds. Releases are staged separately, activated through an atomic `current` pointer, and retained with the active/previous releases.
- Cancellation, retry, restart recovery, rollback, source pinning, and cleanup are guarded by job/application ownership and audited APIs.
- Repository tests cover interrupted work and release-pointer behavior. Low-disk/crash behavior on an actual host remains an integration gate.

### Phase 05 implementation status

- Runtime inventory reports installed PHP CLI/FPM, Composer, Node.js and package managers, Nginx, and Docker with host preflight context.
- APT package operations show a simulation before changes. ARG does not add an unverified third-party repository or run a remote shell installer.
- Application runtime/build preferences are stored separately and validated against installed versions; missing optional runtimes remain explicit.
- Tests cover inventory parsing and preference validation. Installing/removing packages needs an isolated Debian/Ubuntu host test before general availability.

### Phase 06 implementation status

- Laravel source inspection, PHP requirement checks, persistent `.env`/`storage`, lockfile-only Composer install, redacted environment handling, and an allowlisted Artisan runner are implemented.
- Web activation creates a dedicated PHP-FPM pool and managed Nginx site, validates both configurations, and restores prior configuration on failure. MySQL provisioning creates a per-app database user with database-scoped grants and stores its random password only in the app's private environment file.
- Queue, Scheduler, Horizon, Reverb, and Octane use fixed systemd unit templates. Reverb/Octane use loopback ports; Octane switches the site's Nginx upstream and restores PHP-FPM mode when disabled.
- Deploy and manual rollback refresh active Laravel web/services and attempt to restore the prior release/runtime on failure. This does not roll back database migrations.
- The current change set adds PHP-FPM `system` resolution, service/port conflict checks, MySQL account cleanup, and safer release recovery. The workspace passed 105 Rust tests, `cargo build --locked --release`, frontend type checking, and frontend production build. Actual Nginx, MySQL and systemd behavior still requires a disposable host.

### Phase 07 implementation status

- Node projects are inspected from `package.json` and exactly one supported lockfile. npm, pnpm, and Yarn Classic use frozen/reproducible install modes; unsupported or conflicting managers fail explicitly. A `start` script is required; `build` is optional.
- Dependency installation and build scripts run in transient systemd units using a DynamicUser, private temporary storage, restricted filesystem access, fixed resource/time/output bounds, and no access to ARG's private data. A build runs only when a deployment job targets a Node app.
- Each Node app gets a UUID-derived systemd unit, private `.env` file, read-only release/environment mounts, a dedicated app group, writable `shared/data`, fixed loopback host/port, resource ceilings, restart-on-failure, and a graceful-stop timeout. Secret values are never read back through the API.
- Activation verifies the real listener is loopback-only and sends an HTTP request before success. When a domain is configured, a separate ARG-managed Nginx proxy is added only after collision checks; unrelated or manually modified sites are not overwritten. Certificate issuance remains an explicit Nginx action.
- Deployment and manual rollback restart only a service that was already enabled. Failed activation attempts to restore the prior pointer and service state. This does not undo database or external side effects performed by app code.
- The phase gate passed 115 Rust tests and frontend type-check/build in the workspace. Release and documentation builds are part of this phase's final gate. Actual npm ecosystem builds and systemd/Nginx lifecycle checks need a disposable Debian/Ubuntu host and are not represented as passed by unit tests.

### Phase 08 implementation status — static sites and SSR

- The registry accepts `auto`, `static`, `react`, `vue`, `vite`, `node`, `next`, and `nuxt`. Auto mode detects from the package manifest; a selected framework provides an explicit delivery override.
- React/Vue/Vite and prebuilt directories with `index.html` publish through Nginx without a Node web process. The app stores its output path, SPA fallback, and static publication settings. Build output is constrained and validated before activation.
- Build-time variables are stored separately, accept framework-public prefixes only, and are passed only to the build step; the 16 KiB limit and secret-like-name checks apply. The UI warns that values can become part of public bundles. Runtime variables remain in a separate private file.
- Next.js/Nuxt and generic Node apps use the SSR path. The standard Next CLI and Nuxt `.output/server/index.mjs` entrypoints are supported; NITRO host/port are set to loopback and the restricted systemd service starts only after an explicit enable.
- Each static Nginx site is named for its app. Existing unrelated domain owners are not overwritten. A failed Nginx activation restores the release pointer; deploy and rollback synchronize Nginx root with the selected release output.
- Automated coverage checks static-vs-SSR classification, prebuilt root/subdirectory output, traversal rejection, Nuxt entrypoint selection, SPA fallback, and Nginx rules. Real Debian/Ubuntu Nginx/systemd validation remains a separate gate.

### Phases 09–12 implementation boundary

- Application networks are server-local. Docker bridge networks are created `--internal` and labeled with their ARG record ID. The helper only attaches a container when Docker confirms its `com.arg.application_id` label matches the selected application. Native Node services use their configured loopback port; this is service discovery metadata, not isolation.
- Persistent Node data is created under `/var/lib/arg/deployments/apps/<app-id>/shared/data/volumes/<name>` and appears inside the Node systemd sandbox at `/shared/data/volumes/<name>`. Scans are explicit and bounded. Detaching a record keeps files; purging requires a separate confirmation and refuses a separate mounted filesystem. The stored quota is informational and is not enforced by the kernel.
- App S3 bindings reference existing ARG Buckets and prefixes. They never return credentials. Prefix and access mode are metadata; actual permission boundaries must be configured in the provider's access key or policy. Referenced Buckets cannot be detached until app bindings are removed.
- Node process output uses the systemd journal. API reads are capped at 500 lines and can resume after a journal cursor. Journal retention is system-wide and configured by the host; ARG does not expose a misleading per-app retention switch. This phase does not provide a separate log collector for Docker, Nginx, jobs, or S3 transfers.
- Automated tests and builds validate code paths and schema migrations. Docker isolation, group permissions, journal cursor behavior, and filesystem lifecycle need a disposable Debian/Ubuntu integration host before production claims.

### Phases 13–18 roadmap

Implementation order, API and state boundaries, acceptance gates, and external test requirements for the remaining phases are documented in [Application platform roadmap: Phases 13–18](app-platform-roadmap-13-18.md). The status table distinguishes repository work from host and multi-host validation.

### Vocabulary and ownership

| Term | Meaning |
| --- | --- |
| Server | One ARG installation and the Linux host it administers; identified by `server_uuid`. |
| Controller | An ARG installation sending an authenticated request to a paired server. |
| Managed server | The paired ARG installation that authenticates and executes a scoped request locally. |
| Workspace | The current UI/request context: local server or one explicitly selected remote server. |
| Application | A deployable project owned by exactly one server. |
| Service | A process or container belonging to an application, such as a web process, worker, or database. |
| Environment | An application configuration scope such as development, staging, or production. |
| Resource adapter | A controlled integration with an existing host resource, such as an Nginx site, Docker container, systemd unit, database, or S3 bucket. |

An Nginx site or Docker container can be linked to an Application, but must not be duplicated or silently adopted. Importing an existing resource requires an explicit ownership/adoption flow and conflict checks.

### Routing and authorization

- Every remote read or write has an explicit target `server_uuid`; the UI's selected workspace is not by itself authorization.
- “All servers” is an aggregation/read view. A mutation from that view must require an explicit destination.
- The controller authenticates the user locally, but the destination independently validates the controller credential, actor, capability, scope, and requested operation.
- Browser code must never receive or persist a raw Federation node token or remote workspace credential.
- Remote tokens have bounded lifetimes or rotation, immediate revocation, endpoint validation, and destination-side audit. Workspace sessions expire after 12 hours and renew through the authenticated controller channel; retryable credential rotation preserves a pending credential on the controller.
- Protocol negotiation advertises supported capabilities and version ranges. If a capability or safe compatibility cannot be established, the operation is unavailable; do not silently downgrade to a broader operation.
- Remote requests use bounded payloads/timeouts, idempotency keys for retryable writes, and structured error responses. Streaming must have backpressure and cancellation behavior.

### Privilege boundary

Browser → authenticated Axum API → validated domain operation → restricted helper over Unix socket when root is needed. Application build commands and project code are untrusted input. They must run under an application-specific unprivileged identity and bounded execution context; they must never be passed as user-constructed shell strings to the privileged helper. Helpers accept structured allowlisted operations and validate them independently.

### Application and job ownership

Application identity and deployment state are owned by the server where the app runs. The controller may route to that server but must not create a second authoritative copy. Long-running operations are represented as durable jobs with a target server, actor, state transitions, bounded logs, cancellation semantics, idempotency, and recovery after service restart. UI state is derived from the job record and host observations, not from the lifetime of an HTTP request.

### Storage and migration rules

- All persistent data is explicitly server-scoped. Selecting another workspace does not move or copy files, volumes, databases, or secrets.
- S3 is object storage. A mount is an optional compatibility layer, not a POSIX guarantee; database files and workloads requiring reliable POSIX locks must not be placed on S3 mounts.
- A cross-server application move is a migration workflow, not an ID update: preflight, snapshot, transfer, restore, verify, cutover, and cleanup are separate steps.
- Rollback guarantees end at a documented cutover boundary. Once the destination accepts writes, returning to the source may lose writes unless reverse synchronization or a maintenance window is used.
- SQLite migrations are additive, idempotent, backed up before risky transformations, tested against both fresh and existing databases, and preserve unrelated ARG records.

## Phase gates and test matrix

At every phase gate, run an executable baseline for frontend checks/build, Rust tests/build, shell syntax, docs validation, and version consistency. The phase-by-phase results and the remaining host-integration limits are recorded in the status table above. Automated tests do not replace a real two-VPS integration test.

| Environment | Required coverage |
| --- | --- |
| One VPS, no Docker | Install/upgrade compatibility, local management, no accidental remote dependency. |
| One VPS with Docker and runtimes | Resource ownership, service state, storage, logs, and optional dependency behavior. |
| Two paired VPSs | Pair, switch workspace, route read/write, revoke, identity sync, and audit on both sides. |
| Remote VPS offline | Stale-state labeling, timeout, retry, no ambiguous writes, and recovery after reconnect. |
| Interrupted/low-disk operation | Durable job recovery, cancellation, rollback boundary, and preservation of user data. |
| Mixed protocol versions | Capability negotiation, explicit unsupported state, and no unsafe fallback. |

Every phase must add the tests for the behavior it changes. The final release matrix is not a substitute for phase-level tests. Security-sensitive operations require authorization tests at the API boundary, not only UI tests.

## Phase 00 repository inventory

- **Backend:** Rust/Axum. Route registration is in `src/routes.rs`; major domain modules include Federation, Nginx, Docker, S3, files, MySQL, metrics, and maintenance.
- **Frontend:** React, TypeScript, Vite, Tailwind, and Radix/shadcn-style components. Navigation is defined in `frontend/src/navigation.ts` and shell behavior in `frontend/src/App.tsx`.
- **Persistence:** SQLite bootstrap is in `src/main.rs`; the Application schema migration is isolated in `src/apps.rs` and invoked at startup.
- **Privileged operations:** ARG service communicates with `arg-privileged.service` over a private Unix socket. No feature may turn this into a generic root command runner.
- **Federation:** protocol v2 and remote workspace API/terminal proxy are implemented. Phase 02 adds server roles, destination-checked scopes, capability negotiation v1, workspace renewal, credential rotation, and health states; a real two-VPS integration test is still required.
- **Application platform:** Phases 03–08 add the ownership-aware registry, durable deployments, runtime inventory, Laravel/Node runners, and static/SSR delivery. Phases 09–12 add server-local app networks, persistent Node data, S3 references, and resumable journal reads. Phase 13 monitoring is under implementation. The design and acceptance gates for Phases 13–18 are in the [remaining-phase roadmap](app-platform-roadmap-13-18.md); cross-VPS data networking and migration still require dedicated implementation and real multi-host tests.
- **CI:** GitHub Actions workflows exist for CI and documentation publishing. Local required checks are documented in `docs/en/development.md`.
- **Versioning:** Rust and frontend versions are synchronized; stable and prerelease updates use `scripts/bump-version.sh`, `scripts/bump-development-version.sh`, and `scripts/check-versioning.sh`.

## Decisions that must be revisited before implementation

1. Whether the first application runner is native systemd, container-only, or a per-app adapter supporting both. Do not promise isolation stronger than the selected execution boundary provides.
2. How existing Nginx sites and Docker containers are discovered, imported, and protected from duplicate ownership.
3. How session and credential rotation will migrate existing Federation pairings without breaking access or leaving stale long-lived sessions.
4. Which first release target is supported (Ubuntu/Debian versions, architectures, and minimum disk/RAM); package availability varies by release.
5. Which data types can be moved with a tested rollback guarantee and which require maintenance downtime or explicit best-effort semantics.

