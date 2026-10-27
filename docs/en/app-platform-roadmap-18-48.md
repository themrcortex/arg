# ARG application platform roadmap — Phases 18–48

This roadmap continues Phases 13–17. Phase 18 is a hardening checkpoint, not the project end; Phase 48 is the final parity and release gate. “Optional” phases 39, 40, and 47 stay disabled by default, but any selected feature still needs its own acceptance gate.

The Persian file preserves the full phase specifications from the master plan: [نقشهٔ راه فازهای ۱۸ تا ۴۸](../fa/app-platform-roadmap-18-48.md). Phase 18's checklist and the current 13–17 boundaries are also recorded in [the preceding roadmap](app-platform-roadmap-13-18.md) and [architecture status](app-platform-architecture.md). A plan entry is not evidence that code or tests exist.

## Execution ledger — 2026-10-27

**Batch 18–22 repository checks:** 141 Rust tests passed; the frontend type-check and production build passed; the optimized Rust release build passed; the bilingual documentation check/build passed (192 pages, 384 localized articles). S3 helper credentials use AES-256-GCM ciphertext under `/etc/arg/s3` and a root-only local key; existing JSON credentials migrate on their first successful helper read. Provider-neutral connection management, key recovery/rotation, and secret reuse scopes are not implemented yet. Workspace/project/environment metadata, default migration, CRUD APIs, app assignment, and selectors are present; environment-specific runtime/config/secret separation and production promotion locks are still open. The current S3 app binding remains metadata and does not enforce provider ACLs. Laravel, React/Vite, and Node presets create an application draft only; they do not acquire source, install software, or deploy. Two/three-VPS host tests are unavailable in this environment and remain pending.

These are bounded repository changes, not a claim that Phases 18–22 have passed their full acceptance gates. External host tests remain separately identified and must not be treated as passed.

**Current batch: Phases 23–27, partial implementation.** The repository passes 155 Rust tests, `cargo check`, optimized release build, frontend type-check/build, and bilingual documentation check/build (192 pages, 384 localized articles). Phase 23 analyzes only the active release and stores a command-free Build Plan. Phase 24 adds a guarded, read-only source browser for text files in that release; it is not a private Git integration or editor. Phase 25 records content-addressed app/environment metadata snapshots without secret values; restore, preview lifecycle, and promotion are not implemented. Phase 26 links an existing Docker container to a Docker app for status display only and does not take over the container. Phase 27 implements Cloudflare zone verification and guarded DNS record CRUD with an encrypted service-user credential file; only Cloudflare is wired, propagation checks and DNS-01 are absent. Cloudflare tests are validation/unit tests and have not called a live provider. Multi-provider and disposable-host acceptance remains pending.

**Phases 28–32, partial implementation:** `v0.6.9-dev.8` adds a domain-verified app-to-Nginx ownership link, read-only PostgreSQL inventory, bounded active-release source archives, node tags and persisted rollout previews, plus private-network policy drafts. The rollout preview does not run remote updates; network drafts do not create tunnels, routes, or firewall rules. The source archive is neither a backup nor secret-scanned. Live Nginx/PostgreSQL/fleet/network acceptance remains pending.

**Phases 33–37, partial implementation:** app-linked Nginx traffic can be sampled on demand from at most 4 MiB and 5,000 access-log lines; it reports only logged request counts, body bytes, status groups, and available hourly buckets. Durable in-app events are recorded when supported app CPU/RAM/service alerts open or resolve, and admins can acknowledge them; event history is retained for up to one year. An authenticated role-capability endpoint and tests make the current policy explicit: only `admin` passes the admin API guard; `operator` and `viewer` do not yet have separate operation grants. A host-security inventory checks only whether selected scanner binaries exist. Extension administration stores/revokes Ed25519 trust keys and validates a constrained manifest plus a signature over its declared package digest; it does not receive package bytes, verify the actual archive, install, or execute extensions. No HTTP latency percentiles, durable network telemetry, notification delivery providers, passkeys/2FA, WAF enforcement, SBOM/CVE scanning, or extension marketplace is included in this batch.

**Validation for Phases 33–37:** 167 Rust tests, Rust check, frontend TypeScript check and production build, bilingual documentation check/build, optimized Rust release build, and bounded access-log sampling tests passed. The final post-push version gate remains pending. No external VPS/provider test is claimed.

## Phase 18 — Hardening checkpoint

