# ARG UI Redesign — Audit and Delivery Plan

**Audit date:** 2026-10-27  
**Audited branch/version:** `development` / `0.6.9-dev.12`  
**Scope:** Existing React application and user-visible flows. This is based on checked-in source; a route or API is not inferred from a proposed feature.

## 1. Current Frontend Map

ARG is a Vite + React 19 + TypeScript single-page application. Navigation is hash-based (`frontend/src/navigation.ts`) and page selection is handled by `frontend/src/App.tsx`; there is no React Router. `main.tsx` mounts the app in `StrictMode`, sets the document to Persian RTL in `frontend/index.html`, and registers the production service worker. The API client lives in `frontend/src/lib/api.ts`.

The shell, authentication, workspace selection, navigation, update dialog/overlay, global notices, lazy routes, and route skeletons currently share the 1,134-line `App.tsx`. Core overview, system, network, firewall, Nginx, account, user, audit, and settings pages share the 2,053-line `pages.tsx`. These files contain real API operations and should be decomposed only along tested UI boundaries; a visual redesign must not rewrite their API contracts.

### Routes and page owners

| Route (`#/…`) | Current owner | Actual responsibility |
| --- | --- | --- |
| `dashboard` | `pages.tsx` → `DashboardPage` | Server KPIs/charts, alerts, storage, recent/quick operations. |
| `system` | `pages.tsx` → `SystemPage` | Host metrics, mounts, storage inventory and resource history. |
| `nodes` | `NodesPage.tsx` | ARG node pairing and workspace mode; admin only. |
| `apps` | `ApplicationsPage.tsx` plus `Application*Panel.tsx` | App records, deployment, source, backup, monitoring, resources, Nginx binding and workflow drafts; admin only. |
| `maintenance` | `MaintenancePage.tsx`, `PlatformReadinessPanels.tsx`, `HostSecurityPanel.tsx`, `ExtensionSecurityPanel.tsx` | Host controls, ARG backups/restore, Doctor, API keys and readiness/security panels; admin only. |
| `mysql` | `MysqlPage.tsx` | MySQL connection gate, databases/users/settings, SQL, presets and service actions; admin only. |
| `network` | `pages.tsx` → `NetworkPage` | Interfaces and traffic history. |
| `ports` | `pages.tsx` → `PortsPage` | Listening ports and port controls. |
| `firewall` | `pages.tsx` → `FirewallPage` | UFW state, installation, rules and protected-port behavior. |
| `nginx` | `pages.tsx` → `NginxPage`, `nginx/*` | Nginx sites, config, site actions, traffic, maintenance, SSL, resources and S3 export. |
| `docker` | `DockerPage.tsx` | Docker installation/status and container, image, volume, network and Compose operations. |
| `files` | `FileManagerPage.tsx`, `components/file-manager/FileManagerBrowser.tsx` | Server file browser, uploads and filesystem actions. |
| `s3` | `S3StoragePage.tsx` | Provider connections, buckets/objects, transfers, usage and mount controls. |
| `dns` | `DnsPage.tsx` | Cloudflare DNS connection and record management. |
| `terminal` | lazy `TerminalPage.tsx` | Persistent terminal sessions and Root Terminal flow; remains mounted while navigating away. |
| `packages` | `AdminOpsPage.tsx` → `AptPage` | APT package operations and runtime inventory/actions. |
| `services` | `AdminOpsPage.tsx` → `ServicesPage` | systemd service status/actions. |
| `users` | `pages.tsx` → `UsersPage` | ARG users and access control; admin only. |
| `audit` | `pages.tsx` → `AuditPage` | Operation history; admin only. |
| `settings` | `pages.tsx` → `SettingsPage` | Panel preferences, TLS certificate paths, thresholds and updater entry. |
| `account` | `pages.tsx` → `AccountPage` | User identity, password, sessions and logout. |
| `bug-report` | `BugReportPage.tsx` | Diagnostics and GitHub bug report submission. |

`AllServersPage.tsx` is a real conditional workspace view when the selected workspace is `all`; it is not a separate hash route and is currently read-only. Several page-owned subviews are tabs, accordions, drawers or dialogs rather than routes.

## 2. Existing Components, Styling, and Interaction Patterns

