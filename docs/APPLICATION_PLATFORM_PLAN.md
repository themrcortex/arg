# ARG Application Platform Work Plan

This plan covers the application platform request and its follow-up requirements: application-to-application networking, database visibility and management, container connectivity, mesh topology, editable workflows, safe launch defaults, release naming and retention, and separate staging launch configuration.

## Verified starting point

- Local branch: `development`
- Starting commit: `9a54b491db0aa720f0cdf6b0b9cca1851a1b61c0`
- Existing deployment already launches the supplied Node.js test project after a start script is configured.
- ARG has persistent deployment Jobs and release directories, systemd-managed Node SSR, per-app Laravel/MySQL provisioning, Docker application networks, Docker ownership labels, app workspaces/environments, and a graph of server-to-server links.
- Existing workflow records are validated DAG drafts only. They are not executed.
- `docker-bridge` is an actual internal Docker network; `host-local` is only a port mapping. There is no cross-Node app overlay, PostgreSQL management API, or workflow runner in the starting revision.

## Requirements and phases

| Phase | Tasks | Acceptance evidence | Status |
|---|---|---|---|
| 1. Workflow designer | Persisted custom DAG editor; edit/create/delete; labels; dependency selection; cycle and bounds validation; execution status kept explicit. | Rust validation tests, API routes, UI typecheck/build, route tests. | Implemented; executor remains a separately scoped capability |
| 2. Local application mesh | Global topology page; live Docker network inspection; safe app-owned container attachment; detach/delete; host-local marked metadata-only. | Helper ownership parser test, admin-scoped mesh API, UI build. | Implemented |
| 3. Database topology | Show sanitized Laravel DB endpoint metadata; omit passwords/URLs; link to the app's existing MySQL management; connection health stays unverified. | Secret-safe summary function and UI build. | Implemented |
| 4. Launch defaults and release identity | Per-app saved source and pre-deploy commands, health path, retention; Node runtime script settings; automatic or custom launch; monotonic version number; optional release name. | Additive migration, default-source validation, monotonic version test, launch route refresh test. | Implemented |
| 5. Production/Staging launch targets | Create a separate staging application ID and environment; select same-project staging context; set distinct domain and loopback port; never copy secrets or active release. | Database ownership checks, domain/port preflight, separate deployment history. | Implemented; stage still requires its own first source deployment |
| 6. Deployment detail page and app cards | Persistent full-page launch route; automatic/custom source selection; Node build/start settings; pre-deploy commands; health path; environment; version and retention; persistent job logs; compact runtime/environment cards with a direct domain link. | Route refresh test, frontend typecheck/build, Rust tests. | Implemented for existing deployment adapters |
| 7. Database and container integration | Use existing protected MySQL tools and ARG-owned Docker networks; do not expose fake PostgreSQL controls. | Ownership checks, redacted DB metadata, Docker helper tests. | Implemented within current provider support |
| 8. Cross-Node app mesh | Negotiate explicit Node capabilities and encrypted transport before showing remote links as active; otherwise maintain a clearly labeled design/backlog. | Two isolated Node integration environment required. | Blocked on available paired test Nodes |
| 9. WordPress version selection | Fetch stable core versions from the WordPress API; validate and persist a selected semver; use the exact tag from the official WordPress GitHub mirror as the first deployment source; improve template cards with platform icons/brand accents. | Version parser and source-selection tests, legacy migration, frontend check/build. | Implemented; complete automated DB/wp-config/Nginx/SSL provisioning remains separate |
| 10. Final quality gate and release | Format, Rust check/tests, frontend check/offline tests/build, docs review, separate commits, publish development release only after passing gates. | Rust 300/300 tests; format/check pass; frontend typecheck, 18/18 tests and production build pass. GitHub release job 41407482986 succeeded. | Released as v0.7.9-dev.4 |

## Prompt-by-prompt task inventory

### Application network and database prompt

- Provide a page that shows the applications and their actual local network relations.
- Create private Docker bridge networks and connect only containers with a matching ARG application ownership label.
- Read live membership from Docker; do not present saved `host-local` rows as an isolated network.
- Detach an app and remove an empty ARG-owned network.
- Show safe database connection metadata for supported apps and provide a route to the existing MySQL manager.
- Keep PostgreSQL, DB reachability, and cross-Node overlays unavailable unless their real adapter and enforcement exist.

### Workflow prompt

