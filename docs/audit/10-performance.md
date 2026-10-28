# 10 — Performance, concurrency and resource use

## Scope and evidence

- Source baseline: `development` at `eec273cde037fa57edc1b5ab042fda7acc26a32c` (`0.7.3-dev.1`).
- Audit work is isolated on `audit/development-hardening`; this phase's implementation is commit `e4d128ea3eb467f38edafde1d320f7cd7d68a925`.
- Reviewed the shared SQLite state and connection setup, deployment queue/worker and job-output SSE, metrics sampling and retention controls, frontend polling reviewed in [09-frontend.md](09-frontend.md), and the bounded file/S3 operations described in [03-privileged-helper.md](03-privileged-helper.md), [05-deployment.md](05-deployment.md), and [06-files-and-s3.md](06-files-and-s3.md).
- No production host, user database, S3 provider, remote Node, or production workload was accessed.

## Findings

### PERF-001 — Deployment SSE could miss new output after the retained log window filled

- **ID:** PERF-001
- **Title:** The SSE revision stopped changing when a deployment had reached the 2,000-log retention cap.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED; fixed
- **Confidence:** High. The old revision was derived from step count plus retained log count. The retained log count is fixed at 2,000 once pruning begins, so later output did not change that value.
- **Module:** Deployment progress stream
- **File:** `src/deploy.rs`
- **Function:** `events`, `job_event_revision`, `prune_job_logs`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`; pre-fix audit branch `cea99e6e33850d82ed8ebc53a86c76285319afb1`; fix `e4d128ea3eb467f38edafde1d320f7cd7d68a925`
- **Affected behavior:** Long-running builds could append output after the 2,000 newest log rows were retained, while the SSE revision remained unchanged. The browser then received heartbeats but not the newly appended log/job state.
- **Technical evidence:** `prune_job_logs` retains the newest 2,000 rows. The previous SSE revision used the count of those retained rows, not a monotonic log cursor. `read_job` separately loaded the job, all steps, 20 recent logs, and the retained log count on every 900 ms heartbeat.
- **Safe reproduction:** In an in-memory SQLite database, create a job, append enough lines to reach 2,000 retained records, capture the old revision calculation, append another line, and compare the revision. The retained count remains 2,000 even though the newest row advances. No server or external service is involved.
- **Root cause:** Retention count was used as a change cursor even though pruning makes the count constant.
- **Security/functional impact:** Live deployment diagnostics stopped advancing for sufficiently verbose/long builds. The durable logs remained available through the logs endpoint, but the live view could appear stalled.
- **Proposed fix:** Base change detection on job state, step count, and the latest retained log ID. Load the full job payload only when that revision changes; send a lightweight heartbeat otherwise.
- **Implemented change:** Added `JobEventRevision` and an indexed summary query. A new log advances the revision even when the retained row count remains capped. The SSE handler now avoids rebuilding the full job payload on unchanged heartbeats, while retaining authorization checks and terminal-state delivery.
- **Tests added:** `deployment_output_retention_and_sse_revision_advance_at_limit` fills the retention window, appends one more line, checks the row cap and newest message, and asserts the revision advances.
- **Regression risks:** Step changes are represented by new step rows and log events; job state is part of the revision. If a future writer mutates a step in place without adding a log or changing job state/count, the SSE revision would need an additional step-update marker.
- **Commit:** `e4d128ea3eb467f38edafde1d320f7cd7d68a925`
- **Remaining risk:** The SSE stream still polls at 900 ms per subscriber. High subscriber counts should be measured on a representative host before replacing polling with a notification mechanism.

### PERF-002 — Shared SQLite mutex may serialize concurrent request work

- **ID:** PERF-002
- **Title:** All server handlers share one synchronous SQLite connection behind a process-wide mutex.
- **Severity:** P2 / Medium
- **Status:** AUDIT_REQUIRED
- **Confidence:** High that access is serialized; impact is not measured.
- **Module:** Database access across Axum handlers and workers
- **File:** `src/main.rs`
- **Function:** `AppState` and database initialization; consumers use `state.lock()` throughout modules
- **Commit SHA:** `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** Concurrent handlers wait for the same `std::sync::Mutex<rusqlite::Connection>` and execute SQLite work synchronously. A burst of API requests, metrics writes, a deployment log stream, or a slower query can therefore increase wait time for unrelated requests.
- **Technical evidence:** `AppState` is `Arc<Mutex<Connection>>`; SQLite uses WAL and a 5-second busy timeout. This confirms serialization and a bounded lock wait, but does not establish a production latency problem.
- **Safe reproduction:** A meaningful reproduction needs a disposable ARG instance with representative read/write traffic and measured lock wait / request latency. A synthetic mutex-only benchmark would not represent SQL, disk, or real handler behavior and was not treated as proof.
- **Root cause:** The connection and lock are shared for straightforward synchronous database access from many modules.
- **Security/functional impact:** No correctness or authorization defect was demonstrated. Under concurrent load, latency and Tokio worker blocking are possible.
- **Proposed fix:** First instrument lock wait and database query duration in a test build and benchmark realistic read/write mixes. If contention is material, compare a dedicated DB worker or a small connection pool while preserving migration ordering, SQLite transaction semantics, and foreign-key settings.
- **Implemented change:** None. A database architecture change without representative measurements would create broad risk across hundreds of call sites.
- **Tests added:** None; this remains a measurement task, not a confirmed regression.
- **Regression risks:** Prematurely switching to an async mutex or pool can introduce lock-across-await problems, inconsistent per-connection PRAGMAs, transaction behavior changes, and new migration races.
- **Commit:** —
- **Remaining risk:** No representative VPS load test or lock-wait instrumentation was available in this environment.

