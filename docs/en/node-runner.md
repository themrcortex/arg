# Node.js application runner

The Node.js Runner manages an application process from its **Applications** card. It uses the selected app's existing deployment releases and runtime preference; it does not create a second app or require a separate panel. The runner supports lockfile-based npm, pnpm, and Yarn Classic projects that provide a `start` script.

## Before the first deployment

1. In **Applications**, create or open an application and select **Node.js** as its framework.
2. Set its Node runtime and, if needed, a separate build runtime in **Runtime manager**. The exact selected binaries must already be installed on this VPS.
3. Reserve an unused application port above 1023. ARG expects the application to honor `PORT` and bind to `127.0.0.1`; the process is never intended to listen on a public interface.
4. Deploy a source containing a valid `package.json`, exactly one supported lockfile, and `scripts.start`. A `scripts.build` entry is optional.

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

1. Acquire and validate the Git, ZIP, or approved directory source using the deployment job limits.
2. Create persistent app storage and a private environment file if this is the first release.
3. Inspect `package.json`, resolve the package manager from the lockfile, install dependencies, and run `build` only when that script exists.
4. Execute install/build in a transient systemd unit with a DynamicUser, private temporary directory, read-only system areas, bounded CPU, memory, tasks, output, and a 20-minute limit.
5. Verify the release and atomically switch `current`. The service uses the release through a read-only bind mount.
6. If a Node service is already enabled, restart it and check its loopback listener and `GET /`. On activation/health failure, ARG attempts to restore the previous release and restart the previous service. An app that was stopped stays stopped.

Build output is capped and common credential patterns are redacted. A build can still fail because of network access, a missing native build dependency, incompatible Node engines, or project-specific scripts. The job keeps the failure details needed for diagnosis but does not promise that every arbitrary repository can build on every host.

## Environment variables

The **Runtime variables** form can add or replace validated key/value pairs. It displays existing key names only. Leave a value blank to preserve it; there is no way to read an existing secret back into the browser. The private `.env` file lives in the app's shared deployment directory and is attached to each release. The systemd service reads the file and exposes it read-only in its namespace. The app can write only to `shared/data`; release files and environment are read-only while it runs.

The form changes runtime variables, not build-time configuration. Deploy again when a frontend bundler needs a different build-time value. Save does not print values to ARG audit records or service logs.

## Service controls and reverse proxy

Choose **Activate service** after a successful deployment. ARG creates a fixed per-app systemd unit named from the application UUID. It uses `DynamicUser`, a restricted filesystem, loopback `HOST`, the reserved `PORT`, `Restart=on-failure`, a 30-second graceful-stop window, and fixed resource ceilings. It does not accept an arbitrary systemd unit or shell command from the form.

When the app has a domain, ARG can create a dedicated, ARG-managed Nginx reverse-proxy site. It refuses to overwrite an unrelated site or an existing site whose domain/port no longer matches the generated configuration. It validates and reloads Nginx before reporting success. WebSocket upgrade headers and proxy timeouts are included. SSL issuance and HTTP-to-HTTPS policy remain managed from the Nginx page; the runner does not issue a certificate implicitly. Without a domain, the process can be managed locally but ARG does not create a public route.

Service status is based on systemd and the actual listening socket. Before reporting activation as successful, ARG checks that the reserved port is listening only on loopback, then probes `GET /`. HTTP 2xx–4xx is accepted as an application response; 5xx, an invalid response, a public bind, or a missing listener fails verification and stops a newly enabled service. Use **Execution logs** to inspect recent bounded service output.

## Operations and limits

- **Manage / Refresh:** reads status on demand; the panel does not repeatedly scan package files while the card is closed.
- **Activate / Stop:** enables or stops only this app's ARG-owned unit. A conflicting port or invalid release prevents activation.
- **Execution logs:** reads recent service logs; it does not open a shell.
- **Save runtime variables:** validates names and values and persists them privately. Existing values cannot be retrieved.
- **Deploy / Retry / Roll back:** uses the durable deployment job and release history. Rollback changes code and restarts an enabled service; it does not reverse database or external side effects made by application code.

ARG does not manage Node clusters, arbitrary worker commands, per-route health checks, staging promotion, package-manager auto-installation, or Yarn Berry. A project-specific command is not accepted as a free-form field. Static React/Vite publishing and Next.js/Nuxt SSR are described in the separate static publishing guide.

## Troubleshooting

| Message or symptom | What to check |
| --- | --- |
| `package.json` or `scripts.start` missing | Confirm the deployed source root is the Node app root and define the start script. |
| Lockfile missing, conflicting, or unsupported | Keep one supported lockfile and commit it. Yarn Berry is not yet supported. |
| Runtime or package manager missing | Install the exact supported version from the Runtime manager and refresh inventory. |
| Build exits unsuccessfully | Open the deployment job output; check engine constraints, native build prerequisites, and the project's own script. |
| Port already in use | Select a free reserved port and configure the app to use `PORT`; do not bind it publicly. |
| No loopback listener / HTTP 5xx | Check `scripts.start`, app logs, required environment keys, and whether the app honors `HOST` and `PORT`. |
| Nginx proxy rejected | Review the existing site's `server_name` and configuration. ARG will not replace a manually changed or unrelated site. |
| Service stopped after failed activation | The runner stopped a newly created service after verification failed. Correct the cause and activate again. |

## API and authorization

The runner uses `GET /api/apps/{id}/node`, `GET|PUT /api/apps/{id}/node/service`, `PUT /api/apps/{id}/node/environment`, and `GET /api/apps/{id}/node/service/logs`. Writes require an administrator session, a valid same-origin request, application ownership on the selected server, and an audit event. Root work is passed as structured actions over the protected helper socket; user-provided shell strings are not executed.

Automated tests cover manifest/lockfile selection, environment validation, generated unit isolation, loopback-only verification, HTTP status handling, and deployment rollback paths. Running the service against a disposable Debian/Ubuntu VPS with systemd and Nginx remains an integration test and is not claimed by the repository test suite.
