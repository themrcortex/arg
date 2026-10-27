const e=`# ARG Change Log

## v0.7.1-dev.1 — 2026-10-27

### Development
- Development prerelease based on the next patch after main.

## Unreleased

## v0.7.0 — 2026-10-27

### Added
- Refresh the dashboard UI with a shared glass design system, responsive navigation, reusable data views, and step-based flows for complex operations.
- Add detailed application workspaces with framework marks, per-application routes, and server-to-Node connection topology based on real status exchanges.
- Give each Nginx site a dedicated, tabbed workspace for overview, configuration, domains and SSL, operations, and activity; persist the selected site and tab in the hash URL.
- Expand the bilingual documentation site with grouped navigation, search, and updated guides.
- Add controller-side updates for compatible full-access Nodes, including branch/build-mode choice, progress, cancellation, rollback, and post-update identity checks.

### Fixed
- Reconcile S3 mount status against both systemd and the live mount table, and safely normalize stale mount-directory ownership before mounting.
- Release database locks before awaiting Node update operations so the Axum handlers remain \`Send\` and build successfully.

### Security
- Require the negotiated \`node.update.manage\` capability and full-management scope for remote updates; retain pairing identity and credentials through network loss, failed verification, cancellation, and rollback.

### Validation
- Frontend type-check, production build, offline service-worker tests, and \`git diff --check\` passed.
- \`cargo check --locked\` and \`cargo test --locked\` passed (181 Rust tests).

## v0.6.9-dev.12 — 2026-10-27

### Changed
- Complete the Phase 48 audit of Phases 18–48 in Persian and English, recording implementation evidence and open acceptance gates for every phase.
- Keep the release gate blocked: tenant isolation, multi-host/restore drills, independent security review, artifact signing, and several live provider/host acceptance tests remain incomplete. No stable release or \`main\` promotion is claimed.
- Allow removal of an ARG Node pairing from the local panel when the remote Node rejects its credential (401); report that remote revocation could not be confirmed, while preserving the pairing after insufficient permission (403) or network/TLS failures.

### Validation
- 178 Rust tests, \`cargo check --locked\`, and \`cargo build --locked --release\` passed.
- Frontend type-check/build and both offline Service Worker policy tests passed; documentation type-check/build passed in Persian and English.
- \`git diff --check\` passed. Live multi-tenant, mobile-browser, provider, mail-host, and VPS upgrade/restore acceptance was not available and remains open.

## v0.6.9-dev.11 — 2026-10-27

### Added
- Add an installable, shell-only PWA cache and offline status notice. The service worker bypasses every \`/api\` request, bounds cached fingerprinted assets, and ARG removes its shell cache at logout.
- Add read-only ARG Doctor checks for OS family, systemd, Docker presence, root disk availability, TLS file presence, runtime executable presence, saved S3/node state, and app/deployment counts.
- Add an HTTPS-only v1 read API for application and deployment summaries, pagination, OpenAPI, and audited create/revoke controls for expiring API keys.
- Add Persian and English public API and Doctor/PWA guides.

### Security
- Store API bearer tokens as hashes only; reveal raw tokens once, cap active keys at 10 per administrator, enforce fixed read scopes and 60 requests per minute, and mark responses \`no-store\`.
- Exclude deployment source configuration and detailed error output from the public API. The API has no write scopes, webhooks, or tenant-specific key restrictions.

### Changed
- Record Phases 43–47 as partial in both roadmaps. Team isolation/tenant quotas, full installer profiles, remote/mobile acceptance, and the optional AI assistant remain open and are not presented as implemented.

### Validation
- 178 Rust tests, two Service Worker policy tests, \`cargo check --locked\`, frontend type-check/build, documentation check/build (194 pages per locale), optimized Rust release build, and \`git diff --check\` passed. No live multi-tenant, mobile browser, provider, installer-profile, or AI acceptance is claimed.

## v0.6.9-dev.10 — 2026-10-27

### Added
- Add persisted, application-scoped workflow drafts with a bounded typed DAG validator. The API rejects duplicate IDs, missing references, invalid step kinds, oversized graphs, and cycles; drafts do not execute jobs, commands, or triggers.
- Add Python web and WordPress application templates. They create application metadata only and do not install Python packages, WordPress, PHP/MySQL, or system services.
- Add per-user dashboard widget ordering and visibility with Minimal, DevOps, Hosting, and Fleet layout presets. Preferences are validated and stored separately for each account.

### Changed
- Clarify the implementation boundary for optional cloud provisioning and mail-server phases: no provider inventory/cost claims are made without a test account, and Postfix/Dovecot are not installed without an isolated host.
- Document current partial status for Phases 38–42 in both roadmaps, including workflow execution, Python/WordPress runtime setup, dashboard theme tokens, and external host tests that remain open.

### Validation
- 176 Rust tests, \`cargo check --locked\`, frontend type-check/build, documentation type-check/build, and \`git diff --check\` passed. The optimized Rust release build is run for this version before push. Cloud-provider and isolated mail-host tests were not available.

## v0.6.9-dev.9 — 2026-10-27

### Added
- Add on-demand app-linked Nginx traffic snapshots by reading a dedicated access log with a 4 MiB and 5,000-line ceiling; show only measured request counts, response body bytes, status groups, and logged hourly buckets.
- Add durable in-app monitoring events for supported application service/CPU/RAM alert transitions, with admin acknowledgement and one-year retention.
- Add an authenticated role-capability endpoint and explicit UI guidance that \`operator\` and \`viewer\` currently have no separate admin operation grants.
- Add a read-only host security inventory for selected scanner binaries; it does not install or execute scanners or claim that a tool is configured or active.
- Add an audited Ed25519 trust-key registry and constrained extension manifest validation. Package bytes are not uploaded or verified, and extensions cannot be installed or executed.

### Changed
- Bound Nginx access-log sampling by both file bytes and line count, avoiding an unbounded helper output buffer.
- Record Phases 33–37 as partial implementations in the Persian and English roadmap, with unimplemented acceptance items called out.

### Validation
- 167 Rust tests, \`cargo check --locked\`, frontend TypeScript check and production build, bilingual documentation check and production build, optimized Rust release build, and \`git diff --check\` passed. The post-push version check is still pending. External VPS/provider tests were not run.

## v0.6.9-dev.8 — 2026-10-27

### Added
- Add an audited application-to-Nginx ownership link that verifies the app domain against the site's \`server_name\` without changing Nginx configuration.
- Add bounded, read-only PostgreSQL service/version/readiness inventory; SQL and database mutations remain unavailable.
- Add local, checksummed active-release source archives with file/count/storage caps, sensitive-path exclusions, explicit deletion, and download. These archives exclude database/runtime data, are not secret-scanned, and are not restorable backups.
- Add node tags and persisted canary rollout previews; previews do not run builds or updates on remote nodes.
- Add private-network policy drafts with explicit \`enforced=false\`; no WireGuard, route, or firewall changes are made.

### Changed
- Add per-app Nginx link, source-archive, PostgreSQL inventory, fleet-preview, and network-draft controls with their limitations shown in the UI.
- Update the bilingual Phase 28–32 status and record remaining host/fleet acceptance work.

### Validation
- 161 Rust tests, \`cargo check --locked\`, frontend TypeScript and production build, bilingual documentation check and production build, and \`git diff --check\` passed. The optimized Rust Release build passed after the version bump. Live VPS/Nginx/PostgreSQL/WireGuard tests remain pending.

## v0.6.9-dev.7 — 2026-10-27

### Development
- Add bounded, non-executing build-plan analysis for active releases and a read-only source browser that blocks hidden files and escape paths.
- Add content-addressed application metadata snapshots without secret values, plus display-only links from Docker applications to existing containers.
- Add a Cloudflare DNS page with encrypted local credentials, zone verification, bounded record validation, confirmation for changes, and audit events. Provider propagation checks and DNS-01 are not included.
- Record the Phase 23–27 scope and outstanding host/provider validation in the bilingual roadmap and architecture status.
- Repository checks: 155 Rust tests, frontend type-check/build, documentation check/build (192 pages, 384 localized articles), and optimized Rust release build passed. Live Cloudflare and disposable-host tests remain pending.

## v0.6.9-dev.6 — 2026-10-27

### Development
- Add the Phase 18–22 implementation checkpoint to the development release.
- Encrypt S3 helper credential payloads with AES-256-GCM using a root-only vault key; migrate legacy credential files lazily during helper reads.
- Add workspace, project, and environment metadata with additive migration, scoped CRUD APIs, application assignment, and environment selectors.
- Add versioned Laravel, React/Vite, and Node template drafts; no source acquisition or deployment is implied.
- Require an optimized Rust release build in CI and publish the bilingual Phase 18–48 roadmap.
- Repository checks: 141 Rust tests, frontend type-check/build, documentation check/build, and optimized Rust build passed. Disposable-host and multi-VPS acceptance remains pending.

## v0.6.9-dev.5 — 2026-10-27

### Development
- Development prerelease based on the next patch after main.

### Phase 13 — Application monitoring
- Add bounded, background sampling for CPU time, memory, systemd state, and the latest measured persistent-volume usage of supported app services.
- Persist per-app history, alert thresholds, and deduplicated open/resolved alerts; show monitoring lazily in each local application card so page navigation does not start scans.
- Document Phases 13–18 in Persian and English, with explicit implementation, host, and multi-VPS acceptance gates.
- Repository gate: 136 Rust tests, Rust release build, frontend type-check/build, and documentation production build. Live systemd workload verification remains a disposable-host check; Phases 14–17 remain planned designs, not shipped features.

## v0.6.9-dev.4 — 2026-10-27

### Development
- Development prerelease based on the next patch after main.

### Phase 08 — Static delivery and SSR
- Add automatic/manual React, Vue, Vite, Next.js, Nuxt, and generic Node delivery selection, including prebuilt static output and safe custom paths.
- Publish validated static releases through app-owned Nginx sites with SPA fallback, immutable hashed-asset caching, sensitive-file denial, and release-aware rollback.
- Support standard Next and Nuxt SSR entrypoints, loopback health checks, and separate private runtime versus public build-time environment settings.
- Add responsive controls and bilingual guidance for static output paths, SPA behavior, SSR entrypoints, and the public visibility of build variables.

### Phase 09 — Application networks
- Add ARG-owned private Docker bridge networks with internal routing, generated names, ownership labels, member tracking, and delete-in-use protection.
- Allow Docker app containers created with an ARG application link to join only their own managed network; add loopback-only endpoint records for Node SSR services and state clearly that these do not provide network isolation.
- Add an app workspace panel for creating networks, connecting eligible services, reviewing members, and detaching them.

### Phase 10 — Persistent application volumes
- Add per-app persistent data directories under the deployment shared-data root, with an app-specific system group, safe paths, usage scans bounded to 100,000 entries, and explicit detach-versus-purge actions.
- Keep volume data outside release directories so a deploy or rollback does not replace it. Display quota as informational only because filesystem quota enforcement is not configured.

### Phase 11 — Application S3 bindings
- Add per-app Bucket/prefix bindings for object storage, backups, assets, artifacts, and existing mounts, while keeping credentials out of the app API and frontend.
- Protect connected Buckets from detach/delete until app bindings are removed. Show provider ACL requirements and the fact that ARG metadata does not itself restrict S3 permissions.

### Phase 12 — Persistent and resumable application logs
- Add bounded Node service journal reads (1–500 entries) with resume cursors and identify journald as the persistent source.
- Let the app panel request newer log entries without losing the current output. Retention continues to follow the host-wide journald policy; ARG does not claim per-app retention controls.
- Add bilingual app resource guidance and automated validation for network names, volume names, S3 prefixes, and allowed binding purposes.
- Validate the five-phase batch with 132 Rust tests, Rust checks, frontend typecheck/production build, documentation production build, and whitespace checks. Live Docker, systemd, S3-provider, and Debian/Ubuntu integration still requires a disposable host.

## v0.6.9-dev.3 — 2026-10-27

- Add a managed Node.js runner with isolated dependency installation/builds, per-app systemd services, private runtime environment, loopback health checks, and guarded Nginx proxying.
- Add bilingual Node runner documentation and update application/deployment phase status to match implemented behavior and remaining VPS integration checks.
- Pass the phase gate with 115 Rust tests, Rust release build, frontend production build, and documentation production build; disposable Debian/Ubuntu systemd/Nginx integration remains to be tested.

## v0.6.9-dev.2 — 2026-10-27

### Development
- Development prerelease based on the next patch after main.

### Phase 06 — Laravel runner
- Configure a Laravel site's Nginx and PHP-FPM pool through the application workspace, with config validation and restoration on failed activation.
- Manage Queue, Scheduler, Horizon, Reverb, and Octane using fixed, app-scoped systemd units; keep Reverb and Octane on loopback and switch the Nginx upstream safely for Octane.
- Provision a database and least-privilege MySQL account for an app; keep the generated password only in its private \`.env\` file and remove partially created accounts when provisioning fails.
- Refresh active web and background services after deployment or rollback, and restore the previous release/runtime when activation fails.
- Resolve the \`system\` PHP-FPM choice from installed versions, check service ports for conflicts, and keep database/file configuration aligned during rollback recovery.
- Document the implemented scope and remaining VPS-only integration checks in Persian and English.

## v0.6.9-dev.1 — 2026-10-26

### Development
- Development prerelease based on the next patch after main.
- Give each ARG installation a stable server UUID and synchronize its configurable public name across paired controllers while retaining controller-local labels.
- Add single/multi-server view preferences and a read-only all-servers overview; route selected workspaces by server identity, keep selections per browser session, and migrate existing numeric selections and legacy pairings safely.
- Preserve old pairings through additive SQLite migrations and add focused tests for identity migration and workspace-key resolution.
- Add explicit federation roles, scoped read-only/full pairings, capability negotiation, compatibility and health states, expiring workspace sessions, credential rotation, and endpoint validation with DNS pinning.
- Restrict proxied remote sessions by read/write/terminal scopes and audit writes; read-only pairings cannot rotate their controller credentials.
- Add a server-owned Application registry with UUIDs, domain and port conflict checks, optimistic edits, soft archive/restore, and honest unknown runtime status.
- Add a responsive Applications page with read-only multi-VPS aggregation and bilingual registry documentation.
- Add durable deployment jobs with persisted steps, idempotency, leases/heartbeat, restart recovery, admin-only cancellation/retry/SSE, and atomic release-pointer rollback.
- Support public allowlisted HTTPS Git, bounded ZIP uploads, and allowlisted existing directories; reject traversal, symlinks, special files, suspicious ZIP ratios, and oversized releases, and exclude environment files.
- Stream ZIP uploads to disk, verify their SHA-256 before extraction, pin fetched Git revisions for retries, and retain five recent releases plus the active/previous versions with audited cleanup.
- Keep framework build scripts, runtime activation, HTTP health checks, and Nginx cutover disabled until their isolated runtime phases.
- Add a live runtime inventory for PHP CLI/FPM, Python, Node.js, Composer, npm/pnpm/yarn/Corepack, Nginx, and Docker with OS, memory, APT source, and deployment-disk preflight data.
- Add explicit APT simulation previews before installing runtime packs, and store safe per-application runtime and Node build-version preferences through additive migrations.
- Refresh runtime inventory after package changes and keep package installation on configured Debian/Ubuntu APT sources; ARG does not add an external repository or run a remote install script.

## v0.6.8 — 2026-10-26

### Improved
- Complete the copper glass visual treatment throughout the ARG panel, including S3 connection cards, service pages, form controls, tooltips, terminal frame and the update overlay.
- Preserve readable terminal and code output, status colors and reduced-transparency behavior.

## v0.6.8-dev.2 — 2026-10-26

### Improved
- Complete the glass treatment across S3 connection tiles, operations and package cards, form controls, tooltips, terminal chrome and the update overlay.
- Keep status colors, code output and PTY contrast legible, and honor reduced-transparency preferences.

## v0.6.8-dev.1 — 2026-10-26

### Development
- Development prerelease based on the next patch after main.

## v0.6.7 — 2026-10-26

### Fixed
- Make web updates and rollback recover correctly from cancellation, stale locks and failed activation.
- Repair Nginx resource readings, site certificate requests, maintenance access and web-worker file permissions.
- Manage existing UFW web-port rules and numbered rules without risking protected access.

### Improved
- Apply ARG's copper glass visual system across the panel shell, cards, menus and dialogs.
- Load the SSH service menu promptly, even while update and public-IP checks run.
- Extend bilingual documentation and run frontend, backend, documentation and system-script checks on both branches.

## v0.6.7-dev.11 — 2026-10-26

### Fixed
- Read Nginx application-service CPU and memory usage from cgroups when systemd counters are unavailable; show actionable errors instead of raw systemd output.
- Make update cancellation wait for the updater recovery trap and restore the prior binary and database before releasing the update lock.
- Serialize rollback with updates, retain the current release for recovery, and keep ARG stopped if recovery would leave a binary/database mismatch; generic cancellation cannot interrupt this transaction.
- Report rollback and cancellation terminal states accurately, and show branch/build mode with explicit confirmation steps in the update UI.
- Load the SSH management menu before network update/IP checks finish; refresh status asynchronously.
- Run Nginx site certificate issuance and post-issuance refresh outside the async runtime worker, so long Certbot requests do not stall panel request handling.
- Let the secret maintenance-entry URL pass the Nginx maintenance gate so it can set the administrator's bypass cookie before redirecting back to the site.
- Add an ARG Nginx site action to grant the web worker read/traverse ACLs on a site's \`/var/www\` document root without changing file ownership or asking for SSH commands.
- Reconcile HTTP/HTTPS UFW toggles with existing manually-created rules, and show/delete numbered UFW rules from ARG with stale-rule and protected-access checks.

### Improved
- Apply ARG's copper-toned glass surfaces to the shell, cards, menus, dialogs and notifications while retaining readable dense data and terminal output.
- Reduce hidden-page Nginx resource polling and support promoting a tested development build to a stable version with the release script.


## v0.6.7-dev.10 — 2026-10-26

### Improved
- Expand the bilingual panel reference with detailed dashboard controls, operational examples, safety notes, and verified S3 provider setup steps.

## v0.6.7-dev.9 — 2026-10-26

### Improved
- Add smooth article transitions, route loading feedback, and reliable scroll-to-top behavior in the documentation portal.


## v0.6.7-dev.8 — 2026-10-26

### Fixed
- Retry the MySQL user catalog without \`account_locked\` on server versions that do not expose that column, retaining lock status where available.


## v0.6.7-dev.7 — 2026-10-26

### Improved
- Simplify the documentation sidebar with collapsible sections, active-page visibility, responsive search, and accessible mobile navigation.


## v0.6.7-dev.6 — 2026-10-25

### Added
- Expand bilingual documentation to 178 navigable guides with task-by-task instructions and examples.
- Add a Documentation shortcut to the panel utility bar.


## v0.6.7-dev.5 — 2026-10-25

### Improved
- Add Windows-style storage volume cards, hierarchical disk scanning, path purpose reports, and safe cleanup navigation.

## v0.6.7-dev.4 — 2026-10-25

### Fixed
- Download ARG backups as binary data without consuming the successful response as JSON first.
- Confirm backup restoration in a clear modal with the required phrase and show operation feedback through the panel toast.

## v0.6.7-dev.3 — 2026-10-25

### Improved
- Name new S3 mount paths from the connection and bucket, with collision-safe suffixes; retain and return the actual persisted mount path.

## v0.6.7-dev.2 — 2026-10-25

### Fixed
- Show each Nginx site's maintenance state as soon as the site list loads; collapsed cards no longer default to “off”.

## v0.6.7-dev.1 — 2026-10-25

### Fixed
- Allow uploads to new S3 object keys when compatible providers return a 404 HEAD response, while still rejecting explicit missing-bucket responses.
- Distinguish missing objects from missing buckets in S3 transfer errors.

## v0.6.6-dev.3 — 2026-10-25

### Added
- Add a per-site Nginx workflow to copy the site's local document root into a unique S3 snapshot prefix, with scan limits, progress, sensitive-file exclusions, and no changes to the live site.

### Fixed
- Keep HTTP (80) and HTTPS (443) editable in UFW, including when ARG or SSH happens to listen on those ports.
- Import and remove the existing ARG-labeled website UFW rules so toggles work with rules created by the installer.

## v0.6.6-dev.2 — 2026-10-25

### Development
- Development prerelease based on the next patch after main.

## v0.6.6-dev.1 — 2026-10-25

### Included
- Carry forward the SSH update controls from \`main\`: select stable or development sources and choose whether Rust build cache is retained or cleared.

## v0.6.6 — 2026-10-25

### Fixed
- Format systemd \`MemoryMax\` limits as byte values without a \`B\` suffix, which systemd rejects on some supported releases.

## v0.6.5 — 2026-10-25

### Improved
- Let the SSH installer update flow choose the stable \`main\` or prerelease \`development\` branch before fetching and building.
- Add \`fast\` builds that retain Rust compilation cache and \`complete\` builds that clear it before and after compilation to reduce persistent disk usage.
- Allow non-interactive updates with \`arg update main complete\` or \`arg update development fast\`.

### Fixed
- Apply complete cache cleanup to both SSH and web-triggered updates without affecting initial installations.

## v0.6.5-dev.1 — 2026-10-25

### Added
- Add a ZeroSSL second-source ACME flow after Let's Encrypt failure; domain issuance prompts for EAB credentials and stores them only in a temporary root-only config file.
- Add panel SSL certificate settings with automatic discovery, safe certificate/key installation, host/IP SAN validation, and scheduled HTTPS activation.

### Fixed
- Keep HTTPS optional during installation and continue ARG setup when certificate tools are unavailable, ports conflict, or both issuers fail.
- Show a persistent HTTP transport warning on login and throughout the panel.

## v0.6.4-dev.1 — 2026-10-25

### Fixed
- Detect the server's public IPv4 address through multiple independent services and reject malformed, private, or non-public addresses.
- Retry transient Let's Encrypt API network timeouts up to three times, restore any temporarily stopped web service, and explain how to diagnose persistent outbound connectivity failures.
- Preserve the current panel configuration when certificate issuance fails; show that the failure is in ACME connectivity rather than public-IP detection.

## v0.6.4 — 2026-10-25

### Added
- Try ZeroSSL as a second ACME source after Let's Encrypt fails. The installer asks for ZeroSSL EAB credentials when needed and keeps them in a temporary root-only file.
- Add panel SSL settings for certificate/private-key paths and HTTPS port, with automatic discovery of existing Certbot certificates.

### Fixed
- Keep HTTPS optional during installation: skip at the host prompt or decline fallback, and continue installing ARG when certificate tooling, port availability, or issuance fails.
- Validate certificate expiry, host/IP SAN, and public/private key matching before activation; install the key root-owned with ARG-group read access and restart the service safely.
- Keep a persistent warning on login and panel pages when the connection uses unencrypted HTTP.

## v0.6.3 — 2026-10-25

### Fixed
- Detect the server's public IPv4 address through multiple independent services and reject malformed, private, or non-public addresses.
- Retry transient Let's Encrypt API network timeouts up to three times, restore any temporarily stopped web service, and explain how to diagnose persistent outbound connectivity failures.

## Unreleased

## v0.6.2 — 2026-10-25

### Fixed
- Keep \`/api/update/cancel\` available while update locking blocks other state-changing requests, so administrators can cancel an active panel update.

## v0.6.1 — 2026-10-25

### Added
- Calculate S3 Bucket usage from paginated object listings, cache byte and object totals in SQLite, refresh on demand or when older than 24 hours, and invalidate snapshots after uploads and deletes.
- Add background scan state, stale-result recovery after restart, and a single global scanner to bound S3 request load; remove manual entry of used-space values.
- Add an administrator-only MySQL/MariaDB manager with installation, catalog inspection, bounded SQL execution, and password reset support.
- Require fresh ARG administrator authentication for database operations; keep MySQL credentials ephemeral and never expose stored password hashes.
- Add an administrator-only server maintenance page with live host specifications, safe ARG repair, server reboot, ARG database backup download, and integrity-checked restore points.
- Add an explicit confirmation flow for full ARG removal; application data removal is scheduled outside the panel service so the uninstall can finish after the panel shuts down.

## v0.6.0 — 2026-10-25

### Added
- Add a version consistency check that compares the checked-out app, frontend manifests, and GitHub \`main\` and \`development\` branches.
- Add a development version bump command that creates ordered \`X.Y.(Z+1)-dev.N\` prereleases after the current stable version.

### Changed
- Set the stable ARG release to v0.6.0 and use the Cargo package version as the panel's embedded version.
- Require development to include main and remain on a higher prerelease version before a stable GitHub release can be published.

## v0.5.5 — 2026-10-25

### Fixed
- Treat an unavailable first-admin setup code as a normal update state so it cannot abort cleanup or leave the update reservation locked.

## v0.5.4 — 2026-10-25

### Fixed
- Preserve the update function's local state during successful cleanup and disable the exit trap before returning, even when optional setup-code display returns a nonzero status.

## v0.5.3 — 2026-10-25

### Fixed
- Run update cleanup before returning from the installer so shell exit traps do not reference expired local variables or leave the update lock behind.

## v0.5.2 — 2026-10-25

### Added
- Add administrator controls to cancel an active update and manually roll back to the last verified ARG release.
- Keep a consistent SQLite snapshot alongside the previous binary so rollback restores a compatible application/database pair.

### Fixed
- Detect abandoned updater locks from live process and systemd state, then clear stale reservations without waiting for the previous six-hour timeout.
- Verify updates with a local HTTP health check and automatically restore the previous binary and database snapshot if the new release fails to start.
- Ensure cancellation during installation restores the previous release and releases the updater lock on every exit path.

## v0.5.1 — 2026-10-25

### Changed
- Raise the VPS-to-S3 directory transfer limit to 10,000 files and 20 GiB, retain the 512 MiB per-file safety limit, and allow directory scans up to 64 levels.
- Expand directory listings to 10,000 entries so large directories are not silently omitted during transfer scans.
- Move Nginx service resource controls into an expandable section on each site card and add a systemd service picker alongside manual unit entry.

### Fixed
- Make directory upload copy behavior explicit, preserve its folder structure, and improve the partial-transfer/resume message.
- Remove resource management from the site tools modal so it is available directly from each site card.

## v0.5.0 — 2026-10-25

### Added
- Let administrators enter a Bucket's manually measured used size and optional quota when the S3 provider does not report them.
- Move VPS-to-S3 transfers into the Object page and support selecting and uploading a bounded directory tree with per-file progress.
- Show the value's source and recorded time; keep unknown capacity clearly unknown and avoid fabricated usage percentages.

### Changed
- Return the latest Bucket usage snapshot with the saved Bucket list and record manual changes in the audit log.
- Limit directory transfers to 2000 files, 2 GiB total, and 512 MiB per file; skip symlinks and allow resuming after a partial failure.
- Bump ARG to v0.5.0.

## v0.4.42 — 2026-10-25

### Added
- Create a \`development\` branch alongside \`main\`; document the stable and integration branch roles.
- Allow administrators to choose \`main\` or \`development\` in the update dialog, with branch-specific checks and installation.

### Changed
- Keep \`main\` as the default for installation and CLI updates, and persist the selected web update branch in the browser.
- Validate update branch names in the API, privileged helper, and installer; only \`main\` and \`development\` are accepted.
- Bump ARG to v0.4.42.

## v0.4.41 — 2026-10-25

### Added
- Add Quick and Complete update build modes. Quick retains Cargo's release cache for faster future updates; Complete clears it before compilation and after installing the verified binary.
- Remember the selected update mode in the browser and explain the disk-space and build-time tradeoffs before starting an update.

### Changed
- Default updates to Complete mode and keep the existing installed binary intact if compilation or health verification fails.
- Clarify that Complete mode removes retained build artifacts after the update but still needs temporary disk space while compiling.
- Bump ARG to v0.4.41.

## v0.4.40 — 2026-10-25

### Changed
- Replace the S3 split-pane view with a connection-only overview and a dedicated workspace for each selected connection.
- Add in-place S3 connection settings edits; existing credentials stay hidden and are retained unless replacement keys are entered.
- Repair ownership and restrictive permissions of legacy S3 credential files through the privileged helper instead of failing every operation with an unsafe-permissions error.
- Bump ARG to v0.4.40.

## v0.4.39 — 2026-10-25

### Added
- Add a server-side file browser and direct local-disk ↔ S3 object transfer, including optional source deletion only after a successful upload.
- Add per-Bucket Rclone mount controls with read-only mode, systemd auto-start, and safe dedicated mount/cache paths.
- Add the current ArvanCloud Iran Central 1 endpoint preset and prefill its \`ir-central1\` region from the selected panel endpoint.

### Changed
- Report cloud mounts separately from local disk capacity; bound Rclone's local VFS cache to 1 GiB.
- Prevent deleting or detaching a Bucket/connection while its mount is active; refuse local-download overwrites and require explicit confirmation before replacing an S3 object.
- Bump ARG to v0.4.39.

## v0.4.38 — 2026-10-25

### Security
- Reject oversized account passwords before Argon2 hashing and add per-IP login throttling for remote clients.
- Trust forwarded HTTPS only from loopback proxies; mark native TLS requests internally and discard client-supplied trust markers on HTTP.
- Restrict the privileged helper runtime directory so the ARG service account cannot replace the root-owned socket path.
- Bound privileged helper request reads and concurrent operations to protect root memory and worker capacity.
- Block additional cloud metadata addresses from S3 endpoints even when private endpoints are enabled.
- Reject a symlink selected as the root of a ZIP archive.

### Changed
- Bump ARG to v0.4.38.

## v0.4.37 — 2026-10-25

### Added
- Add a custom Dashboard Fast Action editor and runner. Custom commands run as the ARG service account, are limited to 30 seconds and 2048 bytes, cap captured output, and write start/result audit entries.
- Add an optional “نمایش لاگ‌های اجرا” setting for custom actions.

### Changed
- Replace the Dashboard Fast Actions Access Log option with “نمایش لاگ‌های اجرا”; keep the optional site Error Log display.

## v0.4.36 — 2026-10-25

### Performance
- Reduce release build time on VPS hardware by disabling costly link-time optimization and using 16 parallel Rust codegen units; keep normal optimized release code generation for runtime performance.
- Avoid repeating GitHub version checks during an active update; poll the local helper's progress and wait for its successful service health check before refreshing.
- Bound update-check network timeouts and run update status checks asynchronously so slow GitHub responses do not block a Tokio worker.
- Reduce background monitoring traffic by polling dashboard alerts every 30 seconds and caching thresholds for 60 seconds.

## v0.4.35 — 2026-10-25

### Added
- Add dashboard Fast Actions and per-site Nginx action buttons with custom names and icons, editable actions, execution results, server status, and optional site logs.
- Add a bounded deep disk scan mode for larger paths, with the selected scan limits shown in the interface.

### Changed
- Keep terminal tabs and connections mounted while navigating between pages; improve empty-state behavior, fullscreen fallback, scrollback, and output-following.
- Make long-range charts identify dates as well as times, and make traffic-volume bars reveal the exact bucket timestamp and RX/TX values when selected.
- Make resource threshold notices dismissible and automatically clear them when readings return below the alert range.
- Apply subtle ARG geometric patterns and softer interaction feedback across the panel, while honoring reduced-motion preferences.
- Protect active panel and SSH firewall ports and prevent changes to protected systemd services.

## v0.4.34 — 2026-10-25

### Added
- Add the first ARG S3 storage manager with separate AWS S3, Cloudflare R2, ArvanCloud, ParsPack and other S3-compatible presets.
- Add protected S3 credential storage, connection testing, Bucket listing/association, gated Bucket creation, paginated Object browsing, streaming upload/download and confirmed Object deletion.
- Add an all-mount storage inventory to the dashboard and System page, separating local, remote, cloud/FUSE and virtual filesystems and avoiding duplicate block-device totals.
- Add ZIP creation and download for regular files/folders in the server file manager, plus safe extraction into a new folder with path traversal and overwrite protections.

### Security and limits
- Keep cloud usage/quota unknown until a trustworthy measurement source is implemented; do not display estimated capacity as real data.
- Bound archive input/expanded data and S3 upload/download sizes; reject symlinks and special archive entries.
- Document current S3 scope and limitations in \`docs/S3_STORAGE.md\`. Rclone/S3FS mounts, sync jobs, usage scans, contracts/reminders and MinIO deployment remain future work.

## v0.4.33 — 2026-10-25

### Security
- When HTTPS is configured, stop opening ARG's plain HTTP backend port in UFW; retain only the HTTP challenge and HTTPS listener rules.
- Protect the active ARG bind ports and the effective SSH ports reported by \`sshd -T\` from managed firewall rules.
- Require the current administrator password before opening Root Terminal. Its short-lived, one-use ticket is tied to the exact login session and rate-limited.
- Refuse systemd stop, restart, disable, and mask actions for ARG, SSH, and core network services.

### Quality
- Add GitHub Actions checks for Rust tests/build checks and frontend type/build checks.
- Make file-manager test directories unique so parallel test execution is reliable.

## v0.4.32 — 2026-10-25

### Fixed
- Show five server-driven update stages and an unmistakable copper active loading state instead of optimistic stage jumps.
- Keep the phase monotonic for each running job and trust the privileged helper rather than arbitrary browser timeouts.
- Hold the update overlay through reload and display new release notes only after the refreshed app authenticates.
- Prevent an ordinary manual refresh from reopening the successful-update changelog.
- Reject remote sign-ins over plaintext HTTP and keep the HTTP listener on loopback when TLS is configured.
- Include the rebuilt React assets in the release bundle so small VPS installs do not need Node.js or an extra frontend build.

### Documentation
- Add an outstanding security risks and remediation report in docs/SECURITY_REVIEW.md.

## v0.4.31 — 2026-10-24

### Changed
- Add a bounded disk-usage scanner that reports filesystem capacity and usage, largest directories, and largest files for a selected path.
- Stop scans after 12 seconds or 100,000 entries, skip symbolic links and nested filesystems, and let administrators open large folders in the file manager.

## v0.4.30 — 2026-10-24

### Changed
- Give each Nginx site a dedicated Certbot certificate lineage, avoiding interactive expansion of unrelated certificates during noninteractive SSL issuance.
- Add a secure Laravel/PHP-FPM visual preset with an index.php-only handler, protected files, IPv4/IPv6 HTTPS listeners, and HTTP redirect.
- Add per-site access-log traffic samples with hourly response-volume charts, read-only GitHub repository checks, and direct access to each site's document root in the file manager.
- Add per-connection Nginx response rate controls and dedicated access/error log defaults to the visual editor.
- Cache Docker, APT, and systemd listing requests briefly to reduce repeated work when switching pages; invalidate after management actions.

## v0.4.29 — 2026-10-24

### Added
- Let administrators set the panel's IANA time zone, with validation and a live time preview.
- Apply the selected zone to traffic charts, resource charts, audit events, file timestamps, and account sessions.

## v0.4.28 — 2026-10-24

### Improved
- Adapt the file manager toolbar, path controls, and search layout to narrow screens.
- Replace the wide file table on phones with compact file cards and touch-friendly actions.

## v0.4.27 — 2026-10-24

### Fixed
- Prevent the privileged helper from claiming a root terminal's PTY as its own controlling terminal, allowing the root shell to attach cleanly.

## v0.4.26 — 2026-10-24

### Changed
- Add a dedicated, administrator-only server file manager with directory browsing, search, create, rename, copy, move, paste, delete, upload, download, text editing, and Unix permission controls.
- Route filesystem work through the privileged helper with path validation, upload and edit limits, confirmed destructive actions, protected system roots, and symlink-safe delete/copy behavior.

## v0.4.25 — 2026-10-24

### Changed
- Split the Rust backend into focused \`docker\`, \`metrics\`, and \`nginx\` modules, keeping each manager's API handlers and privileged operations together.
- Move the API route table into \`routes\` and keep shared authentication, database state, and application startup in the application entry point.

## v0.4.24 — 2026-10-24

### Added
- Add Docker Engine and Compose management for containers, images, networks, and persistent volumes.
- Add container creation, lifecycle controls, logs, inspect, command execution, port and environment configuration.
- Add image pull, build, tag, push, removal, and unused image cleanup.
- Add Docker service installation, lifecycle controls, Compose project actions, and confirmation-gated destructive cleanup.

## v0.4.23 — 2026-10-24

### Changed
- Add admin-only \`Terminal Root\` sessions with an explicit danger confirmation, separate PTYs, and cleanup when a tab disconnects.
- Remove the terminal tab cap and record root terminal openings in the audit log.
- Show full semantic versions prominently in the update page and dialog, with short commit IDs as supporting details.

### Fixed
- Read Nginx site contents from both current and legacy privileged-helper response formats, so reconciliation previews show the actual on-disk configuration.
- Compare the database snapshot against the file contents themselves and keep Nginx visual settings when syncing the file into ARG.
- Close a successful reconciliation without reopening the same confirmation modal during the follow-up refresh.
- Include the remote Cargo package version in update-check responses while retaining commit-based update detection.

## v0.4.22 — 2026-10-24

### Improved
- Group APT operations into clear update and cleanup sections with compact responsive action cards.
- Show the exact system command beside each APT action and animate an indeterminate progress bar inside the active card while it runs.
- Add paginated grid and list views for installed packages, with search and clearer per-package actions.

## v0.4.21 — 2026-10-24

### Fixed
- Fix a quoting error in the update helper that prevented update checks and web-triggered updates from starting.
- Sanitize update-state messages without leaving an unterminated shell quote.

## v0.4.20 — 2026-10-24

### Improved
- Show a soft, responsive page skeleton during navigation and let page content fade upward smoothly when ready.
- Replace the sharp pulse skeleton with a gentle shimmer and tune shared button and card motion to use softer easing.
- Add initial loading skeletons to APT and system service management while their data is fetched.

## v0.4.19 — 2026-10-24

### Fixed
- Keep SSH and panel access rules ahead of UFW rules before enabling the firewall, and allow website ports 80 and 443 by default unless an explicit inbound deny rule exists.
- Make TCP ports 80 and 443 configurable from the firewall page, with one-click HTTP and HTTPS allow rules.

## v0.4.18 — 2026-10-24

### Fixed
- Persist authoritative updater job phases and final success/failure state so polling can distinguish a completed build, a failed build, and an orphaned update marker.
- Release the blocking update overlay into a recovery state after a stalled job, while preserving the manual full-refresh and retry controls.
- Restyle update progress as clear build stages, unify the log-details control with the ARG theme, and auto-follow fresh log output until the reader scrolls away.
- Turn terminal titlebar controls into close, minimize, and maximize actions; minimizing keeps the PTY alive, and maximizing can be exited with Escape.
- Smooth page and sidebar transitions and remove the Persian product label from the sidebar/header brand lockup.

## v0.4.17 — 2026-10-23

### Fixed
- Finalize the update flow from confirmed version and process changes even when the update marker lingers, then release the blocking overlay before a cache-busting full reload.
- Keep the completion dialog visible with the changelog briefly before refresh; show a completion toast after reload instead of reopening the same modal.

## v0.4.16 — 2026-10-23

### Added
- Added an APT operations page for repository refresh, upgrade, full upgrade, package install and removal, and cache cleanup, with installed package search and version details.
- Added a systemd services page with service discovery, status filters, start/stop/restart/reload, enable/disable, mask/unmask, reset-failed, status inspection, and journal logs.
- Serialized APT transactions and coordinated package and service operations with panel updates; privileged command arguments are allowlisted and validated.

### Fixed
- Corrected the locked checksum for \`slab 0.4.11\`, which prevented Cargo from verifying and building the project.

## v0.4.15 — 2026-10-23

### Added
- Added independent terminal tabs with per-session PTY connections, editable names, close controls, and a polished tab bar.
- Added per-tab terminal copy, cut-selection, paste, clear, and reconnect actions with keyboard guidance for Ctrl+Shift+C and Ctrl+Shift+V.

### Fixed
- Kept terminal connection attempts in a calm connecting state and delayed failure messaging until the WebSocket actually closes.
- Prevented a closing old terminal socket from overriding the connection state of a newly reopened session.
- Added live updater logs, an explicit failure state, and a completion refresh flow with a manual full-refresh fallback.

## v0.4.14 — 2026-10-23

### Fixed
- Replaced premature UFW and Nginx not-installed states with page skeletons until initial status data loads.
- Kept installation controls hidden when a service status request fails, and added a retry state for unavailable status.

## v0.4.13 — 2026-10-23

### Changed
- Moved CPU, memory, and disk alert threshold controls to the main Settings page and removed them from the dashboard and System page.
- Added a shared top-of-page resource alert that polls live server metrics and shows warning or critical status when configured thresholds or the critical level are reached.

## v0.4.12 — 2026-10-23

### Changed
- Added subtle ARG girih, chart-grid, card-corner, and bottom-trim patterns from the supplied design packs to the dashboard surfaces without changing layout breakpoints or component dimensions.

## v0.4.11 — 2026-10-23

### Added
- Shared update reservations and live progress polling so every signed-in panel session sees the same update overlay and is temporarily locked.
- Serialized panel mutations with update startup, rejected conflicting write requests, and closed active web terminal sessions when an update begins.
- Kept an explicit cache-clearing full refresh action available in the completion dialog if the automatic refresh does not restore the panel correctly.

## v0.4.10 — 2026-10-23

### Fixed
- Refresh the updater's server instance ID immediately before scheduling installation so restart detection does not rely on stale update-check state.
- Replaced the full-width gradient animation with a bounded, smoothly moving progress indicator and explicit update stages with elapsed time.
- Aligned updater polling timeout with its 10-minute status message.

## v0.4.9 — 2026-10-23

### Added
- Added persistent per-site Nginx configuration snapshots and visual builder settings in the database, with SHA-256 drift checks against files on each refresh.
- Added an explicit reconciliation dialog to apply the database version to the Nginx file or import the current file into the database; Nginx validation runs before database-to-file sync.
- Expanded visual site creation and editing with request size and timeout controls, dedicated access/error logs, server-token and security-header toggles, and inline help tooltips.
- Added per-context manual Nginx directives for HTTP, server, default location, and TLS, with saved builder settings restored when reopening managed sites.

## v0.4.8 — 2026-10-23

### Changed
- Fixed stale panel bundles after updates by content-hashing frontend assets and disabling browser and proxy caching for embedded panel files.

## v0.4.7 — 2026-10-23

### Changed
- Replaced the jumping update bar with a smooth continuous indeterminate indicator and phase-specific status text.
- Update completion now waits for the panel process to restart and return with its new process identity, instead of treating a synchronized source revision as a completed installation.

## v0.4.6 — 2026-10-23

### Changed
- Made chart SVG coordinates follow the rendered container size, preserving chart proportions across narrow and wide layouts instead of stretching to a fixed view box.

## v0.4.5 — 2026-10-23

### Changed
- Added immediate loading feedback to page refresh controls and update actions, including a spinning icon, disabled state, and progress label while requests are running.

## v0.4.4 — 2026-10-23

### Changed
- Redesigned distribution information cards used for role counts, network traffic, and recent audit events with clearer hierarchy, spacing, and proportional bars.

## v0.4.3 — 2026-10-23

### Changed
- Reduced CPU work for long history ranges by sampling rows in SQLite before converting them to JSON, and changed live dashboard polling from every 5 seconds to every 15 seconds.
- Added an update completion fallback that clears browser Cache Storage and Service Worker registrations, then reloads the panel with a cache-busting URL.

## v0.4.2 — 2026-10-23

### Changed
- Replaced the temporary notice banner with a consistent RTL toast system based on shadcn Sonner.
- Styled toast messages for the ARG theme and distinguish successful actions from errors.

## v0.4.1 — 2026-10-23

### Added
- Added a visual Nginx site builder with static, reverse proxy, PHP-FPM, and redirect presets, TLS certificate and key selection, repeatable directives, location blocks, and configuration preview.
- Added per-site descriptions, safe site renaming, enabled-state management, domain aliases and wildcards, DNS checks, validated configuration updates, site duplication, configuration downloads, and access/error log viewing.
- Added custom chart ranges from 5 minutes to 30 days, date/time filtering, compact network interface cards, active-interface filtering, and expandable interface details.
- Added a real interactive server terminal with live input/output, resize, reconnect, and scrollback.

### Changed
- Improved chart history and network monitoring layouts, page navigation and scroll behavior, and terminal session isolation.
- Restricted terminal access to administrators and same-origin connections, limited concurrent sessions, and run terminal shells as the ARG service account without ambient capabilities.

## v0.4.0 — 2026-10-23

### Changed
- Rebranded the service, CLI, installer, source package, repository, and user-facing documentation as ARG.
- Replaced the installer and interface accent palette with ARG copper, charcoal, teal, and slate colors.
- Added a migration path from prior installations that preserves the database, HTTPS files, Nginx site, and managed firewall rules.

## v0.3.0 — 2026-10-23

### Added
- Added Nginx site management, including site enable and disable controls and validated configuration editing.
- Added the shared update dialog and blocking progress overlay for panel updates.
- Added live dashboard metrics with saved resource and network charts.
- Added automatic reload after the new version is confirmed, followed by this release note.

### Changed
- Reworked the dashboard and navigation with the ARG visual identity.
- Improved update status and retry controls.
- Made the shared update dialog available from the top bar and Settings page.
`;export{e as default};
