# Node.js application runner

The Node.js Runner manages an application process from its **Applications** card. It uses the selected app's existing deployment releases and runtime preference; it does not create a second app or require a separate panel. The runner supports lockfile-based npm, pnpm, and Yarn Classic projects with a usable package script or a supported Next.js/Nuxt entry point.

## Before the first deployment

1. In **Applications**, create or open an application and select **Node.js** as its framework.
2. Set its Node runtime and, if needed, a separate build runtime in **Runtime manager**. The exact selected binaries must already be installed on this VPS.
3. Reserve an unused application port above 1023. ARG expects the application to honor `PORT` and bind to `127.0.0.1`; the process is never intended to listen on a public interface.
4. Deploy a source containing a valid `package.json` and exactly one supported lockfile. By default ARG runs `scripts.start` when present, and runs `scripts.build` when present. You can choose another script name from the manifest in **Runtime and tools**.

For example, a minimal manifest might contain `"scripts": {"build":"tsc -p .", "start":"node dist/server.js"}`. This is only an example: ARG reads the project manifest and runs the project's own script inside the bounded build/service environment. It does not infer that every Node project is a web server.

## Project inspection and package managers

Open **Manage** on the application card to load the runner status. Inspection reads the active release and reports the detected Node version, `engines.node`, package manager, manifest scripts, dependency readiness, and environment variable **names**. Environment values are never returned.

The selected lockfile determines the package manager:

| Lockfile | Command |
| --- | --- |
| `package-lock.json` | `npm ci --no-audit --no-fund` |
| `pnpm-lock.yaml` | `pnpm install --frozen-lockfile` |
| `yarn.lock` with Yarn Classic | `yarn install --frozen-lockfile` |

If more than one supported lockfile exists, or `packageManager` requests an unsupported manager (including Yarn Berry), inspection fails with a specific error. ARG does not silently choose a different package manager or regenerate the lockfile. The selected manager must be installed from the host's configured package sources. Install lifecycle scripts run only as part of the isolated build because native packages often require them; the build does not have access to ARG's database, TLS keys, or system configuration.

## Deployment behavior

For a Node application, a deployment performs these steps:

1. Acquire and validate the Git, ZIP, or approved directory source using the deployment job limits. If every ZIP file is inside the same top-level project folder, ARG removes that one wrapper folder so `package.json` lands at the release root. Mixed-root archives keep their original structure.
2. Create persistent app storage and a private environment file if this is the first release.
3. Inspect `package.json`, resolve the package manager from the lockfile, install dependencies, and run the configured Build script. `auto` selects `build` when available; `none` skips it for SSR apps. Static apps need a Build script.
4. Execute install/build in a transient systemd unit with a DynamicUser, private temporary directory, read-only system areas, bounded CPU, memory, tasks, output, and a 20-minute limit.
5. Verify the release and atomically switch `current`. The service uses the release through a read-only bind mount.
6. If a Node service is already enabled, restart it and check its loopback listener and configured HTTP health path (default `/`). A 2xx response is required. On activation/health failure, ARG attempts to restore the previous release and restart the previous service. An app that was stopped stays stopped.

Build output is capped and common credential patterns are redacted. Job stage changes and bounded command output are stored in SQLite; the panel refreshes status while a job is active, and retained logs remain available after a page reload. Logs keep the latest 2,000 records per job and can be paged in the UI. Output is recorded after each package command exits; while a command is still running, the live indicator shows its current stage rather than a byte-level stream. A build can still fail because of network access, a missing native build dependency, incompatible Node engines, or project-specific scripts. The job keeps the failure details needed for diagnosis but does not promise that every arbitrary repository can build on every host.

The **Build script**, **Runtime script**, and **Health Check path** settings live with the Node Runner. Script fields accept only names from `package.json`, not shell commands. Health Check accepts a local URL path and is probed from the VPS over loopback; use a route such as `/health` or `/ready` that returns 2xx.

## Environment variables

The **Runtime variables** form can add or replace validated key/value pairs. It displays existing key names only. Leave a value blank to preserve it; there is no way to read an existing secret back into the browser. The private `.env` file lives in the app's shared deployment directory and is attached to each release. The systemd service reads the file and exposes it read-only in its namespace. The app can write only to `shared/data`; release files and environment are read-only while it runs.

The form changes runtime variables, not build-time configuration. Deploy again when a frontend bundler needs a different build-time value. Save does not print values to ARG audit records or service logs.

## Service controls and reverse proxy

After a successful deployment, choose **Set up complete website**. ARG resolves DNS, enables the app's fixed systemd unit and checks its health endpoint, creates/enables its Nginx site, then requests an SSL certificate with Certbot and redirects HTTP to HTTPS when SSL is not configured for this hostname. Enter a renewal email only when a new certificate is needed. The dialog shows the DNS addresses it resolved; make sure they point to this VPS and that inbound port 80 is reachable.

If another Nginx file already claims the hostname, moving it to this app requires a separate confirmation. ARG only proceeds when it finds exactly one simple regular file under `sites-available` that it can safely parse. It writes a root-only backup of the entire file to `/var/backups/arg/nginx-domain-takeovers`, removes only the selected hostname from the old `server_name` directives, and preserves other aliases. Wildcard claims, multiple conflicting files, `conf.d`, the default site, and sites owned by another ARG app are not changed automatically. If setup fails before the new Nginx site is complete, ARG attempts to restore the old configuration and reports a restore failure and backup location.