**Goal:** establish a reliable release-candidate baseline for what is implemented so far. Run the single-host, two-host, and three-host-with-one-offline matrices; review remote routing, terminals, code/file access, secrets, hooks, uploads, and build execution; stress app/job/log workloads and SQLite; verify migrations, upgrades, backups, restores, RTL/mobile/accessibility, and compatibility. Keep cross-server networking or migration behind a feature gate unless live tests pass.

**Gate:** reproducible E2E evidence, old-install upgrade safety, documented limitations/runbooks, aligned versions and release notes. Host and multi-host tests that cannot run in CI remain `NOT_RUN_EXTERNAL`, never implied passes.

## Phase 19 — Connections Hub and central Secret Vault

**Depends on:** 02, 03, 11 · **Priority:** P1

- Add provider-neutral connection metadata (`provider`, scope, server/workspace, capabilities, verification time) for Git, S3, DNS, SMTP, and cloud providers. Keep secret bytes outside SQLite metadata.
- Build an authenticated-encryption vault with key/recovery ADR, root-only storage, rotation/revocation, masked references, scoped delivery, audit, and deletion guards for dependants.
- Add a Connections Hub, explicit Test Connection, expiry/reachability states, and reusable references in wizards. Never return secret material to the browser after saving.

**Tests / acceptance:** encryption and wrong-key failure, rotation, scope isolation, no secret in HTTP/audit/logs, remote-offline revocation. One permitted S3 or Git connection must be reusable by multiple apps without cross-workspace disclosure.

## Phase 20 — Workspaces, Projects, Environments, and lifecycle

**Depends on:** 03, 04, 19 · **Priority:** P1

- Create `Workspace → Project → Environment → Application/Service`, with an automatic default workspace/environment so existing single-server installs keep working.
- Isolate branch/config/secret/domain/S3/database references for development, staging, and production while allowing shared runtimes where safe.
- Add project overview, environment switching, service dependency graph, scoped app list, production locks, approvals, and audited promotion. Data-copy into staging requires backup and privacy warnings.

**Tests / acceptance:** migrate all existing apps to defaults; same project/environment names, cross-project isolation, production lock, and failed-promotion rollback. Deploy the same Laravel app to staging and production without touching a neighboring app.

## Phase 21 — Service bindings and component attachment

**Depends on:** 09, 11, 19, 20 · **Priority:** P1

- Add a typed binding resolver for service and secret references, dependency graphs, scope checks, health, and detach-impact previews.
- Attach existing or provisioned database, Redis/cache, queue, S3, SMTP, and telemetry services. Generate masked Laravel/Node environment mappings and use least-privilege users/tokens.
- Keep existing app S3/network/volume features as adapters; an ARG metadata label alone must not be presented as provider ACL or network isolation.

**Tests / acceptance:** cross-project allow/deny, broken/remote dependency, rotation, masked secrets, incompatible runtime, and safe detach. Connect an app to S3 or Redis without pasting the same credential into multiple pages.

## Phase 22 — App templates and one-click catalog

**Depends on:** 04, 20, 21 · **Priority:** P1

- Define versioned declarative YAML/JSON templates for runtime, source/image, fixed steps, health, domains, services, networks, volumes, and generated secrets. Validate schema, compatibility, digests, ports, tokens, and permissions before creating a job.
- Start with a small curated set: Laravel, React/Vite static, Next, Nuxt, Node API, Redis, PostgreSQL, and WordPress only after its driver is ready. Keep custom local templates and credential-free import/export.
- Add searchable gallery, prerequisites/resources/license, preview diff, async install through the durable job engine, cleanup, and audit.

**Tests / acceptance:** invalid schema/token, duplicate port, zip traversal, secret leakage, replayed install, and offline catalog. At least three templates must install, pass health checks, and clean up on a disposable VM.

## Phase 23 — Buildpacks and project detection

**Depends on:** 04, 05, 07, 08, 22 · **Priority:** P1

- Inspect manifests and lockfiles for PHP, Laravel, Node, static, Python, Docker/Compose, and monorepos. Produce an immutable `BuildPlan` with confidence, runtime, commands, artifacts, ports, environment needs, health checks, and warnings; always allow a manual override.
- Preview generated host-runner or container plans and their diff before execution. Build in a non-root sandbox with CPU/RAM/disk/time/network bounds and no panel-secret or unrestricted Docker-socket access.

**Tests / acceptance:** fixtures for Laravel, React, Vue, Next, Nuxt, Express, TypeScript, Nest, static, Python, Compose, and monorepo; ambiguous detection and invalid manifests. A repo without a Dockerfile must show an editable, reviewable plan before deploy.

