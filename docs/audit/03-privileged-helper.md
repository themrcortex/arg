# 03 — Privileged helper and root operations

## Scope and evidence

- Baseline: `development` at `eec273cde037fa57edc1b5ab042fda7acc26a32c`.
- Audited revision after this phase: `52824d034b0f988785866b89d0f6f7decb6894cc` on `audit/development-hardening`.
- Read-only source review covered the installed `arg.service` and `arg-privileged.service` units, Unix socket setup, `SO_PEERCRED`, helper dispatch, file operations, systemd service operations, Node build sandbox, and Laravel background service generation.
- No production host, live database, or user data was accessed. Regression tests use in-memory inputs and local temporary paths.
- No database migration was needed.

## Boundary map

| Boundary | Observed behavior |
|---|---|
| Web process | `install.sh` runs the panel as `User=arg`, `Group=arg` under `arg.service`, with filesystem and kernel sandboxing. |
| Helper process | `arg-privileged.service` runs the same binary as root and creates `/run/arg-privileged/manager.sock` under a root-owned runtime directory. The socket is mode `0660`; runtime directory mode is `0750`, group `arg`. |
| Caller authentication | `SO_PEERCRED` supplies caller PID/UID. Before this audit, the helper accepted any process with UID `arg`. It now also requires the caller's cgroup to be the installed `/system.slice/arg.service` unit. |
| Request protocol | Newline-delimited JSON, maximum request 4 MiB, now read with a 10-second timeout. Only 8 client connections are admitted at once; privileged work remains capped at 8 tasks. |
| Response protocol | JSON newline response capped at 32 MiB by the helper and by the panel client. Incomplete or oversized replies fail as a gateway error. The cap preserves the existing 16 MiB file-download contract after Base64 expansion. |
| Operations | `execute_privileged_request` matches explicit action names and dispatches to feature helpers. System commands are generally started with fixed executable paths/argv; no user request is passed to a general shell. The root terminal is an intentional exception with its own route-level authorization, one-time ticket, PTY validation, and process tracking. |
| Node builds | `systemd-run` uses `DynamicUser=yes`, app-specific group, filesystem isolation, resource ceilings, and `InaccessiblePaths` for ARG's data/config/binary directories. |
| Laravel workers | The generated queue/scheduler/Horizon units run application code as `User=arg`, the same UID as the panel. This was the critical trust-boundary issue recorded below. The helper now rejects those units by cgroup even though the UID matches. |

## Findings and changes

### PRIV-001 — Any `arg` process could invoke the root helper

- **ID:** PRIV-001
- **Title:** The helper trusted UID `arg` without identifying the panel process.
- **Severity:** P1 / High
- **Status:** CONFIRMED
- **Confidence:** High; the helper and both service-unit templates establish the same UID, and the helper action path is directly reachable over the socket.
- **Module:** Privileged helper / Laravel application services
- **File:** `src/main.rs`, `src/laravel.rs`, `install.sh`
- **Function:** `privileged_peer_allowed`, `process_belongs_to_arg_service`, `laravel_service_helper`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** Any process running with UID `arg` could connect to the root-owned helper socket. Laravel queue/scheduler workers execute deployed application code as `arg`; a compromised worker could bypass HTTP authentication and request root helper actions.
- **Technical evidence:** The socket directory was root:`arg` mode `0750`, the socket mode `0660`, and the peer check only compared `SO_PEERCRED.uid` with the `arg` account. Laravel service units set `User=arg` and `Group=arg`; the helper dispatch includes privileged filesystem, service, package, TLS, maintenance, database, and application-management operations.
- **Safe reproduction:** `privileged_helper_accepts_only_the_panel_systemd_service_cgroup` verifies that `/system.slice/arg.service` is accepted while `arg-app-…-queue.service` and a user service named `arg.service` are rejected. No helper action or system mutation is executed by the test.
- **Root cause:** UID equality was treated as proof that a caller was the web application, even though deployed application workers shared that account.
- **Security/functional impact:** Application code running under a shared service UID could reach root operations without an ARG administrator session. This violated privilege separation.
- **Proposed fix:** Validate peer credentials and require the peer's cgroup to be the exact installed ARG panel service. Reject requests if `/proc/<pid>/cgroup` cannot be read or the process is outside that unit.
- **Implemented change:** `privileged_peer_allowed` now checks PID, UID and cgroup. Exact `/system.slice/arg.service` membership is required; worker units and unrelated processes are rejected. Existing admin checks on HTTP handlers and action-specific validation remain in place.
- **Tests added:** `privileged_helper_accepts_only_the_panel_systemd_service_cgroup`.
- **Regression risks:** A nonstandard deployment that renames `arg.service` or runs the panel outside systemd will lose helper access. The installer-generated service name is fixed to `arg.service`; unsupported layouts should fail closed with the existing helper-unavailable response.
- **Commit:** `d2f5f19 fix(security): bind privileged helper to panel service`
- **Remaining risk:** The check assumes the standard systemd cgroup path on supported Debian/Ubuntu installs. A clean-install and upgrade test on those distributions is still required. This change does not isolate one Laravel site from another site running under the shared `www-data` PHP-FPM user; it closes the path to the root helper.

### PRIV-002 — Privileged socket clients could occupy unbounded tasks and memory