- The project already has Radix-backed shadcn-style primitives under `frontend/src/components/ui`: Alert, Avatar, Badge, Button, Card, Dialog, Dropdown Menu, Input, Label, Progress, Select, Separator, Skeleton, Sonner, Switch, Table, Tabs and Tooltip. Lucide icons are used throughout. No new UI dependency is justified by this audit.
- `Button`, `Card`, `Dialog`, `Sonner` and utility classes already carry ARG-specific styling. There is no shared `GlassDrawer`, `Stepper`, `PageHeader`, `FilterBar`, or `EmptyState` primitive yet; several pages implement their own versions.
- `index.css` has dark-only HSL and `--arg-*` variables, a copper identity (`#dda46f`), glass/card/sidebar/popover rules, chart decoration, reduced-motion and reduced-transparency support. Tailwind is configured for `darkMode: ['class']`, but source contains no user-selectable/persisted light/dark theme. Current background and many component details still use literal dark colors.
- A broad `.arg-main-canvas` selector glass-treats nested `bg-card`/`bg-background` elements. That makes visual treatment inconsistent at deep nesting and applies blur widely, increasing GPU cost and potentially hurting dense tables. Theme colors, spacing, border widths, shadows, and semantic status colors are not yet governed by one complete token set.
- The desktop sidebar is fixed/compactable (224/56 px), with tooltip labels in collapsed mode; mobile navigation is an off-canvas drawer with overlay. Main content already uses responsive grids and dialog sizing, but every major page still needs breakpoint/overflow review.
- The document is Persian RTL only. LTR direction is applied selectively for commands, addresses, endpoints and code. Logical CSS properties are mixed with physical `left/right` styles; keyboard focus, ARIA and error/loading states exist in many flows but are not uniform.
- Loading skeletons, Sonner toasts, error alerts, confirmation dialogs and a page fade exist. The shell starts a short delayed route-ready transition. Async behavior is a mix of local state, polling, API calls and cached reads; each redesign must keep the server response authoritative and clean up timers/listeners.
- `frontend/src/lib/heavy-cache.ts` deduplicates/caches selected expensive reads. Page navigation and lazy-loading must not trigger additional host scans or duplicate requests.

## 3. Current UX Problems Verified in Source

1. **Theme gap:** only the dark token set is active; no complete light theme or theme control exists despite Tailwind's class mode.
2. **Shell coupling:** global auth, navigation, dialogs, updater lifecycle, update polling and route composition sit in `App.tsx`, making shell changes risky and hard to review.
3. **Core page density:** `pages.tsx` combines unrelated high-interaction pages and contains many bespoke `Card`/form/table/dialog structures.
4. **Uneven surface treatment:** the stylesheet applies glass via selectors that infer intent from utility class substrings. Nested form surfaces, tables, charts and management controls can receive the same treatment as summary cards.
5. **No audited visual language across all modules:** specialized pages (`ApplicationsPage`, Docker, S3, files, MySQL, terminal, admin operations) define their own layouts and repeated empty/loading/action patterns.
6. **Wizard coverage is incomplete:** S3 already has a short step state in a dialog; Nginx SSL and new-site setup use page-local dialogs/forms; Docker management is action-oriented rather than a unified staged deployment; backup restore already has a confirmation dialog; panel update already has a persisted/shared overlay and stage model. We must improve the actual existing flows instead of replacing them with mock flows.
7. **Contrast/performance trade-off:** current card blur is broad; terminal correctly uses an opaque coding surface but the glass selectors need explicit exclusions. Existing reduced-transparency support must remain.
8. **Visual acceptance gap:** current checks cover TypeScript/build and service worker policy, not screenshot baselines, keyboard walkthroughs, visual mobile modal checks, or all route/action paths.

## 4. Existing Critical Flows and API Boundaries

| Flow | Source and current behavior | Redesign constraint |
| --- | --- | --- |
| Panel updater | `App.tsx`: version check, branch/build-mode preferences, shared update state, progress overlay, cancel/rollback, verify and hard reload; Changelog is imported from `CHANGELOG.md`. | Preserve backend stages, lock state, cancellation, rollback, other-session notice and full reload. Never invent progress. Keep Changelog synchronized after reload. |
| Nginx SSL | `pages.tsx` `NginxPage`: validates domain names, checks DNS, calls site SSL endpoint, receives Certbot outcome and refreshes site config. New-site builder can create/enable a site and request SSL. | Convert to reviewable stages around the current endpoints; report challenge/issue/config/reload responses from the server. Preserve backup/restore behavior. |
| S3 connection and object operations | `S3StoragePage.tsx`: connection wizard dialog with provider/credentials/test/save; object listing/upload/download/copy/delete, local VPS transfers, mount lifecycle and usage scan call `/api/s3/*` and `/api/files/*`. | Keep one-time secrets private and mount/transfer state real. Do not replace capability-gated actions or usage provenance with mock data. |
| Docker | `DockerPage.tsx`: performs real Docker service/inventory and container/image/volume/network/Compose API operations; UI has no universal deployment wizard today. | Improve existing action confirmations and Compose interaction first. Add a deployment wizard only if an existing API operation can complete every step end-to-end. |
| ARG backup/restore | `MaintenancePage.tsx`: create/upload/download backups; restore confirmation requires exact `RESTORE ARG`; backend makes a protective backup. | Preserve explicit replacement warning, selected backup facts, exact confirmation, busy lock and success/error result. |
| Server file manager | `FileManagerPage.tsx` delegates reusable browsing/actions to `FileManagerBrowser.tsx`; S3 page has provider-specific object and server-transfer views. | Keep file actions and progress states intact; standardize controls while retaining local path vs S3 object semantics. |
| Root terminal | `TerminalPage.tsx` keeps per-tab terminal state, reconnect and root elevation flows; App keeps it mounted behind other routes. | Keep xterm opaque for readability/performance, preserve live session and root warning/ticket authorization, never remount on route switches. |
| Firewall/network/service actions | `pages.tsx` and `AdminOpsPage.tsx` call real allowlisted APIs and report service/rule state. | Preserve protected ports and explicit confirmation for risky service/firewall changes; use semantic status labels, not color alone. |

