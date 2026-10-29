# Source Deployments and Durable Jobs

This page describes ARG's base deployment workflow. ARG acquires source into an isolated release, validates its contents, and atomically switches the `current` pointer. Job state and step history live in SQLite, so closing the browser does not discard progress.

## Current scope

Open an application card in **Applications** and choose **Deploy source**. Four source types are available:

1. **Public HTTPS Git:** fetch a GitHub, GitLab, Bitbucket, or Codeberg repository at a branch, tag, or commit. URLs containing usernames, passwords, or tokens are rejected.
2. **Private SSH Git:** fetch a private GitHub repository with a Deploy Key scoped to that repository. See [SSH keys and Deploy Keys](./ssh-deploy-keys.md) for key generation, GitHub setup, and limitations.
3. **Existing VPS directory:** the canonical path must be beneath `/var/www` or `/opt/arg/apps`. Symlinks anywhere in the source tree are rejected.
4. **ZIP archive:** uploads are limited to 256 MiB, expanded data to 2 GiB, and entries to 20,000.

ARG stores releases under `/var/lib/arg/deployments/apps/<application-id>/releases/<job-id>`. `current` is a relative symlink switched with an atomic rename only after release validation. Previous releases remain on disk and a successful job can roll back to the previous release.

To bound disk usage, ARG retains the five newest releases plus the active and previous release. Older managed release directories are removed while their database history remains marked `expired`. An incomplete ZIP upload temp file older than one hour is removed at worker startup; failed-job ZIPs remain available for retry for up to seven days.

## States and progress

Jobs move through `queued`, `validating`, `running`, `verifying`, and then `succeeded`, `failed`, or `cancelled`. Every step records its message and start/end timestamps in `deployment_job_steps`. An SSE event endpoint is available; the current page polls persisted history. Refreshing the page does not lose progress.

Only one job per application may be active, and one worker runs jobs per VPS. The worker writes a lease and heartbeat. On ARG restart, interrupted work is queued again. Repeated requests with the same idempotency key return the same job; retry uses a new key.

## Custom pre-activation commands

The **Deploy source** form accepts optional validation or preparation commands before it creates a job. Each non-empty line is one command. Commands run sequentially after source acquisition and automatic install/build, but before ARG switches the `current` pointer. If any command fails, the new release is not activated and the existing release remains current.

Input limits are eight commands, 512 characters per line, 4 KiB total, and 32 tokens per command including the executable. Single or double quotes can keep whitespace inside one argument. This is **not a shell**: variables, `&&`, pipes, redirects, substitutions, and `;` are not interpreted. ARG starts only the executable with its argument list. For example, if the project defines a `verify` package script: `npm run verify -- --strict`. ARG does not interpret shell operators; however, the selected executable can run its own logic. For example, `npm run` runs the package script declared in `package.json`, still inside the restricted sandbox.

Commands run in a restricted sandbox with write access to staging and the build cache. They do not inherit ARG's shell environment, and execution remains resource- and time-bounded. Confirm that commands will run on the VPS before starting the job. External side effects—such as database changes, API calls, or sent messages—are not undone by release rollback. The command text is not returned by the Job details API; inspect each step's output and avoid printing secrets.

## File safety

- Large ZIPs, suspicious compression ratios, traversal paths such as `../../etc/passwd`, symlinks, and special device files are rejected.
- Directory copying rejects symlinks and special files. `.env*`, `.git`, and `node_modules` are excluded.
- Releases are capped at 2 GiB and free space is checked before copying or extraction.
- Git is invoked with fixed arguments, and fetching a repository does not run its hooks. Pre-activation commands run only when you enter and confirm them in the form; build scripts are limited to the selected script in a supported runner. Fetch has a 180-second timeout.
- Public HTTPS URLs cannot contain credentials. Private SSH keys are Ed25519 files under `/var/lib/arg/ssh-keys` with restricted permissions; Git uses batch mode and strict host-key checking.
- Register a Deploy Key in GitHub for the specific repository and preferably keep it read-only. ARG does not have GitHub API access to add the key automatically.