- **ID:** PRIV-002
- **Title:** Accepted helper connections had no admission limit or request read deadline.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High; the accept loop spawned a task for every socket connection, while the work semaphore was acquired only after the complete request had been read.
- **Module:** Privileged helper IPC
- **File:** `src/main.rs`
- **Function:** `run_privileged_helper`, `read_privileged_line`, `serve_privileged_connection`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** Local callers able to access the socket could open many slow connections. Each task could retain a request buffer up to 4 MiB indefinitely before the execution semaphore applied. The helper's `LimitNOFILE=1024` bounded descriptors but did not prevent a large number of read tasks/buffers.
- **Technical evidence:** The accept loop called `tokio::spawn` before request parsing; `PRIVILEGED_REQUESTS` was acquired after parsing; request framing used `read_line` with a byte cap but no timeout.
- **Safe reproduction:** `privileged_socket_reader_rejects_oversized_and_stalled_requests` uses in-memory Tokio duplex streams to show oversized and silent clients are rejected without connecting to the installed socket.
- **Root cause:** Execution concurrency was bounded, but connection admission and request-read duration were not.
- **Security/functional impact:** A local process in the allowed account/group could increase root-helper task and memory use and delay legitimate management operations.
- **Proposed fix:** Bound connections before spawning, enforce a short deadline while reading, and cap response serialization and client reads.
- **Implemented change:** The helper admits at most 8 connections, reads at most 4 MiB within 10 seconds, encodes at most 32 MiB per response, and the client rejects oversized or incomplete replies. The 32 MiB response limit leaves room for a 16 MiB file after Base64 expansion.
- **Tests added:** `privileged_socket_reader_rejects_oversized_and_stalled_requests`; `privileged_socket_response_is_bounded_and_must_be_complete`.
- **Regression risks:** Requests larger than 4 MiB and helper replies larger than 32 MiB fail closed. Current file upload/download contracts fit those limits; oversized directory listings may return a controlled error.
- **Commit:** `d2f5f19 fix(security): bind privileged helper to panel service`; `52824d0 fix(security): bound privileged socket responses`
- **Remaining risk:** Saturation behavior drops newly accepted connections without a structured busy reply. The panel reports the helper as unavailable; client retry/backoff UX is not changed here.

### PRIV-003 — Disk scans retained every path although output was capped

- **ID:** PRIV-003
- **Title:** Disk scan memory grew with the entire scanned tree.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High; the previous implementation appended one allocated path string for every file and directory, then sorted and truncated the arrays to 100/100/2000 results.
- **Module:** File manager / privileged disk analysis
- **File:** `src/files.rs`
- **Function:** `scan_directory_size`, `scan_disk_usage`, `LargestScanEntries`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** Deep scans may visit 500,000 entries over up to 90 seconds. Retaining every path caused memory proportional to the number and length of all paths, even though the API returned only bounded top entries.
- **Technical evidence:** The former `files` and `directories` vectors were populated per entry, sorted after recursion, and truncated only while serializing the response. `seen_files` already had an entry-count ceiling for hard-link deduplication.
- **Safe reproduction:** `disk_scan_keeps_only_bounded_top_entries_in_deterministic_order` adds more candidates than the collector capacity and verifies bounded length, top-size ranking, and stable tie order.
- **Root cause:** Output truncation occurred after unbounded intermediate collection.
- **Security/functional impact:** A large directory tree could consume substantial extra memory and pressure the VPS while scanning. The scan already had entry, depth, duration and single-scan limits, but these did not bound stored path vectors.
- **Proposed fix:** Maintain bounded top-K collections during traversal and count direct children separately from retained paths.
- **Implemented change:** Three min-heaps retain at most 100 largest files, 100 largest directories, and 2,000 direct child directories. Ordering is size descending then path ascending. The separate hard-link set remains bounded by the existing entry limit.
- **Tests added:** `disk_scan_keeps_only_bounded_top_entries_in_deterministic_order`; existing expandable-child and scan-limit tests still pass.
- **Regression risks:** Equal-size selection is now deterministic by path and may differ from the old filesystem iteration order. The returned byte totals and result caps are unchanged.
- **Commit:** `512ba90 fix(files): bound disk scan result memory`
- **Remaining risk:** `seen_files` still grows up to the configured scan entry cap to avoid double-counting hard links. It is bounded but can still use meaningful memory in deep mode; changing that would affect usage correctness and needs a measured design.

## Verified controls and review limits

- The socket is local-only and permission-protected by a root-owned runtime directory; helper peer identity is checked using `SO_PEERCRED`.
- Action selection is an explicit allowlist; request fields are further validated in the responsible feature helper. Unknown action names are rejected.
- The reviewed operational commands use argv-based process APIs. The only shell launched by the root-helper path is the separately authenticated root terminal; it is not a generic request-provided command string.
- File operations reject parent traversal, use canonical paths for existing objects, skip symlinks during scans, bound recursive copy/ZIP work, and have ZIP-slip/symlink regression tests.
- Systemd service actions have a fixed action set, unit-name validation, and protected-service checks. A full per-action TOCTOU review across every Nginx, Docker, S3, MySQL, DNS, package and maintenance handler remains assigned to the relevant feature phases; it is not claimed complete here.
- `cargo test --locked --all-targets`: **220 passed, 0 failed** at this phase's final source revision.
- `cargo check --locked --all-targets`: **passed** at this phase's final source revision.
- `git diff --check`: passed before commits.
- `cargo fmt`/`rustfmt` could not be run in the container because the rustfmt component download failed certificate validation (`UnknownIssuer`). Baseline formatting had already failed the same tool gate. Formatting is therefore not claimed clean.
- No live helper, systemd service, filesystem, package manager, firewall, or production system was exercised.
