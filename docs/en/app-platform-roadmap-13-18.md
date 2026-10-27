# Application platform roadmap: Phases 13–18

This roadmap continues the implemented application platform through its release gate. It separates repository work from host and multi-VPS integration evidence. A phase is not called complete because a screen or schema exists: its acceptance checks must pass, and unavailable external checks stay visible as outstanding.

## Current checkpoint

Phases 00–12 are described in [Application platform architecture](app-platform-architecture.md). Phase 13's repository implementation and automated gate are complete in this candidate; a disposable-host check remains. Phases 14–18 below are the design and acceptance plan; they are not shipped functionality until their phase gate passes.

## Phase 13 — Application monitoring and alerts

### Purpose

Give each application a truthful view of its own managed process and durable data, without attributing shared Nginx, PHP-FPM, host traffic, or shared databases to one customer site.

### Delivery

- Sample only ARG-managed, app-scoped systemd units. Record active state, cumulative CPU time, memory current, and the latest known persistent-volume measurement.
- Derive CPU usage from two cumulative readings. Label it as percent of one core; values above 100% are valid on multi-core hosts.
- Persist bounded history with an additive SQLite migration, index it by app/time, and prune expired samples. Sampling must have a per-cycle work cap and must not block page navigation.
- Expose per-app read and alert-rule APIs with admin authorization, origin checks on writes, input bounds, and audit records.
- Show the panel collapsed by default on the local app card. Load history only when expanded. Display stale, unavailable, and not-measured states as such.
- Support deduplicated open/resolved alerts for service inactivity, CPU, and memory. Rules are configurable per app. Network bytes and HTTP latency remain unavailable until measured by a trustworthy app-scoped source.

### Acceptance gate

1. Migration succeeds on a fresh and existing ARG database without changing existing rows.
2. CPU delta handles missing samples, counter reset, zero interval, and long gaps without fabricated values.
3. Sampling is bounded, skips unsupported/shared units, and recovers after ARG restart.
4. Threshold validation, authorization, alert deduplication/resolution, retention, and API error paths are tested.
5. Frontend type-check/build and Rust test/release build pass. On a disposable host, verify systemd counters and service state against known workloads.

### Current implementation boundary

The code adds the sampler, SQLite history/rules/alerts, admin endpoints, and a collapsed per-app panel for systemd-backed application services. It samples at most 20 least-recently-sampled services per minute and keeps 30 days. It does not measure HTTP latency, per-app network traffic, Docker process metrics, or shared service attribution. Those remain explicit gaps.

## Phase 14 — Secure cross-VPS application network

### Purpose and design

Create an optional private data plane between paired ARG servers. Existing Federation HTTPS is the control plane; it is not a private application network and must not be presented as one.

### Delivery sequence

1. Approve an ADR covering WireGuard versus an authenticated tunnel, key ownership/rotation/revocation, allowed routes, MTU, NAT traversal, failure behavior, and recovery when one ARG node is offline.
2. Add a peer state machine (`planned`, `provisioning`, `ready`, `degraded`, `revoked`, `failed`) and reconciliation jobs. Store public keys and safe routing metadata; keep private keys root-only and never return them through APIs.
3. Default to deny. Require an explicit app/service-to-peer route and CIDR allowlist. Reject overlap with host, Docker, metadata, and existing peer networks.
4. Add bounded handshake/latency/route health checks and audit peer lifecycle changes. Do not expose unrestricted shell or arbitrary route commands to the browser.
5. Add UI only after the helper can safely create, verify, reconcile, and remove the tunnel. Show control-plane pairing separately from data-plane reachability.

### Acceptance gate

- Unit and helper tests for key-file permissions, CIDR validation, route overlap, revocation, reconciliation idempotency, and default-deny behavior.
- A real two-VPS test verifies authenticated peer establishment, only approved routes, restart recovery, node loss, key rotation, and revocation.
- No “connected” status is shown based only on controller API reachability. Until the two-host gate passes, this phase remains design-only or explicitly experimental.

## Phase 15 — App-scoped terminal, files, and source browser

### Purpose

Provide application-specific operations without reusing the global root/server terminal or unrestricted server file manager.

### Delivery

- App terminal uses a per-app unprivileged identity, fixed working-directory roots, a PTY lifetime/session cap, idle timeout, output bounds, explicit authorization, and audit events. It cannot become root or read ARG secrets, sibling apps, host credentials, or arbitrary absolute paths.
- File browser is rooted in the app's release/shared-data paths. Resolve paths beneath an opened root, reject traversal and symlink escape, and use optimistic version/ETag checks for edits.
- Separate immutable deployed releases from writable shared data. Editing the active release is disabled; users create a new deployment or a staged change and redeploy.
- Git view exposes repository identity, selected ref, pinned commit, and deployment history. It must not display stored Git credentials or private key material.
- Stream uploads/downloads with size limits and cancellation; destructive changes require explicit confirmation and are audited.

### Acceptance gate

