# Source Deployments and Durable Jobs

This page describes ARG's base deployment phase. ARG acquires source into an isolated release, validates its contents, and atomically switches the `current` pointer. Job state and step history live in SQLite, so closing the browser does not discard progress.

## Current scope

Open an application card in **Applications** and choose **Deploy source**. Three source types are available:

1. **Public HTTPS Git:** fetch a GitHub, GitLab, Bitbucket, or Codeberg repository at a branch, tag, or commit. URLs containing usernames, passwords, or tokens are rejected.
2. **Existing VPS directory:** the canonical path must be beneath `/var/www` or `/opt/arg/apps`. Symlinks anywhere in the source tree are rejected.
3. **ZIP archive:** uploads are limited to 256 MiB, expanded data to 2 GiB, and entries to 20,000.

ARG stores releases under `/var/lib/arg/deployments/apps/<application-id>/releases/<job-id>`. `current` is a relative symlink switched with an atomic rename only after release validation. Previous releases remain on disk and a successful job can roll back to the previous release.

To bound disk usage, ARG retains the five newest releases plus the active and previous release. Older managed release directories are removed while their database history remains marked `expired`. An incomplete ZIP upload temp file older than one hour is removed at worker startup; failed-job ZIPs remain available for retry for up to seven days.

## States and progress

Jobs move through `queued`, `validating`, `running`, `verifying`, and then `succeeded`, `failed`, or `cancelled`. Every step records its message and start/end timestamps in `deployment_job_steps`. An SSE event endpoint is available; the current page polls persisted history. Refreshing the page does not lose progress.

Only one job per application may be active, and one worker runs jobs per VPS. The worker writes a lease and heartbeat. On ARG restart, interrupted work is queued again. Repeated requests with the same idempotency key return the same job; retry uses a new key.

## File safety

- Large ZIPs, suspicious compression ratios, traversal paths such as `../../etc/passwd`, symlinks, and special device files are rejected.
- Directory copying rejects symlinks and special files. `.env*`, `.git`, and `node_modules` are excluded.
- Releases are capped at 2 GiB and free space is checked before copying or extraction.
- Git is invoked with fixed arguments, without a shell. Project hooks and scripts are never executed; fetch has a 180-second timeout.
- The installer limits `/var/lib/arg` to the ARG service account. Private Git credentials in a URL are not accepted.

## Cancel, retry, and rollback

- **Cancel:** queued work becomes cancelled immediately. For work in progress, cancellation is recorded and the worker stops at a safe step boundary; Git is bounded by a timeout.
- **Retry:** failed Git and directory jobs can be queued again with the same source and a new key. ZIP retry requires its temporary upload to still exist; otherwise upload it again.
- **Rollback:** only a previous release belonging to the same application can be selected. ARG validates the release path and atomically switches `current`; files are not deleted.
- **Failure before activation:** the current release is unchanged. A job reports success only after the pointer switch and result are recorded.

## Explicit limitations

This phase packages source and prepares/activates a release, but it **does not run framework builds, start Node/Python/PHP, install dependencies, create or edit Nginx configuration, restart services, perform HTTP health checks, or automatically publish the website**. The optional health-check path only verifies that a regular file exists inside the release. Those features depend on later runtime, environment, and website integration phases; application runtime remains unknown until an actual observer is implemented.

## API

- `POST /api/apps/{id}/deploy` — Git or directory source; JSON fields: `source`, `idempotency_key`, optional `healthcheck_file`.
- `POST /api/apps/{id}/deploy/archive` — raw ZIP body with `Content-Type: application/zip` and `Idempotency-Key` header.
- `GET /api/apps/{id}/deploy/jobs` — the latest 50 application jobs.
- `GET /api/deploy/jobs/{id}` — job and step history.
- `GET /api/deploy/jobs/{id}/events` — Server-Sent Events; administrator session is rechecked during the stream.
- `POST /api/deploy/jobs/{id}/cancel` and `POST /api/deploy/jobs/{id}/retry`.
- `POST /api/apps/{id}/deploy/rollback` — body: `{"release_id":"…"}`.

All writes require an administrator session, valid Origin, and an audit event. The destination path is derived from the application's UUID and is never accepted from the request.
