## v0.13.0 — 2026-10-23

### Added
- Added an interactive xterm.js server terminal backed by a real PTY with live input, output, resize, reconnect, and scrollback.
- Restricted terminal sessions to administrators, same-origin WebSocket connections, and four concurrent sessions.
- Terminal shells run as the ARG service account with ambient capabilities cleared and no-new-privileges enabled; session process groups are terminated on disconnect.

## v0.12.0 — 2026-10-23

### Changed
- Added a subtle fade-up transition when switching between panel pages, while respecting reduced-motion preferences.

## v0.11.0 — 2026-10-23

### Changed
- Reset the page scroll position whenever navigation changes the active page, including browser hash navigation.

## v0.10.0 — 2026-10-23

### Changed
- Moved network traffic monitoring above the interface cards and compacted the card layout.

### Added
- Added an active-interface filter and expandable per-interface details for addresses, MTU, link speed, packet counts, errors, drops, and carrier state.

## v0.9.0 — 2026-10-23

### Added
- Expanded resource and network history filters to 5 minutes, 1 hour, 24 hours, 7 days, and 30 days.
- Added custom start and end date/time filters for charts, with validation and bounded sample density for long ranges.

## v0.8.0 — 2026-10-23

### Added
- Added per-site domain management for a primary hostname and aliases, including wildcard hostnames.
- Added DNS A/AAAA resolution checks and clear guidance when domains do not resolve.
- Domain changes update every `server_name` directive in the site and are applied only after Nginx configuration validation succeeds.

# ARG Change Log

## v0.7.0 — 2026-10-23

### Added
- Added site-specific tools for viewing detected Nginx settings, downloading config, and duplicating a site as a disabled draft.
- Added per-site access and error log viewing, restricted to files under `/var/log/nginx`.

## v0.6.0 — 2026-10-23

### Added
- Added persistent per-site descriptions and a dialog to edit a site's name and notes.
- Added safe Nginx site renaming that updates the enabled symlink and validates the resulting configuration.

## v0.5.0 — 2026-10-23

### Added
- Added a visual Nginx site builder with static, reverse proxy, PHP-FPM, and redirect presets.
- Added configurable TLS certificate and private key paths with a restricted SSL file browser.
- Added repeatable custom directives and location blocks, plus generated configuration preview.
- Added common server inputs for document root, proxy target, PHP-FPM socket, Gzip, and request size.

### Changed
- The visual builder can prefill common fields from an existing site and transfers generated output to the validated editor before saving.

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
- Applied the ARG visual identity across the frontend while preserving the ARG service, CLI, installer, API, and database identifiers.
- Improved update status, installed version details, and retry controls in Settings.
