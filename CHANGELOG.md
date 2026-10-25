# ARG Change Log

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
- Create a `development` branch alongside `main`; document the stable and integration branch roles.
- Allow administrators to choose `main` or `development` in the update dialog, with branch-specific checks and installation.

### Changed
- Keep `main` as the default for installation and CLI updates, and persist the selected web update branch in the browser.
- Validate update branch names in the API, privileged helper, and installer; only `main` and `development` are accepted.
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
- Add the current ArvanCloud Iran Central 1 endpoint preset and prefill its `ir-central1` region from the selected panel endpoint.

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
- Document current S3 scope and limitations in `docs/S3_STORAGE.md`. Rclone/S3FS mounts, sync jobs, usage scans, contracts/reminders and MinIO deployment remain future work.

## v0.4.33 — 2026-10-25

### Security
- When HTTPS is configured, stop opening ARG's plain HTTP backend port in UFW; retain only the HTTP challenge and HTTPS listener rules.
- Protect the active ARG bind ports and the effective SSH ports reported by `sshd -T` from managed firewall rules.
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
- Split the Rust backend into focused `docker`, `metrics`, and `nginx` modules, keeping each manager's API handlers and privileged operations together.
- Move the API route table into `routes` and keep shared authentication, database state, and application startup in the application entry point.

## v0.4.24 — 2026-10-24

### Added
- Add Docker Engine and Compose management for containers, images, networks, and persistent volumes.
- Add container creation, lifecycle controls, logs, inspect, command execution, port and environment configuration.
- Add image pull, build, tag, push, removal, and unused image cleanup.
- Add Docker service installation, lifecycle controls, Compose project actions, and confirmation-gated destructive cleanup.

## v0.4.23 — 2026-10-24

### Changed
- Add admin-only `Terminal Root` sessions with an explicit danger confirmation, separate PTYs, and cleanup when a tab disconnects.
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
- Corrected the locked checksum for `slab 0.4.11`, which prevented Cargo from verifying and building the project.

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