Test path traversal, symlink swaps, cross-app access, stale ETags, PTY disconnect/reconnect, output floods, session expiry, permission boundaries, and non-admin denial. Run on a disposable host under the actual service users and filesystem layout. The global root terminal remains a separate, explicitly dangerous feature.

## Phase 16 — Safe application migration between VPS nodes

### Purpose

Move an app as a resumable, verifiable workflow while preserving the source as the recovery authority until target validation and explicit cutover.

### Migration state machine

`draft → preflight → source_snapshot → transfer → restore → validate → cutover_pending → cutover → complete`, with terminal `failed`, `cancelled`, or `rolled_back` states. Every step is durable and idempotent.

### Delivery

- Build a preflight dependency graph: runtime/version, app services, domain/Nginx/TLS references, persistent volumes, DB, S3 bindings, ports, scheduled jobs, and external dependencies.
- Stateless apps may be eligible for lower-downtime migration only after matching runtime and health checks. Stateful migrations require a consistency plan, write freeze or replication strategy, downtime notice, and validated backup/restore.
- Transfer signed manifests and checksummed, bounded chunks through the authenticated node protocol; support resume and cancellation. Never move plaintext app secrets in ordinary job logs or browser state.
- Restore to a separate target app identity and staging domain/port. Validate filesystem ownership, service state, HTTP health, and application-provided checks before cutover.
- Cutover is explicit. Keep source data and service state fenced/read-only or stopped according to the selected plan; avoid two writers. Rollback restores routing and source service from the recorded state.
- Delete source only in a separate later operation after a retention period and a fresh confirmation.

### Acceptance gate

Test interrupted/resumed transfer, checksum mismatch, disk exhaustion, target runtime mismatch, failed restore, failed health checks, duplicate cutover request, fencing, and rollback. A real two-VPS run is required for both a stateless app and a stateful app with data. A metadata-only owner change does not count as migration.

## Phase 17 — Deployment hooks and application backups

### Signed deployment hooks

- Accept GitHub/GitLab-style webhook requests only on a dedicated route with per-hook secret references, HMAC verification over the raw body, constant-time comparison, timestamp/replay protection, and event-size limits.
- Bind a hook to one app and exact repository/ref policy. Queue an existing durable deployment job; never run shell text from a webhook.
- Return an opaque request/job ID and avoid logging secrets, full payloads, or environment values. Support rotation/revocation and audit delivery outcomes.

### Backups

- Define a versioned backup manifest that names app metadata, selected release, persistent data, and optional supported database snapshot. Exclude plaintext `.env`, credentials, and transient caches by default.
- Encrypt archives before remote transfer using a user-controlled recovery key or approved root-only key; include checksums and schema/format versions. A secret export is a separate, explicit encrypted flow.
- Restore into a new staging identity first. Verify archive integrity, paths, ownership, required runtime, database compatibility, and health before offering cutover.
- Schedule backups with durable jobs, bounded concurrency/retention, destination capability checks, and clear last-success/stale states. A successful upload alone does not prove a usable backup; perform restore drills.

### Acceptance gate

Verify HMAC/replay/ref rejection and idempotent queueing. Restore encrypted backups in a clean test environment, verify checksums and permissions, and test missing keys, corrupt archives, low disk, cancellation, retention, and partial remote upload. A backup is “verified” only after a restore test.

## Phase 18 — Release hardening and completion

### Required release evidence

- Threat-model review for federation, helper actions, build sandboxes, app terminals, S3 secrets, webhooks, backup keys, file access, and migration/cutover.
- Automated tests: Rust unit/integration, frontend type-check/build, docs build, migration from supported prior schemas, and helper allowlist tests.
- Compatibility matrix for supported Debian/Ubuntu releases and optional Nginx, Docker, PHP, Node, MySQL, and S3 providers. Missing optional services must remain graceful.
- Disposable-host workflows for install/upgrade/rollback, Nginx/PHP-FPM, Node/systemd, disk pressure, backup/restore, and cancellation/restart recovery.
- Multi-VPS workflows for pairing, overlay, app migration, offline nodes, revocation, and protocol-version mismatch. No multi-VPS feature is production-ready without at least two independently provisioned hosts.
- Performance checks on representative VPS sizes: page navigation must not trigger heavy scans; sampling and background jobs have concurrency, CPU, memory, I/O, and retention bounds.
- Bilingual operator documentation, failure runbooks, migration/rollback guide, changelog, and version bump only after all available gates pass.

### Completion rule

Mark each feature as `implemented`, `automated tests passed`, `host integration passed`, and `multi-host passed` separately. Do not collapse these into a single “complete” badge. Disable or clearly mark any feature whose safety gate is still open.

## Batch workflow

The execution plan groups Phases 13–17 as one five-phase batch, followed by Phase 18 as the final release gate. For each batch: inspect current implementation; implement only real, bounded behavior; update this roadmap and the changelog; run the complete local test/build suite; run host or multi-host gates only where the required disposable infrastructure exists; then commit and push to `development`. A blocked external gate is recorded with its evidence and does not get described as passed.