## Cancel, retry, and rollback

- **Cancel:** queued work becomes cancelled immediately. For work in progress, cancellation is recorded and the worker stops at a safe step boundary; Git is bounded by a timeout.
- **Retry:** failed Git and directory jobs can be queued again with the same source and a new key. ZIP retry requires its temporary upload to still exist; otherwise upload it again.
- **Rollback:** only a previous release belonging to the same application can be selected. ARG validates the release path and atomically switches `current`; files are not deleted.
- **Failure before activation:** the current release is unchanged. A job reports success only after the pointer switch and result are recorded.

## Runtime integrations added after the base job engine

The optional pre-activation commands above are bounded and do not create a general-purpose shell. A persistent framework runtime is created only when the application has an implemented runner:

- **Laravel:** see [Laravel management](./laravel-runner.md) for Composer lockfile installs, private environment, approved Artisan operations, Nginx/PHP-FPM, and optional services.
- **Node.js:** see [Node.js application runner](./node-runner.md) for supported lockfiles, bounded dependency/build steps, runtime environment, a per-app systemd service, loopback verification, guarded Nginx proxy creation, and release recovery.
- **Other frameworks:** Python and WordPress do not have a managed runtime runner in this release. Static React/Vue/Vite publishing and Next.js/Nuxt SSR are covered in [Static and SSR publishing](./static-and-ssr.md). The pre-activation commands above do not create a permanent runtime or add framework support.

The optional `healthcheck_file` is a regular-file check inside the release. Laravel and Node runners add their own framework-specific activation checks; a file-existence check is not a substitute for those runtime checks. Database migrations or other external side effects produced by application code are not reversed by code rollback.

## Troubleshoot job details

If Job details returns a database error, keep the Job ID and exact error text, then inspect the ARG service log. Do not edit the SQLite table manually. Check the authoritative Job list and worker state before retrying so a still-running job is not duplicated. `Invalid column type Integer ... finished_at` was a decoder type mismatch for SQLite's integer timestamp; the `development` branch now reads the value as an integer. Do not convert the stored timestamp to text.

For a failed build or pre-activation command, open that Job's details, locate the failed step, and load its output page. Logs persist across refresh. Do not print secrets from a command or its output. If the panel is unavailable, use SSH:

```bash
sudo arg status
sudo journalctl -u arg -n 200 --no-pager
```

A failed ZIP can only be retried while its temporary upload still exists; upload it again if it has been removed. Rollback switches code releases only; it cannot undo database changes or external service effects.

## API

- `POST /api/apps/{id}/deploy` — Git or directory source; JSON fields: `source`, `idempotency_key`, optional `healthcheck_file`.
- `POST /api/apps/{id}/deploy/archive` — raw ZIP body with `Content-Type: application/zip` and `Idempotency-Key` header.
- `GET /api/apps/{id}/deploy/jobs` — the latest 50 application jobs.
- `GET /api/deploy/jobs/{id}` — job and step history.
- `GET /api/deploy/jobs/{id}/events` — Server-Sent Events; administrator session is rechecked during the stream.
- `GET /api/deploy/jobs/{id}/logs?tail=true&limit=500` — latest output page; `after=<id>` fetches newer events and `before=<id>` fetches older pages. Each response contains up to 500 rows (100 by default; it may be empty), and the latest 2,000 events per job are retained.
- `POST /api/deploy/jobs/{id}/cancel` and `POST /api/deploy/jobs/{id}/retry`.
- `POST /api/apps/{id}/deploy/rollback` — body: `{"release_id":"…"}`.
- Node Runner: `GET /api/apps/{id}/node`, `GET|PUT /api/apps/{id}/node/service`, `PUT /api/apps/{id}/node/environment`, and `GET /api/apps/{id}/node/service/logs`.

All writes require an administrator session, valid Origin, and an audit event. The destination path is derived from the application's UUID and is never accepted from the request.
