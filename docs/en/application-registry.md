# Application registry

The **Applications** page is a registry of projects owned by each ARG installation. It is a separate entity from Nginx configuration files and Docker containers. Linking or adopting those resources belongs to an explicit workflow.

## Open the page

Choose **Applications** under “Applications & services” in the sidebar. The page requires an administrator account. In a single VPS workspace, it reads only that server's registry. In **All servers** mode it aggregates the local registry and nodes whose remote workspaces are ready. Offline or unpaired nodes are shown as unavailable. The aggregate view is read-only; switch to the owning VPS workspace before editing a record.

## Create a record

Choose **Register application** and enter its management metadata:

| Field | Purpose |
| --- | --- |
| Name | Unique on the owning VPS while the record is active. |
| Suggested framework | Laravel, Node.js, Python, WordPress, static, or custom. This is metadata and does not install a runtime. |
| Source type | Manual, Git repository, archive, Docker, existing directory, or image. Selecting one does not fetch or execute it in this phase. |
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

## Later capabilities

Deployment, builds, runtime installation, secret storage, environments, storage plans, health checks, and automatic reconciliation are disabled in this release. They must not be treated as operational controls. Later phases will add explicit ownership links to Nginx/Docker and persistent jobs with conflict checks and observed results.

## API and schema

- `GET /api/apps?include_deleted=true` lists the registry.
- `POST /api/apps` creates a record.
- `GET /api/apps/{id}` reads a record.
- `PUT /api/apps/{id}` performs an optimistic update with `expected_version`.
- `DELETE /api/apps/{id}` soft-deletes; `POST /api/apps/{id}/restore` restores.

The additive, idempotent migration creates `applications` and `application_services`. Records are scoped by `owner_server_uuid`; `observed_state` defaults to `unknown`, `reconcile_policy` to `manual`, and concurrent edits are guarded by `version`.