## 5. Design System Proposal

The latest visual direction is premium minimal, controlled glass, and progressive disclosure. The brand tokens already committed for ARG are copper/teal; the proposed violet sample is a starting point, while ARG's explicit prior brand constraint is to avoid purple. Therefore the redesign keeps the ARG copper accent and uses neutral light/dark glass surfaces rather than introducing a violet identity.

### Tokens

- Semantic tokens: `--background`, `--foreground`, `--surface`, `--surface-elevated`, `--popover`, `--border`, `--input`, `--ring`, `--primary`, `--success`, `--warning`, `--danger`, `--info`, `--muted`, chart series, and focus/selection.
- Shape/space tokens: 4/8 px spacing steps, 12/16/20 px radius tiers, consistent control height, and shared content max-width.
- Depth tokens: surface edge, restrained shadow, `--glass-blur` limited to shell/overlay and selected summary surfaces. Dense tables, code, terminal and charts stay opaque/low blur.
- Complete `data-theme="light|dark"` token sets; default respects system preference on first visit, then persists explicit choice. All status tones must pass AA contrast and include icon/text.
- Use logical properties, `:dir(rtl)` where needed, `font-mono`/`dir="ltr"` for values, and existing Peyda/Abar fonts.

### Shared primitives to evolve from current wrappers

Keep Radix/shadcn primitives and define consistent variants for `GlassCard`, `GlassPanel`, `GlassDialog`, `GlassDrawer`, `PageHeader`, `SectionHeader`, `FormField`, `SearchInput`, `StatusBadge`, `MetricCard`, `Stepper`, `StepNavigation`, `ConfirmationDialog`, `DataTable`, `EmptyState`, `ErrorState`, `LoadingSkeleton`, `FilterBar`, and `ActivityTimeline`. These should be small typed wrappers/compositions, not a second styling framework. Add a Stepper only for multi-step, risky or validation-heavy operations.

## 6. Delivery Phases and Acceptance

Each phase gets a focused commit. Keep server/API code unchanged unless an existing UI integration bug proves a contract change is necessary.

| Phase | Scope | Acceptance |
| --- | --- | --- |
| 1 — Audit & Plan | This document; route/component/API/flow inventory and actual issues. | Every current route and sensitive flow has an owner, current boundary and test/acceptance notes. |
| 2 — Design Foundation | Semantic dual-theme tokens, surface primitives and focus/motion/contrast rules. | Light/dark token smoke checks; existing UI build; reduced-motion/transparency; terminal and dense charts remain readable. |
| 3 — Dashboard Shell | AppShell, topbar/sidebar, theme control, responsive navigation and page header. | All routes remain reachable; admin gates and hash links work; RTL/LTR; desktop/tablet/mobile navigation; no update/terminal lifecycle regression. |
| 4 — Dashboard & Monitoring | Overview cards, server strip, charts, storage and alerts. | Preserve values/provenance; unknown stays unknown; responsive chart labels; no new per-navigation server scan. |
| 5 — Core Management | Docker, Nginx, networking/firewall, services/packages, files, S3, database and apps. | Existing API actions, confirmations, errors, tables, loading and empty states work; no button loses an action. |
| 6 — Step-Based Flows | Actual SSL, S3 connect, safe Docker/Compose tasks when supported, backup/restore and updater. | Back/next/cancel/review validation; backend stage truth; no fake progress/success; secrets remain hidden. |
| 7 — Administration | Terminal, accounts, users, sessions, audit, settings, maintenance and update center. | Terminal remains mounted/opaque; permission and security disclosures are retained; update/restore states survive navigation/reconnect. |
| 8 — Polish & Optimization | Responsive/RTL/LTR/accessibility, motion, empty/error/loading states and blur/render cost. | No horizontal overflow at 320/768/1024/1440 px; keyboard/focus/reduced-motion; no expensive nested blur; check and build pass. |
| 9 — Final Verification | Route/form/action inventory and available visual test coverage. | All routes load; representative and risky actions have verified states; TS/build/tests pass; external visual/manual checks are clearly distinguished from automated checks. |