ARG creates a fixed per-app systemd unit named from the application UUID. It uses `DynamicUser`, a restricted filesystem, loopback `HOST`, the reserved `PORT`, `Restart=on-failure`, a 30-second graceful-stop window, and fixed resource ceilings. It does not accept an arbitrary systemd unit or shell command from the form.

If the app has no domain, its service can be managed locally but ARG does not create a public route. Nginx configuration is validated with `nginx -t` and reloaded only after validation succeeds. The proxy includes WebSocket upgrade headers and proxy timeouts. SSL setup is part of the app's complete website flow.

Service status is based on systemd and the actual listening socket. Before reporting activation as successful, ARG checks that the reserved port is listening only on loopback, then probes the configured health path. Any status outside 2xx, an invalid response, a public bind, or a missing listener fails verification and stops a newly enabled service. Use **Execution logs** to inspect recent bounded service output.

## Operations and limits

- **Manage / Refresh:** reads status on demand; the panel does not repeatedly scan package files while the card is closed.
- **Activate / Stop:** enables or stops only this app's ARG-owned unit. A conflicting port or invalid release prevents activation.
- **Execution logs:** reads recent service logs; it does not open a shell.
- **Save runtime variables:** validates names and values and persists them privately. Existing values cannot be retrieved.
- **Deploy / Retry / Roll back:** uses the durable deployment job and release history. Rollback changes code and restarts an enabled service; it does not reverse database or external side effects made by application code.
- **Custom pre-activation commands:** enter one command per line in the Deploy form. ARG runs them after source acquisition and automatic dependency/build steps, but before activating the release. It passes each command as an executable plus arguments; it does not interpret shell syntax, pipes, redirects, or expansions. The runner can write only to the staging release and build cache inside its CPU/RAM/time-limited systemd sandbox. Production `.env` values and service secrets are not passed; do not put passwords or tokens in command arguments. Live output is retained in that Job's details, and a non-zero exit stops activation. External effects such as database changes are not reversed by a file rollback.
- **Runtime settings:** select manifest script names and a loopback HTTP health path. Saving settings does not execute free-form commands; use **Apply service settings** to safely regenerate and verify an already-running systemd unit.

ARG does not manage Node clusters, staging promotion, package-manager auto-installation, or Yarn Berry, and it does not interpret free-form shell scripts. Build and runtime options are selected from package script names; pre-deploy commands are executed only as an executable and its argument list. Static React/Vite publishing and Next.js/Nuxt SSR are described in the separate static publishing guide.

## Troubleshooting

| Message or symptom | What to check |
| --- | --- |
| `package.json` or a start entry point missing | Confirm the ZIP has one project root folder or a consistent project root. ARG strips one common wrapper folder, but mixed-root archives are left unchanged. Define a start script or use a supported Next/Nuxt entry point. |
| Lockfile missing, conflicting, or unsupported | Keep one supported lockfile and commit it. Yarn Berry is not yet supported. |
| Runtime or package manager missing | Install the exact supported version from the Runtime manager and refresh inventory. |
| Build exits unsuccessfully | Open the deployment job output; check engine constraints, native build prerequisites, and the project's own script. |
| Port already in use | Select a free reserved port and configure the app to use `PORT`; do not bind it publicly. |
| No loopback listener / health check is not 2xx | Check the selected runtime script, health path, app logs, required environment keys, and whether the app honors `HOST` and `PORT`. |
| A Job stays queued or shows a database error | Open **View process details** on the Job card. The dialog shows the original SQLite cause and worker log; transient claim errors are retried and retained in the Job report. Real Git and Node install/build output is streamed and stored with bounded pagination. |
| Nginx proxy rejected | Review the existing site's `server_name` and configuration. ARG will not replace a manually changed or unrelated site. |
| Domain is already used by another site | In the complete website flow, explicitly confirm moving the hostname after reviewing the warning. Only one simple `sites-available` file is eligible for automatic takeover; inspect other cases from the Nginx page. |
| Certbot or HTTP-01 failed | Compare the displayed DNS addresses with the VPS address, expose inbound port 80, and check CDN/proxy settings. The dialog shows the error; Nginx may remain available over HTTP. |
| Service stopped after failed activation | The runner stopped a newly created service after verification failed. Correct the cause and activate again. |

## API and authorization

The runner uses `GET /api/apps/{id}/node`, `GET|PUT /api/apps/{id}/node/service`, `GET|PUT /api/apps/{id}/node/runtime-settings`, `PUT /api/apps/{id}/node/environment`, and `GET /api/apps/{id}/node/service/logs`. Deployment logs use `GET /api/deploy/jobs/{id}/logs?tail=true&limit=500` for the latest page, `?after={id}` for new events, or `?before={id}` for earlier pages. Writes require an administrator session, a valid same-origin request, application ownership on the selected server, and an audit event. Root work is passed as structured actions over the protected helper socket; user-provided shell strings are not executed.

Automated tests cover manifest/lockfile selection, environment validation, generated unit isolation, loopback-only verification, HTTP status handling, and deployment rollback paths. Running the service against a disposable Debian/Ubuntu VPS with systemd and Nginx remains an integration test and is not claimed by the repository test suite.
