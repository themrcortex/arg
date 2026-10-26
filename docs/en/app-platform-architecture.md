# Application Platform Architecture

This document records the Phase 00 baseline and architecture decisions for extending ARG from host administration to application lifecycle management. It describes the current repository, the intended ownership model, and constraints that later phases must preserve. Planned capabilities are not claims about features already shipped.

## Current baseline

ARG is a single installable Rust/Axum service with a React/TypeScript/Vite interface embedded in its release binary. SQLite stores panel accounts, sessions, settings, audit records, metrics, and feature-specific metadata. Host operations that need elevated privileges cross a restricted Unix-socket helper boundary.

Existing modules include `src/federation.rs`, `src/nginx.rs`, `src/docker.rs`, `src/s3/`, `src/files.rs`, `src/metrics.rs`, and `src/maintenance.rs`. Federation can pair ARG instances, read remote status and services, and proxy an authenticated workspace to a selected remote node. Nginx sites, Docker resources, and S3 buckets remain resources of their hosting server. They are not yet represented by a unified Application record. The current route table has no `/api/apps` lifecycle API or persistent application deployment job engine.

The frontend build is embedded from `frontend/dist`. SQLite schema creation and incremental compatibility work currently live in `src/main.rs`; new application schemas must use additive, idempotent migrations and must preserve existing user, session, node, S3, Nginx, and Docker data. The current Node protocol is `arg-node/2`; protocol compatibility must be explicitly reviewed before changing it.

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

Before adding application execution, keep an executable baseline for frontend checks/build, Rust tests/build, shell syntax, docs validation, and version consistency. In the 2026-10-26 workspace, `cargo test --locked`, frontend type checking/build, documentation type checking/build, and bilingual documentation validation were run successfully. These results do not replace a real two-VPS integration test.

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
- **Application platform:** Phase 03 adds an ownership-aware metadata registry. Phase 04 adds a durable single-worker job engine, bounded Git/ZIP/directory acquisition, release staging, atomic `current` pointer changes, progress history, cancellation, retry, and rollback. Framework builds, app runtimes, HTTP health checks, Nginx cutover, and per-app isolation remain future phases.
- **CI:** GitHub Actions workflows exist for CI and documentation publishing. Local required checks are documented in `docs/en/development.md`.
- **Versioning:** Rust and frontend versions are synchronized; stable and prerelease updates use `scripts/bump-version.sh`, `scripts/bump-development-version.sh`, and `scripts/check-versioning.sh`.

## Decisions that must be revisited before implementation

1. Whether the first application runner is native systemd, container-only, or a per-app adapter supporting both. Do not promise isolation stronger than the selected execution boundary provides.
2. How existing Nginx sites and Docker containers are discovered, imported, and protected from duplicate ownership.
3. How session and credential rotation will migrate existing Federation pairings without breaking access or leaving stale long-lived sessions.
4. Which first release target is supported (Ubuntu/Debian versions, architectures, and minimum disk/RAM); package availability varies by release.
5. Which data types can be moved with a tested rollback guarantee and which require maintenance downtime or explicit best-effort semantics.

