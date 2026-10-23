# ARG Change Log

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