- Create blank and preset workflows; edit/delete saved graphs.
- Edit step labels/types, move steps up/down, and edit dependencies; reject unsupported steps, cycles, duplicate IDs, and oversized graphs.
- Persist the graph across refresh and show clearly that the current version stores a design only. Triggers and steps do not run yet.

### Default and custom launch prompt

- Open launch settings as a dedicated hash route, so refresh stays in that page.
- Choose the saved automatic configuration or override source settings for one deployment.
- Save reusable source, branch/path, Node build/start script names, pre-deploy argv commands, health checks, and release retention per app.
- Use an incrementing release version number (`v1`, `v2`, …) and allow a separate optional human release name.
- Preserve Production/Staging identity in each job. A Staging app has a separate service, release directory, context, and domain/port; secrets and active code are never copied.

### Application card usability prompt

- Give each application card a clear platform accent, status and environment badges, and compact runtime/source details.
- Make a registered domain a keyboard-accessible link that opens the site in a separate tab; HTTP is used as the entry point so sites without TLS can still be reached and configured HTTPS sites can redirect normally.

## Current implementation boundaries

- The workflow feature is an editor/validator. Durable workflow job execution, timers, webhook triggers and retry policies are not implemented.
- The local mesh is enforced only for ARG-owned Docker containers on a single VPS. There is no cross-Node overlay/tunnel or network encryption negotiation.
- Database topology is read-only metadata. MySQL administration is linked to the existing Laravel/MySQL functions; no generic SQL console or PostgreSQL manager was added.
- Staging creation makes an isolated application record. It does not clone a repository, `.env`, database, uploaded data, or production release. The first staging deployment must choose its own source and secrets.
- A staging hostname is suggested when the source uses a hostname. DNS records are not created automatically; the operator must point DNS to this VPS. A Node runtime still binds to loopback, so an IP and loopback port is not a public endpoint; use a domain through the supported Nginx path or keep it private on the VPS.
- SSL, domain and process behavior is provided by the application's existing Nginx/Node deployment adapters; no claim of zero downtime is made for other runtimes.

## Safety and compatibility rules

- The database topology view exposes only the Laravel connection driver, host, port, database, username and a password-present boolean. It never returns `DB_PASSWORD` or a connection URL.
- Docker membership is authoritative only when `docker network inspect` succeeds and ARG ownership labels match. A DB membership record alone is not presented as a live connection.
- Staging is a separate application identity with separate release path, process and domain. Runtime secrets and persistent files are not copied from Production.
- Existing installations receive additive SQLite migrations with defaults that preserve current behavior.
- Workflows remain designs until a durable, permission-checked worker exists. The UI must not say “run”, “success” or “scheduled” while only storing a graph.
- PostgreSQL is inventory-only until an actual management adapter and tests are implemented.

## WordPress install version and current limit

The WordPress template and application editor now let the operator select any stable three-part version returned by the official WordPress version API (or enter a pinned numeric version). The version is stored on the application, copied to its separate Staging record, and used as the default deployment reference for `https://github.com/WordPress/WordPress.git`. Deployment therefore fetches a fixed version tag instead of `main` or `latest`. An existing saved custom source still takes precedence until changed.

This does not yet provision a complete WordPress website: the current deployment engine does not create an isolated MySQL database/user, generate `wp-config.php`, bind the WordPress public directory into an ARG-managed Nginx site, or issue SSL as one atomic wizard. The selected version controls the WordPress source release; database, web server and certificate setup still require the existing supported/manual flows. The template UI states this clearly and does not report a site as installed before those operations are actually complete.

## Final quality gate (2026-11-02)

- `cargo fmt --all -- --check`: passed.
- `CARGO_TARGET_DIR=/tmp/arg-audit-target CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo check --locked`: passed.
- Same isolated target with `cargo test --locked`: 300 passed, 0 failed.
- In a clean temporary frontend project copy with `npm ci`, `npm run check`: passed; `npm run test:offline`: 18 passed; `npm run build`: passed. Vite reports the existing 509 kB main bundle advisory.
- Deployment against an actual privileged helper, Docker daemon, database server, production domain or paired Node was not run here. Cross-Node networking and end-to-end WordPress database/wp-config/Nginx/SSL provisioning remain unsupported as stated above.

## Published development release

- Version/tag: `v0.7.9-dev.4`.
- Feature commit: `73e4f92`; version and release commit: `65a824c`.
- GitHub Actions release run `41407482986` succeeded for x86_64 and aarch64 musl builds and published both binaries with SHA-256 sidecars.
- Release page: https://github.com/themrcortex/arg/releases/tag/v0.7.9-dev.4
