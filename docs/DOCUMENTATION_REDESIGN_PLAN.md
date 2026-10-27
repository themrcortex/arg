# ARG documentation redesign plan

## Audit snapshot

The documentation is a Vite + React + TypeScript application in `docs-site/`. A predev/prebuild script copies canonical Markdown into static, per-page assets under `public/content/` and generates a lightweight localized search index. The reader fetches only the current article and sanitizes rendered HTML with DOMPurify after parsing it with `marked`. The site is deployed as a GitHub Pages artifact from `.github/workflows/docs-pages.yml`; Vite's base path is `/arg/`, with query-string navigation (`?page=<stable-id>`) to preserve direct access and refresh compatibility.

The pre-consolidation catalog had 194 page entries. Review of both language folders found 162 one-task articles per language generated from a repeated template: generic prerequisites, generic click/verify steps, and a “technical note” that was sometimes unrelated to the page title. Several even named the wrong product area. The remaining 32 articles were the useful topic references and developer material. These task pages are now consolidated into 34 bilingual topic pages, including maintenance and Cloudflare DNS guides verified against the actual UI and API. Legacy `?page=guide-…` URLs resolve to their canonical topic. Search indexes only catalog pages, so retired template pages no longer produce duplicate results.

Code review confirms the documented product areas represented by existing articles and handlers include host metrics and disk analysis, network and port inspection, UFW, Nginx sites and TLS configuration, Docker operations, file management, S3-compatible storage, MySQL/MariaDB, APT and systemd, terminal access, users/sessions/audit, settings, backups, updates and rollback. The development branch adds implementation and bilingual documentation for server federation, the application registry, runtime management, Laravel and Node runners, static/SSR publishing, deployment jobs, and application resources. These topics are included in the redesigned hierarchy because they now exist in the development source; review their release status before describing them as stable. The branch also has a public API article backed by registered routes.

The audit also found an implemented Cloudflare DNS manager in the ARG dashboard, backed by `frontend/src/DnsPage.tsx`, `src/dns.rs`, and `src/dns_vault.rs`, but no dedicated operator guide. The new bilingual guide covers its admin-only access, least-privilege token setup, zone validation, record changes, secret handling, and explicit limits, including no propagation check or DNS-01 support.

## Problems found

- Long repeated task lists overwhelmed the sidebar and exposed unrelated one-action “pages” as separate destinations.
- Several task pages repeated the same instructions, did not add independent value, or directed readers to UI areas that did not match the described feature.
- Previous/next navigation followed source declaration order, which could cross subject boundaries unexpectedly.
- Multiple open sidebar sections could create a distracting, oversized menu.
- Version labels were stale.
- Local Persian fonts were referenced from the domain root, which is incorrect under the `/arg/` GitHub Pages base path.
- The existing query-string route is not a broken limitation: it works for refresh and direct links without a server-side history fallback. Existing `?page=` URLs remain the canonical compatibility route.

## Proposed information architecture

Use semantic category IDs with no more than three visible levels:

1. Getting started: introduction, install, CLI.
2. Dashboard & monitoring: metrics, alerts, actions, host and disk analysis.
3. Networking & websites: network, ports, firewall, Nginx, and Cloudflare DNS management.
4. Docker & containers.
5. Files & data: file manager, S3, databases.
6. System administration: host, packages/services, terminal.
7. Security & maintenance: accounts, audit, settings, updates, maintenance, troubleshooting.
8. Developers & reference: architecture, development, public API, glossary.
9. Application platform: platform overview and roadmap, application registry, runtimes, runners, deployment jobs, and resources.

The catalog keeps stable IDs for canonical topic pages and resolves retired one-task IDs through explicit prefix aliases. Each retained feature topic has one useful reference with focused headings instead of many repetitive pseudo-tutorials. All entries are checked against current source; absent feature pages are not invented. Persian and English labels use the same tree.

## Migration and implementation phases

1. Inventory routes, content files, current UI, deployment and release version; verify broad product areas against implementation.
2. Consolidate repeated generated task articles into bilingual feature references; retain old `?page=guide-…` links through aliases.
3. Limit the sidebar to one open category and one open subsection, pin it flush to the logical edge, and keep its active route and keyboard controls clear.
4. Add stable permalink anchors to page titles and every Markdown heading; generate full-text search from the canonical catalog only. Add the missing Cloudflare DNS operator guide based on the current implementation.
5. Apply centralized light/dark design tokens, responsive layout and the correct Pages asset base path.
6. Validate TypeScript, production build, navigation aliases, heading anchors and bilingual page/file integrity; review deployment workflow compatibility.

## Acceptance criteria

- All 34 canonical pages resolve to Persian and English source files; retired task IDs resolve to their canonical topic.
- Sidebar has nine semantic top-level categories and opens at most one category/subsection path at a time. It is flush to the logical page edge, has square corners, active state, and keyboard-operable disclosure controls.
- Search indexes the canonical pages and their real Markdown content; old `?page=` deep links, refresh and browser history continue to work.
- Every article title and Markdown heading has a stable, keyboard-accessible permalink anchor.
- Previous/next follows semantic reading order and never moves between unrelated categories unexpectedly.
- Light and dark themes use shared tokens with readable contrast; Persian and English direction, typography and code direction remain correct.
- GitHub Pages builds with the `/arg/` base path; release/version labels match the source release.
- No undocumented planned feature is presented as available.
