# ARG Change Log

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