## Phase 24 — Git sources, registries, and code workflows

**Depends on:** 15, 17, 19 · **Priority:** P1

- Add GitHub/GitLab/self-hosted connections, deploy keys or OAuth, pinned branch/tag/commit, host-key policy, webhook verification, and safe disconnect.
- Expand source import to ZIP, approved server folders, monorepos, and explicitly adopted Nginx apps. A code tab may view/diff/history and stage changes; it must not overwrite a live release.
- Scrub credentials from clone output, pin fetched revisions, enforce mirror/cache quotas, and serialize webhook/deploy races by app/environment.

**Tests / acceptance:** forged/duplicate webhook, wrong branch, denied private clone, traversal/symlink escape, binary/non-UTF8 files, and credential redaction. Each deployment records an immutable source revision.

## Phase 25 — Preview environments and configuration snapshots

**Depends on:** 17, 20, 23, 24 · **Priority:** P1

- Create PR previews with quota, TTL, ephemeral domains, independent data, automatic expiry cleanup, and protected production resources.
- Store immutable snapshots of configuration references, runtime, domains, and bindings without secret plaintext. Diff/restore/promote a pinned artifact through approval and health gates.
- Clearly distinguish preview data from production copies; any production-data sanitization/copy must be opt-in and backed up.

**Tests / acceptance:** abandoned-preview cleanup, secret masking, scope isolation, quota limits, snapshot restore, and proof that preview deletion leaves production volumes untouched.

## Phase 26 — Advanced Docker/Compose orchestration

**Depends on:** 09, 10, 13, 21 · **Priority:** P1

- Bind owned Compose projects and services to apps/servers; detect unmanaged resources without adopting them silently. Pin image digests and show drift/diff before recreate.
- Add graceful recreate, health checks, resource limits, restart-loop protection, per-service states, and guarded rollback to a prior image digest. Scaling/autosleep stay opt-in and disabled by default.

**Tests / acceptance:** drift, missing images, interrupted pulls, restart loops, port collision, rollback, Docker unavailable. A multi-container app presents one truthful state summary plus per-service detail.

## Phase 27 — DNS providers and Zone Manager

**Depends on:** 19, 28 · **Priority:** P2

- Add scoped adapters for Cloudflare, Route 53, and DigitalOcean; validate zone ownership and CRUD A/AAAA/CNAME/MX/TXT/CAA/SRV with TTL constraints and preview diffs.
- Add idempotent writes, propagation checks across resolvers, best-effort rollback, and DNS-01 challenge cleanup. Warn before changing mail-related MX/TXT records.

**Tests / acceptance:** provider mocks, timeout/rate limit, wrong zone, concurrent edits, secret scope, pending propagation. An authorized app domain resolves to the intended server with explicit status.

## Execution log — Phases 28–32

This batch is implemented as bounded, explicit slices: an Nginx ownership link that does not edit config, read-only PostgreSQL inventory, a source-only archive that is not claimed as a restore, fleet tags and a rollout preview that do not execute remotely, and network policy drafts that do not alter routes/firewall. `cargo test --locked` passed 161 tests, `cargo check --locked`, frontend TypeScript/build, bilingual docs TypeScript/build, and `git diff --check` passed. The optimized Release build runs after the batch version bump. A real VPS, PostgreSQL, active Nginx, WireGuard, and multi-node fleet are unavailable in this environment and are not reported as accepted.

## Phase 28 — Domains, SSL, and advanced Nginx performance

**Depends on:** 08, 13, 17 · **Priority:** P1

- Make each generated vhost app-owned; show config diff, run `nginx -t`, reload only after validation, and restore prior config on failure. Never overwrite unmanaged sites.
- Support certificate renewal, SAN, DNS-01 wildcard, validated custom certs, expiry alerts, safe TLS defaults, opt-in HSTS, cache/compression/proxy rules, WebSocket/SSE timeouts, and per-app request/upstream timing.
- Add safe per-app rate limits and maintenance mode. Validate trusted proxy lists; do not trust arbitrary forwarded headers.

**Tests / acceptance:** Nginx fixtures, invalid certificate, wrong owner, renewal failure, duplicate directives, WebSocket streaming, and cache isolation. A failed site update cannot break other vhosts.

## Phase 29 — PostgreSQL and managed databases

**Depends on:** 19, 20, 21 · **Priority:** P2

