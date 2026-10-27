# Application registry

The **Applications** page is a registry of projects owned by each ARG installation. It is a separate entity from Nginx configuration files and Docker containers. Linking or adopting those resources belongs to an explicit workflow.

## Open the page

Choose **Applications** under “Applications & services” in the sidebar. The page requires an administrator account. In a single VPS workspace, it reads only that server's registry. In **All servers** mode it aggregates the local registry and nodes whose remote workspaces are ready. Offline or unpaired nodes are shown as unavailable. The aggregate view is read-only; switch to the owning VPS workspace before editing a record.

## Create a record

Choose **Register application** and enter its management metadata:

| Field | Purpose |
| --- | --- |
| Name | Unique on the owning VPS while the record is active. |
| Suggested framework | Laravel, Node.js, Python, WordPress, static, or custom. This selects available runner panels; it does not install a runtime. |
| Source type | Manual, Git repository, archive, Docker, existing directory, or image. Deploy currently accepts public Git, bounded ZIP, and allowlisted existing directories. |
| Domain | Hostname or IP without a scheme or port. ARG checks active application records and readable Nginx configurations for collisions. |
| TCP port | Optional host port. ARG checks other records and currently listening sockets in `/proc/net/tcp*`; it does not bind the port or change the firewall. |
| Desired state | Administrator intent (`running`, `stopped`, or unset). Until a real observer is connected, observed state remains “not observed”. |
| Labels and description | Internal search and notes; up to 20 labels and a 1,000-character description. |

ARG generates a UUID and record version. The record is stored on that server; a second node is not treated as another source of truth. Secrets and environment values are not stored in the registry.

## Search and filtering

Search matches names, domains, descriptions, and labels. The framework filter narrows by template. **Show archived** includes soft-deleted records. If a server cannot be queried in the aggregate view, an unavailable card explains the issue and links to that server's workspace.

## Edit, archive, and restore

Edits use an optimistic version. If another administrator has changed the record, the stale edit is rejected; refresh the list and apply the change again. **Archive** only removes the ARG registry record from the active list. It does not stop or remove Nginx, files, containers, volumes, or system services. Restore reactivates the record after rechecking name, domain, and port conflicts.

## Read the status fields

- **Registered** means the ARG metadata record exists.
- **Desired state** is administrator intent, not a systemd or Docker observation.
- **Observed state: not observed** means no trusted runtime adapter is connected. ARG does not invent CPU/RAM usage or a Running badge.
- **Live resources not connected** means no process or container has been associated with the record.

## Deployment status

Durable source jobs are available from a local application card. They record state, steps, actor, idempotency key, and server target; they support cancellation, retry, SSE, and release rollback. They stage files and atomically move a `current` pointer. See [Source deployments & durable jobs](./deployment-jobs.md) for limits and operations. Application records can save requested PHP/Node/Python runtime versions and a separate Node build version from the installed inventory; see [Runtime manager](./runtime-manager.md). Laravel apps have a runner for private environment configuration, allowlisted Artisan commands, web/queue services, and database provisioning; see [Laravel management](./laravel-runner.md). Node apps have lockfile-driven builds, isolated systemd services, private runtime environment variables, loopback health checks, and a guarded ARG-managed Nginx reverse proxy; see [Node.js application runner](./node-runner.md). These implemented runners do not make every registry framework executable: Python, WordPress, arbitrary custom apps, static/SSR build templates, and broader staging workflows remain separate phases. The registry continues to report observed service state rather than claiming a site is running from metadata alone.

## API and schema

- `GET /api/apps?include_deleted=true` lists the registry.
- `POST /api/apps` creates a record.
- `GET /api/apps/{id}` reads a record.
- `PUT /api/apps/{id}` performs an optimistic update with `expected_version`.
- `DELETE /api/apps/{id}` soft-deletes; `POST /api/apps/{id}/restore` restores.
- `POST /api/apps/{id}/deploy` enqueues Git/directory source; `/deploy/archive` accepts a bounded ZIP.
- `GET /api/apps/{id}/deploy/jobs` reads persisted job history.
- `GET /api/runtimes` returns the administrator-only installed runtime and tool inventory.
- `GET /api/apps/{id}/laravel` reads Laravel readiness and environment key names without values.
- `PUT /api/apps/{id}/laravel/environment` stores environment variables in a private VPS file; `POST` on the same path generates an empty APP_KEY once.
- `GET /api/apps/{id}/laravel/operations` reads operation history and sanitized output; `POST` queues a Composer install or allowlisted Artisan action.

The additive, idempotent migration creates `applications` and `application_services`, and adds `runtime_version` and `build_runtime_version` to older application tables. Both version fields default to `system`. Records are scoped by `owner_server_uuid`; `observed_state` defaults to `unknown`, `reconcile_policy` to `manual`, and concurrent edits are guarded by `version`.
