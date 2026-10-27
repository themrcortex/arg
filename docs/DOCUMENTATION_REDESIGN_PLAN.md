# ARG documentation redesign plan

## Audit snapshot

The documentation is a Vite + React + TypeScript application in `docs-site/`. It imports Markdown files from `docs/fa/` and `docs/en/` at build time, sanitizes rendered HTML with DOMPurify, and uses `marked` for Markdown. The site is deployed as a GitHub Pages artifact from `.github/workflows/docs-pages.yml`; Vite's base path is `/arg/`, with query-string navigation (`?page=<stable-id>`) to preserve direct access and refresh compatibility.

The current catalog contains 178 stable article IDs and matching Persian and English Markdown files. It grouped these into nine flat numeric categories; many entries were created as one-task numbered guides and ended up in broad catch-all sections such as “Operations & administration”. The interface already has keyboard search, a mobile drawer, a TOC, theme and language controls, copyable code blocks, and previous/next links, but navigation context was flat and pagination followed catalog declaration order. Search matches only title and summary. The displayed version was `0.6.7-dev.11`, while `Cargo.toml` and the latest release commit identify `0.6.8`.

Code review confirms the documented product areas represented by existing articles and handlers include host metrics and disk analysis, network and port inspection, UFW, Nginx sites and TLS configuration, Docker operations, file management, S3-compatible storage, MySQL/MariaDB, APT and systemd, terminal access, users/sessions/audit, settings, backups, updates and rollback. This redesign reorganizes existing material; it does not add operational claims for multi-server control, runners, or advanced application deployment. API reference is omitted because no stable public API contract was identified in the documentation catalog.

## Problems found

- Numeric group identifiers and nine top-level buckets did not communicate the subject hierarchy.
- Related articles were separated from their product overview pages, while very long lists of small guides overwhelmed the sidebar.
- Previous/next navigation followed source declaration order, which could cross subject boundaries unexpectedly.
- Sidebar expansion state did not represent a parent/child hierarchy or persist reader preferences.
- Version labels were stale.
- Local Persian fonts were referenced from the domain root, which is incorrect under the `/arg/` GitHub Pages base path.
- The existing query-string route is not a broken limitation: it works for refresh and direct links without a server-side history fallback. Existing `?page=` URLs remain the canonical compatibility route.

## Proposed information architecture

Use semantic category IDs with no more than three visible levels:

1. Getting started: introduction, install, CLI.
2. Dashboard & monitoring: metrics, alerts, actions, host and disk analysis.
3. Networking & websites: network, ports, firewall, Nginx.
4. Docker & containers.
5. Files & data: file manager, S3, databases.
6. System administration: host, packages/services, terminal.
7. Security & maintenance: accounts, audit, settings, updates, maintenance, troubleshooting.
8. Developers & reference: architecture, development, glossary.

The catalog keeps stable page IDs and Markdown filenames. All proposed entries are drawn from current catalog/content; absent feature pages are not invented. Persian and English labels use the same tree.

## Migration and implementation phases

1. Inventory routes, content files, current UI, deployment and release version; verify broad product areas against implementation.
2. Introduce semantic IDs and a typed, reusable navigation tree with category, subsection and page levels; retain `?page=` URLs.
3. Reorder reading/pagination from the semantic tree; make parent context available for breadcrumbs and active navigation.
4. Apply centralized light/dark design tokens, responsive hierarchy styling and the correct Pages asset base path.
5. Validate TypeScript, production build and bilingual page/file integrity; review deployment workflow compatibility.

## Acceptance criteria

- All existing stable IDs resolve to Persian and English source files.
- Sidebar has eight semantic top-level categories, nested sections, active state, keyboard-operable disclosure controls, ancestor expansion and persisted expansion preferences.
- Search filters entries without removing hierarchy; old `?page=` deep links, refresh and browser history continue to work.
- Previous/next follows semantic reading order and never moves between unrelated categories unexpectedly.
- Light and dark themes use shared tokens with readable contrast; Persian and English direction, typography and code direction remain correct.
- GitHub Pages builds with the `/arg/` base path; release/version labels match the source release.
- No undocumented planned feature is presented as available.