- Preserve MySQL/MariaDB; add PostgreSQL inventory, roles/users, optional host/container service, scoped app provisioning, rotation, and backups.
- Provide bounded, timed SQL access with read-only default, row/byte caps, destructive confirmation, and privacy-safe audit. Report database metrics only when measured.
- Use engine/version-aware transfer and restore with integrity verification.

**Tests / acceptance:** read-only enforcement, query timeout, tenant boundary, restore, and MySQL regression. One app can bind to MySQL in one environment and PostgreSQL in another.

## Phase 30 — Backup platform, recycle bin, and recovery drills

**Depends on:** 11, 16, 17, 19, 20 · **Priority:** P1

- Back up app config/revision metadata, persistent volumes, supported DB dumps, Compose manifests, and secrets only through a separately approved encrypted recovery-key flow.
- Support local/S3 destinations, retention by count/age/size, immutable manifests, checksums, optional encryption, offsite verification, dependency-aware soft delete, and separately confirmed purge.
- Restore to a staged target after quota/runtime/storage/conflict preflight; schedule restore drills outside production.

**Tests / acceptance:** corrupt/missing archives, concurrent deploy, low disk, remote restore, ACLs, and retention. A backup is verified only after a restore proves file and DB integrity.

## Phase 31 — Fleet enrollment and safe rollouts

**Depends on:** 01, 02, 13, 17 · **Priority:** P1

- Guided enrollment with OS/capability preflight, HTTPS fingerprint confirmation, short-lived approval, rotation, and revocation. Add groups/tags, heartbeat, compatibility, and expiring idempotent offline queues.
- Roll out ARG updates canary → batches → remainder with health gates and safe rollback. Show drift before any host remediation; do not auto-change firewall or packages without approval.

**Tests / acceptance:** stale/duplicate enrollment, replay, expired offline jobs, partial rollout, and rollback failure. Three nodes with mixed versions show honest state; one failure does not damage the fleet.

## Phase 32 — Private networking and remote edge access

**Depends on:** 14, 21, 31 · **Priority:** P1

- Reuse the approved overlay for opt-in site-to-site links, key rotation, MTU/health, app-to-service allow rules, scoped DNS discovery, and NAT edge ingress.
- Prevent route leaks, overlapping CIDRs, firewall conflicts, split brain, and stale DNS. Show measured latency only when measured; do not imply shared POSIX storage.

**Tests / acceptance:** offline peer, key rotation, cross-tenant denial, route conflict, reconnect, and firewall rollback. App A reaches only an explicitly allowed DB on B.

## Phase 33 — Metrics, APM-lite, capacity, and log analytics

**Depends on:** 12, 13, 26, 28 · **Priority:** P1

- Add bounded CPU/RAM/disk/network, HTTP rate/errors/latency percentiles, uptime, deploy correlation, DB telemetry, and container restart metrics with explicit source/freshness.
- Downsample raw/minute/hour/day under storage caps. Add per-app/fleet capacity views and optional OpenTelemetry/Prometheus/Grafana/Loki adapters; estimates must be labeled.

**Tests / acceptance:** cardinality, sparse/missing samples, downsampling, load, and remote disconnect. History survives restart and correlates deployment changes with measured errors.

## Phase 34 — Notifications, incidents, and public status

**Depends on:** 12, 13, 19, 33 · **Priority:** P1

- Durable notification bus with retry/dedup for in-app and opt-in Telegram/Discord/Slack/email/webhooks; severity, quiet hours, cooldown, hysteresis, acknowledgement, and delivery audit.
- Add health checks, incident lifecycle, certificate/disk/S3/deploy/backup alerts, and optional public status pages with private topology and secrets removed.

**Tests / acceptance:** retry storm, duplicates, provider failure, webhook SSRF, and public-data leakage. A real alert remains queryable and acknowledgeable while the provider is down.

## Phase 35 — Identity, Passkeys/2FA, and granular RBAC

**Depends on:** 02, 19, 20 · **Priority:** P1/P2

- Define least-privilege owner/admin/developer/operator/viewer policies across server/app/terminal/deploy/secret/S3/DNS/DB. Add TOTP/recovery codes and WebAuthn/passkeys; SSO/OIDC remains opt-in.
- Add step-up auth for root terminal, purge, secret export, production deploy, and cross-server migration; short-lived grants, rate limits, session revocation, and audit correlation.

**Tests / acceptance:** role matrix on every API, federation scope, revocation, 2FA recovery, SSO forgery, and privilege escalation. A viewer cannot gain a terminal or write action through direct HTTP.