### PERF-003 — Deployment queue has no global admission cap

- **ID:** PERF-003
- **Title:** The deployment runner is single-worker, but the number of distinct queued application jobs is not globally bounded.
- **Severity:** P2 / Medium
- **Status:** AUDIT_REQUIRED
- **Confidence:** High for the schema/worker behavior; product impact depends on expected fleet and operator usage.
- **Module:** Deployment queue
- **File:** `src/deploy.rs`
- **Function:** `insert_job`, `persist_new_job`, `claim_next`, `start_worker`
- **Commit SHA:** `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** One worker runs one deployment at a time and a partial unique index allows only one active job per application. However, different applications can each add a queued job; there is no global queued-job limit.
- **Technical evidence:** `claim_next` selects the oldest queued record and `start_worker` awaits that job before claiming another. The enqueue path checks per-application activity but does not check a global queue count.
- **Safe reproduction:** On an isolated in-memory database, enqueue jobs for distinct application IDs and observe that each can remain queued while the worker is occupied. No deployment command or VPS resource is involved.
- **Root cause:** Admission control is per application; the queue relies on the administrator being a trusted actor.
- **Security/functional impact:** A large backlog can retain database rows and cause long wait times. Enqueue requires administrator access; no unauthenticated or cross-role queue attack was found.
- **Proposed fix:** Choose an explicit product limit (global and possibly per actor) based on desired burst deployments and retention expectations, then reject excess submissions with a stable 429/503 response. Do not set an arbitrary cap without a product policy.
- **Implemented change:** None; a safe queue limit needs a product capacity decision and a migration-free implementation can be added once the intended limit is known.
- **Tests added:** Existing tests cover per-application serialization and FIFO claim behavior; no test exists for a global maximum because none is implemented.
- **Regression risks:** A cap that is too low blocks legitimate fleet releases; a cap that counts finished jobs or retries incorrectly will make capacity inconsistent.
- **Commit:** —
- **Remaining risk:** Administrators can create a long queue across many applications.

## Resource controls observed

| Area | Existing bound / behavior | Audit result |
|---|---|---|
| Deployment execution | One worker awaits one job at a time; one active job per application; queued jobs are FIFO | Useful CPU/RAM bound, but no global queue admission cap (PERF-003) |
| Deployment output | Per-job recent log retention is 2,000 rows; page size is bounded; process output is sanitized and bounded at recording | Bounded storage and response; SSE revision defect fixed by PERF-001 |
| Archive upload | 256 MiB upload cap, one upload writer, per-chunk free-space reserve checks and a 15-minute total deadline | Previously regression-tested in `05-deployment.md` |
| S3 transfer and inventory | Bounded transfer slots, object/time scan caps, queue cap, and retry backoff | Previously regression-tested in `06-files-and-s3.md` |
| Metrics history | User-selectable retention is constrained to 1, 3, 7, 14, or 30 days; sampler prunes old samples | Bounded history; cleanup work occurs with sampling and settings changes |
| Frontend polling | Deployment polling is single-flight, times out, and aborts on cleanup; topology probes use a bounded worker count | Previously regression-tested in `09-frontend.md` |
| Shared DB access | Single synchronous connection serialized by `std::sync::Mutex` | Contention impact is unmeasured (PERF-002) |

## Verification

- `PATH="$HOME/.cargo/bin:$PATH" cargo test --locked deployment_output_retention_and_sse_revision_advance_at_limit -- --nocapture` — passed (1 targeted test).
- `PATH="$HOME/.cargo/bin:$PATH" cargo test --locked --all-targets -q` — passed (246 tests).
- `PATH="$HOME/.cargo/bin:$PATH" cargo check --locked --all-targets` — passed.
- `git diff --check` — passed.
- `cargo fmt --all -- --check` — failed because the baseline contains formatting differences across many unrelated Rust files. `rustfmt --edition 2021 --check src/deploy.rs` also reports pre-existing style drift elsewhere in that file; the new helper, SSE change, and regression test were formatted to rustfmt's output. No repository-wide formatting was applied because that would create a broad unrelated diff.
- No representative CPU/RAM/latency benchmark, multi-user load test, systemd build, or production-like SQLite I/O benchmark was available. Runtime performance impact is not claimed beyond the observed reduction in steady-state SQL statement count for an unchanged SSE heartbeat (four reads before, one revision query after).

## Phase result

One confirmed P2 deployment progress defect was fixed and regression-tested. Two resource-policy questions remain explicitly unverified. No database architecture change or concurrency expansion was made without benchmark evidence.