## 7. Verification Matrix

- Automated on each code phase: `cd frontend && npm run check && npm run build` plus `npm run test:offline` when shell/cache behavior changes; `git diff --check`.
- Design/system checks: token contrast in both themes; reduced-motion and reduced-transparency; keyboard tab/focus through navigation, dialogs and stepper.
- Responsive viewports: 320×740, 390×844, 768×1024, 1024×768 and 1440×900; inspect dialog scroll, table overflow, sidebar and terminal sizing.
- Functional smoke: dashboard/system, Docker action, Nginx SSL form, S3 connect/test/object view, file upload/selection, MySQL access dialog, firewall rules, backup restore confirmation, updater overlay and Root Terminal authorization.
- Existing backend checks are useful regression gates but do not prove browser visual acceptance. If no browser automation/runtime is available, record those screenshots/manual flows as not run rather than marking them passed.

## 8. Current Result

- **Phase 1 — Audit & Plan: complete.** Route, component, API and sensitive-flow inventory recorded above.
- **Phase 2 — Design Foundation: complete.** Added persisted dark/light theme context, semantic light/dark tokens, reusable `GlassPanel`/`GlassDialog`/`GlassDrawer` primitives, card surface variants, and semantic status badge variants. Replaced nested class-inferred blur with explicit glass surfaces and made reduced-transparency surfaces use the active theme. The Topbar theme control is intentionally part of Phase 3.
- **Phase 2 checks:** `npm run check`, `npm run build`, two offline Service Worker tests, and `git diff --check` passed. The build reports the existing large main-chunk advisory (~1.06 MB); no new dependency was added. No visual browser run is claimed.
- **Phase 3 — Dashboard Shell: complete.** Extracted the existing shell into reusable `AppShell`, `SidebarNavigation`, and `TopNavigation`; preserved the existing route access filtering, node workspace selector, update controls, account menu, notification flow, terminal retention and mobile focus behavior. Added the persisted theme switch to the topbar. The desktop sidebar floats with controlled glass and a collapse mode; mobile retains a modal drawer and overlay.
- **Phase 3 checks:** `npm run check`, `npm run build`, two offline Service Worker tests and `git diff --check` passed. Production output was refreshed because this repository tracks `frontend/dist`. The existing ~1.06 MB main-chunk advisory remains. No visual browser run is claimed.
- **Phase 4 — Dashboard & Monitoring: complete.** Added a reusable theme-aware `MetricCard` and applied it to dashboard and system health cards. Metric tone colors and resource chart lines now use active theme tokens. System resource cards and detail rows keep missing values unknown instead of showing misleading `0%` / `0 of 0`; storage inventory, S3, network traffic history, time range controls and per-account dashboard layout remain connected to their actual APIs.
- **Phase 4 checks:** `npm run check`, `npm run build`, two offline Service Worker tests and `git diff --check` passed. The existing main bundle advisory remains (~1.06 MB); no new dependency was added. No visual browser run is claimed.
- **Phase 5 — Core Management Pages: complete.** The shared card surface from Phase 2 now supplies consistent glass surfaces across the existing management pages without changing their API contracts. Replaced native browser confirmation prompts in Docker, server files, S3 connections/objects, APT, systemd services, firewall rules, dashboard fast actions and Nginx site actions with the shared accessible confirmation dialog. Destructive S3 connection/object removal asks for the exact target name/key. Standardized the shared page heading hierarchy and Docker overview metrics while preserving existing controls and operation results.
- **Phase 5 checks:** `npm run check`, `npm run build`, two offline Service Worker tests, `git diff --check`, and a source scan for `window.confirm`, `window.alert`, and `window.prompt` passed. No native alert/confirm/prompt calls remain in `frontend/src`. Production output was refreshed; the existing large main-chunk advisory remains. No visual browser run is claimed.
- **Phase 6 — Unified Step-Based Experience: complete.** Added the reusable `UnifiedStepper` with horizontal/vertical and compact modes, pending/active/completed/error/skipped states, screen-reader status, and real progress semantics. Applied it to the S3 provider/credential flow, the server-reported update stages, backup restore review/confirmation, and Docker Compose action review. SSL issuance for both new and existing Nginx sites now checks DNS, shows per-domain results, requires acknowledgment to proceed when DNS is unresolved, presents the redirect/domain review, and then calls the existing Certbot API. A running SSL request remains an honest in-progress state because the current API returns a final response rather than progress events; no simulated intermediate stages or percentages were added.
- **Phase 6 checks:** `npm run check`, `npm run build`, two offline Service Worker tests and `git diff --check` passed. Existing large main-chunk advisory remains (~1.07 MB). No visual browser run is claimed.
- **Phases 7–9:** still open. No page is described as redesigned until its route and interactive states have been migrated and checked.