## Phase 36 — Host security, WAF, and supply-chain checks

**Depends on:** 19, 26, 28, 35 · **Priority:** P2

- Optional ModSecurity/OWASP CRS with detect-first mode, audit, and rollback; optional Fail2ban/Lynis/ClamAV/integrity checks with OS capability detection.
- Add image SBOM/CVE and provenance/signature checks. Show severity and remediation; do not silently mutate host configuration.

**Tests / acceptance:** WAF false positives and rollback, missing scanners, malicious artifact fixtures, and no unapproved config changes. Missing optional tools never break ARG.

## Phase 37 — Extension SDK and secure marketplace

**Depends on:** 19, 22, 35, 46 · **Priority:** P3

- Define versioned `arg-extension.json` with permissions, capabilities, entrypoints, migrations, ARG compatibility, and signed digest.
- Provide install/enable/disable/upgrade/uninstall with backup/rollback, deny-by-default APIs, signature verification, curated/offline sources, provenance, and limited UI contribution points. No unrestricted remote JavaScript or `eval`.

**Tests / acceptance:** tampering, traversal, unsigned denial, incompatibility, interrupted upgrades, RBAC bypass, and rollback. Extensions remain optional and auditable.

## Phase 38 — Visual automation and infrastructure designer

**Depends on:** 04, 20, 21, 34, 35 · **Priority:** P2

- Build versioned workflows on durable jobs with manual/schedule/webhook/event triggers, typed inputs, dependency graph validation, cycle checks, retries/backoff/timeouts/idempotency, compensations, and human approvals.
- Add visual plan/diff before apply for apps, DB/cache, networks, S3, backups, and deploys. Redact outputs and keep per-node history, pause/resume, and recovery.

**Tests / acceptance:** invalid cycles, duplicate triggers, concurrent runs, failed cleanup, offline node, and privilege escalation. A deploy→health-check→notify workflow and scheduled backup work end to end.

## Phase 39 — Cloud provisioning and cost tracking (optional)

**Depends on:** 19, 31, 35 · **Priority:** P3

- Start with read-only inventory for supported cloud providers. Keep tokens in the vault. Provision only after size/region/image/network/SSH/cost preview, quota check, and explicit confirmation.
- Pair created hosts after fingerprint approval; clean partial resources. Keep destroy/resize/snapshot separate, with blast-radius preview. Label estimates separately from provider-billed cost.

**Tests / acceptance:** provider mocks, rate limits, partial-create cleanup, revocation, ownership confusion, and cost discrepancies. A test provider VM is provisioned only with explicit opt-in.

## Phase 40 — Mail server suite (optional add-on)

**Depends on:** 19, 27, 28, 35 · **Priority:** P3

- Optional Postfix/Dovecot/mailboxes with TLS, SPF/DKIM/DMARC, queue controls, mailbox quotas, secure credentials, and backup/restore. Do not log message contents.
- Preflight SMTP/IMAP ports, PTR/rDNS, provider restrictions, reputation, and DNS; users who only host websites never install mail services by default.

**Tests / acceptance:** config and DNS mismatch, relay denial, permissions, install rollback, and TLS expiry on an isolated test host.

## Phase 41 — Python runner and WordPress installer

**Depends on:** 06, 07, 20, 21, 22, 28, 29 · **Priority:** P2

- Add venv-based Python versions and lock-aware requirements/uv/Poetry detection for Flask/Django with Gunicorn/Uvicorn, worker services, static collection, and health checks.
- Add a WordPress template using the shared PHP-FPM/MySQL/security/backup primitives. Existing WordPress import must not silently take ownership. Staging promotion requires DB/media backup and URL rewrite preview.

**Tests / acceptance:** Flask/Django smoke, WordPress install/migration, staging promotion, rollback, unsupported plugin, and permissions. All apps stay in the common Applications UI.

## Phase 42 — Dashboard customization, themes, and branding

**Depends on:** 03, 13, 20 · **Priority:** P2

- Save widget order/visibility and Minimal/DevOps/Hosting/Fleet presets per user and scope. Add token-based light/dark/accent/logo options without copying other products' UI.
- Keep widgets read-only by default, accessible by keyboard, RTL-safe, responsive, lazy-loaded, and paused in hidden tabs. Provide reset and preference migrations.

**Tests / acceptance:** scope leakage, contrast, RTL mobile snapshots, hidden-tab polling, and removed-widget migration. One user's dashboard never changes another's.

