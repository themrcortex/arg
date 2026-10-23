# ARG Change Log

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