## Phase 43 — Mobile/PWA and offline read-only

**Depends on:** 31, 33, 34, 42 · **Priority:** P2

- Cache only the UI shell and explicitly stale, read-only metrics. Never cache credentials/secrets. Add mobile navigation, alerts, reconnect-aware acknowledgement, and optional user-approved push.
- Do not queue destructive/offline mutations; refresh and reconfirm against live state after reconnect. Logout and session expiry purge cached data.

**Tests / acceptance:** lost connection during deploy, wrong-server stale cache, expired session, cache leakage, and iOS/Android layouts. Offline data is always visibly stale; dangerous actions remain unavailable.

## Phase 44 — Teams, quotas, and full multi-tenancy

**Depends on:** 20, 31, 35 · **Priority:** P2

- Add team invitations and workspace/project/environment roles. Enforce per-tenant app/server/CPU/RAM/storage/deploy/API quotas at the destination before scheduling work.
- Isolate secrets, metrics, logs, jobs, files, S3, alerts, and share links. Preserve a backward-compatible local owner. Billing-ready metering is optional; no payment gateway is implied.

**Tests / acceptance:** API/UI/WebSocket IDOR, quota races, revoke invited users, stale sessions, and cross-tenant resource checks. Two teams share a controller without sharing each other's data or terminal.

## Phase 45 — Public API, API keys, webhooks, and OpenAPI

**Depends on:** 19, 34, 35, 44 · **Priority:** P2

- Add hashed, scoped, expiring, rotatable API keys with usage counters/rate limits; publish versioned OpenAPI, pagination, and correlated structured errors.
- Add signed outbound events with dedupe/retry/backoff/dead-letter and SSRF-safe destinations. A CLI may use only the same scoped APIs. Define backwards compatibility and deprecation policy.

**Tests / acceptance:** wrong scopes, expiry, replay, webhook timeout, schema contract, and old-client compatibility. An external CI can trigger one scoped deploy and poll its result.

## Phase 46 — Installer/updater profiles and ARG Doctor

**Depends on:** 18, 31, 35 · **Priority:** P1

- Define real Minimal/Standard/Full opt-in profiles and capability probes on install/reboot. Reuse the current updater with preflight, DB backup, staged release, migration notes, health gate, and crash recovery.
- Add Doctor checks for DNS/TLS, Docker, S3 mounts, node pairing, runtimes, permissions, systemd, and disk/log pressure; show OS/architecture support honestly.

**Tests / acceptance:** old ARG upgrade with apps/S3/Federation, interruption, low disk, rollback failure, and Docker absent. Existing resources and secret references survive update.

## Phase 47 — AI operations assistant (optional)

**Depends on:** 35, 45, 46 · **Priority:** P3

- Provide evidence-linked, read-only explanations of deploy failures, logs, and metric anomalies with citations to actual records. BYOK/provider use is opt-in with cost controls and secret redaction.
- Treat repository content/logs as untrusted prompt input. No shell/root/firewall/DB mutation without explicit human approval and allowlisted tools; core ARG stays useful with AI disabled.

**Tests / acceptance:** prompt injection, unauthorized tool invocation, redaction, provider timeout, and misleading diagnosis. Without approval the assistant cannot execute an operation.

## Phase 48 — Final parity audit, E2E, and release

**Depends on:** selected release-target phases; optional P3 work is not automatically required · **Priority:** release gate

- Publish a feature inventory with `implemented/tested/experimental/not planned` and evidence. Finish threat review for federation, vault, marketplace, WebSockets, terminals, uploads, hooks, DNS, migration, and tenancy.
- Run soak/load tests across apps and three nodes, offline agent, partition, and storage failure. Complete old-version upgrade/rollback, restore drills, retention/quotas, API compatibility, bilingual docs, license review, and signed artifacts.
- Never turn a failed blocker into a pass by hiding it; disable or feature-gate unsafe capabilities and publish limitations.

**Acceptance:** ARG remains lightweight/useful on one VPS while selected multi-server app workflows work reliably, and every release claim has repeatable evidence.

## Execution protocol

Use the dependency graph above rather than implementing future UI ahead of its backend. For each five-phase batch: inspect current code; mark already-satisfied requirements with evidence; implement the smallest complete, safe vertical slice; update bilingual docs/changelog; run Rust, frontend, docs, and relevant host tests; then push to `development`. Any required host/cloud/multi-host test without an available disposable environment stays explicitly pending. Phase 48 is the project release gate, not a promise that optional P3 features are enabled.
