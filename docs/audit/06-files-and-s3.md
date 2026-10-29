# 06 — File manager, storage and S3

## Scope and revision

- Reviewed the `development` baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c` on the isolated `audit/development-hardening` branch.
- Phase changes: `bd2e973` (ZIP archive depth), `52f224c` (S3 marker safety and bounded transfers).
- Reviewed `src/files.rs`, `src/app_code.rs`, `src/app_s3.rs`, and `src/s3/mod.rs`, including privileged file and network action dispatch.
- No live provider, production bucket, user filesystem, or server was contacted. S3 verification and DNS-rebinding behavior were reviewed statically and with offline unit tests.

## Trust boundaries and observed controls

- Browser file/S3 requests require an administrator for privileged changes, and write handlers that accept browser credentials check Origin. Network and filesystem actions are sent to the separately validated privileged helper.
- File-manager paths reject `..`, use canonical or lexical path handling, refuse symlinks for recursive copy/archive, reject ZIP traversal, symlinks and special entries, cap expanded ZIP size at 2 GiB and entry count at 50,000, and create extracted files exclusively.
- Application source trees filter hidden/sensitive paths and cap the returned visible listing. S3 credentials are stored in the secret vault and API models expose presence/rotation metadata rather than secret values.
- S3 object listing has bounded page size. Application S3 bindings explicitly say that `access_mode` is metadata and provider credentials/policies enforce actual object permissions; the UI repeats this limitation. This is **NOT_AN_ISSUE** because the interface does not claim ARG enforces the provider ACL.
- The rust-s3 SDK defaults each request to a 60-second timeout. The application-level inventory scan now adds an overall 15-minute/2-million-object ceiling; these are bounded limits, not guarantees that every provider responds within them.

## Findings

### S3-001 — Connection verification could overwrite an existing object

- **ID:** S3-001
- **Title:** The fixed `arg-verify` key was written without checking whether it belonged to the user.
- **Severity:** P1 / High
- **Status:** CONFIRMED
- **Confidence:** High; the original call unconditionally issued `PutObject` at `arg-verify` and the replacement behavior is covered by unit tests.
- **Module:** S3 connection verification
- **File:** `src/s3/mod.rs`
- **Function:** `test_bucket_connection`, `verify_marker_action`, `verify_marker_bucket`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** Testing an S3 connection could replace user data or create an extra object version whenever a bucket already had an object named `arg-verify`.
- **Technical evidence:** Before the fix, `test_bucket_connection` called `put_object_with_content_type("arg-verify", ...)` directly. There was no HEAD/read preflight and no conditional create header.
- **Safe reproduction:** `verify_marker_never_replaces_existing_unrelated_objects` demonstrates the existing-key conflict decision; `verify_marker_upload_uses_an_atomic_create_only_condition` checks `If-None-Match: *`; `verify_marker_requires_explicit_missing_key_evidence_before_creation` verifies that an ambiguous 404 or missing bucket is not treated as a free key. These tests make no network request.
- **Root cause:** Verification used one deterministic key as a disposable test object without reserving it or checking ownership.
- **Security/functional impact:** A routine health check could destroy one customer object and, with versioning enabled, create unnecessary retained versions and charges.
- **Proposed fix:** Read and bound any pre-existing marker, preserve non-ARG content, and create only after explicit missing-key evidence using an atomic create-only condition.
- **Implemented change:** The verifier reads a pre-existing marker with a 1 KiB bound. A different object or an inconclusive read fails safely; the exact ARG marker is reused and reported as degraded because write permission was not retested. New markers require explicit `NoSuchKey` evidence and are PUT with `If-None-Match: *`.
- **Tests added:** `verify_marker_never_replaces_existing_unrelated_objects`; `verify_marker_upload_uses_an_atomic_create_only_condition`; `verify_marker_requires_explicit_missing_key_evidence_before_creation`.
- **Regression risks:** Some S3-compatible services may not implement conditional PUT. Those services will now fail the marker write safely instead of risking overwrite; provider compatibility still requires real-service testing.
- **Commit:** `52f224c fix(s3): protect marker data and bound transfers`.
- **Remaining risk:** No AWS, ArvanCloud, ParsPack, R2, or MinIO endpoint was available for integration testing. A provider that ignores `If-None-Match` would violate the expected S3 conditional-write semantics and needs an explicit compatibility test.

### S3-002 — Object transfers could create unbounded temporary-file pressure

- **ID:** S3-002
- **Title:** Direct S3 uploads/downloads had no admission limit or free-space reserve across temporary transfers.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High; the prior handlers created one temporary file per request with no transfer semaphore or `statvfs` reserve check.
- **Module:** S3 object transfer and privileged network helper
- **File:** `src/s3/mod.rs`
- **Function:** `upload_object`, `download_object`, `execute_network_action`, `prepare_transfer_directory`, `validate_transfer_file`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** Concurrent large transfers could consume the system filesystem used by ARG. Interrupted requests/processes could leave transfer files behind; zero-byte uploads were rejected by the helper despite being valid S3 objects.
- **Technical evidence:** The previous upload route accepted up to 512 MiB and wrote a shared temporary file per request; download permitted up to 2 GiB. There was no active-transfer admission gate or free-space check before writes. Download cleanup was scheduled independently of response-stream lifetime.
- **Safe reproduction:** The code path was verified statically; no disk was filled. Regression tests cover the reserve boundary, cleanup guard, zero-byte object policy, and the bounded helper action set.
- **Root cause:** Size limits existed, but there was no concurrency budget, remaining-disk reserve, or request-lifetime ownership for temporary files.
- **Security/functional impact:** A burst of authenticated transfers could exhaust disk space and interrupt ARG or other services; orphan files could continue consuming space after failures.
- **Proposed fix:** Bound active transfers, preserve disk reserve on each chunk, constrain upload time and size, and tie temporary-file lifetime to request/stream lifetime with stale-file cleanup.
- **Implemented change:** Direct browser uploads are serialized, capped at 512 MiB, limited to 30 seconds idle and 15 minutes total, and check `statvfs` before each chunk while retaining 64 MiB free. Direct downloads keep their temporary file and exclusive slot until response-stream completion/drop. The privileged helper admits one S3 transfer at a time across its transfer actions, checks local-destination free space and retains the existing 2 GiB download cap. Temporary files are removed by RAII and old, service-owned mode-0600 files are cleaned in bounded batches. Empty object uploads are accepted.
- **Tests added:** `transfer_space_check_keeps_the_configured_reserve`; `dropped_transfer_cleanup_removes_temporary_file`; `transfer_file_size_validation_allows_empty_s3_objects_only_when_requested`; `only_object_transfers_use_the_bounded_helper_transfer_slot`.
- **Regression risks:** The single-transfer policy may reject overlapping transfers with HTTP 429; users must retry. The 64 MiB reserve is fixed and may be conservative on small VPS plans. A request already handed to the S3 SDK relies on the SDK's per-request timeout.
- **Commit:** `52f224c fix(s3): protect marker data and bound transfers`.
- **Remaining risk:** HTTP cancellation and disk-full behavior were not exercised with a mounted test filesystem or a live S3 service. API-level response streaming should be verified on a disposable server.

### S3-003 — Usage scans could paginate indefinitely and queue unbounded work

- **ID:** S3-003
- **Title:** Full-bucket usage scans had no object/time ceiling and one serialized worker did not bound queued tasks.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High; the old helper paginated until the provider stopped returning `is_truncated`, while callers spawned tasks that waited on a single semaphore without an admission ceiling.
- **Module:** S3 usage inventory and object pagination
- **File:** `src/s3/mod.rs`
- **Function:** `begin_usage_scan`, `execute_network_action` (`s3-scan-usage`, `s3-list-objects`)
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** A bucket with pathological pagination/size could occupy the helper for an unbounded duration; repeated scan requests could create a long-lived queue of spawned tasks. A malformed continuation token could also be forwarded without a size ceiling.
- **Technical evidence:** The original scan loop had no cumulative count/deadline and `begin_usage_scan` spawned work waiting on `S3_SCAN_SEMAPHORE.acquire()` without queue admission control. Listing checked prefix length but not continuation-token length.
- **Safe reproduction:** `bucket_usage_scan_stops_at_object_and_time_limits` covers the object and elapsed-time stop boundaries. The implementation returns failure, so partial inventory is not stored as a complete measurement.
- **Root cause:** Serialization limited simultaneous execution but did not limit the amount of work or waiting requests.
- **Security/functional impact:** A slow or very large bucket could hold resources and allow queued tasks to accumulate, delaying other panel work and misrepresenting inventory if partial results were stored.
- **Proposed fix:** Bound each scan and the admission queue; reject oversized continuation tokens before and after provider responses.
- **Implemented change:** Scans stop after 2,000,000 objects or 15 minutes, fail the snapshot instead of storing partial totals, and use a bounded 16-task queue with one active scan. Listing continuation tokens are capped at 4 KiB and control characters are rejected on input and output.
- **Tests added:** `bucket_usage_scan_stops_at_object_and_time_limits`.
- **Regression risks:** A legitimate bucket above the cap reports usage as failed/unknown until a future paginated or provider-reported inventory implementation is added. Excess scans receive HTTP 429 and can be retried later.
- **Commit:** `52f224c fix(s3): protect marker data and bound transfers`.
- **Remaining risk:** Queue admission and stale-snapshot recovery were not exercised against concurrent live requests or process restart. Static review confirms that a restarted worker can retry an old `scanning` record after its stale threshold, but no restart integration test was run.

### FILE-001 — ZIP creation recursively traversed arbitrarily deep directories

- **ID:** FILE-001
- **Title:** Creating a ZIP from deeply nested directories could recurse until stack exhaustion.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High; recursive descent had no depth argument or limit, and a 65-level regression fixture now proves the bounded rejection path.
- **Module:** Server file manager archive creation
- **File:** `src/files.rs`
- **Function:** `create_zip_archive`, `archive_directory`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** An administrator archiving a deeply nested directory could make the privileged helper consume excessive stack and potentially abort.
- **Technical evidence:** Before the fix, `archive_directory` recursively called itself for each child directory with no depth ceiling. The archive already had an entry cap, but a narrow deep tree can stay below that cap.
- **Safe reproduction:** `zip_creation_rejects_excessive_directory_depth_and_removes_partial_archive` builds a temporary 65-level tree, calls the archive operation, and verifies a clear error and removal of the partial ZIP.
- **Root cause:** Entry count and byte size were bounded, but recursive depth was not.
- **Security/functional impact:** A crafted or unusually deep tree could exhaust the privileged helper thread's stack and interrupt file operations.
- **Proposed fix:** Add a small depth ceiling and ensure every early archive failure removes the partial output.
- **Implemented change:** ZIP directory recursion is capped at 64 nested levels; failure goes through the existing partial-archive cleanup path, including failure to add the root ZIP directory entry.
- **Tests added:** `zip_creation_rejects_excessive_directory_depth_and_removes_partial_archive`.
- **Regression risks:** Directories deeper than 64 levels cannot be archived in one operation; users can archive a shallower subdirectory.
- **Commit:** `bd2e973 fix(files): cap ZIP archive directory recursion`.
- **Remaining risk:** No large-tree performance benchmark was run; entry count and expanded-size ceilings remain the primary limits for wide trees.

### S3-004 — Endpoint validation and S3 request resolution are not pinned to one DNS result

- **ID:** S3-004
- **Title:** DNS validation is subject to a resolve/validate/connect time-of-check/time-of-use gap.
- **Severity:** P1 / High
- **Status:** AUDIT_REQUIRED
- **Confidence:** High that the gap exists; exploitability depends on control of endpoint DNS and the provider/request path.
- **Module:** S3 endpoint validation and HTTP transport
- **File:** `src/s3/mod.rs`; dependency `rust-s3 0.38.0`
- **Function:** `validate_endpoint`, `make_bucket`, rust-s3 request transport
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** ARG resolves the configured endpoint and rejects unsafe addresses before use, but the SDK later makes requests using the hostname and the HTTP client resolves it independently.
- **Technical evidence:** `validate_endpoint` performs local name resolution before credentialed operations. rust-s3 0.38.0 creates its own Reqwest client, exposes a client getter but no public client setter/connector hook, and builds requests with the original endpoint hostname. No resolved-IP pin is passed from validation into the actual socket connection.
- **Safe reproduction:** Static dependency/source trace only. No rebinding DNS, internal endpoint, or real credentials were used.
- **Root cause:** Validation and transport own separate DNS resolutions; the current SDK interface does not provide a supported way to pin a validated resolution while preserving TLS hostname verification.
- **Security/functional impact:** If an attacker can change DNS answers between validation and a credentialed request, the request could be routed to an unintended network destination. Existing private-address checks do not eliminate this race.
- **Proposed fix:** Add a reviewed transport that pins the resolved address while preserving the original TLS server name and HTTP Host, or vendor/patch an upstream SDK with a tested connector hook. Rejecting arbitrary custom endpoints would reduce scope but would break supported providers and is not proposed as a silent change.
- **Implemented change:** No transport change was made; the limitation is explicitly recorded rather than claiming endpoint validation prevents rebinding.
- **Tests added:** None; a secure transport-level test requires a controlled DNS/connector implementation.
- **Regression risks:** Replacing the S3 transport can affect AWS signing, path/virtual-host addressing, TLS validation, proxies, and provider compatibility.
- **Commit:** Documentation only in the phase report commit.
- **Remaining risk:** Open high-priority security work. Do not treat the current DNS precheck as DNS pinning.

### FILE-002 — Privileged file operations remain path-based across validation and use

- **ID:** FILE-002
- **Title:** File-manager validation and mutation use separate path lookups, leaving possible symlink/rename races.
- **Severity:** P1 / High
- **Status:** AUDIT_REQUIRED
- **Confidence:** Medium; path checks and later path-based syscalls are visible, but a deterministic end-to-end exploit was not reproduced in this phase.
- **Module:** Privileged server file manager
- **File:** `src/files.rs`
- **Function:** `lexical_existing`, `create_safe_directories`, `extract_zip_archive`, `copy_tree`, `remove_tree`, `execute_action`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** Several actions check canonical paths or symlink metadata and later reopen/mutate by pathname. A concurrent local process able to modify a selected parent directory may be able to change a path component between those steps.
- **Technical evidence:** ZIP extraction checks directory components with `symlink_metadata` and later calls path-based `OpenOptions::open`; copy and chmod similarly perform metadata checks followed by path-based operations. Final-component `O_NOFOLLOW` is not consistently used for these operations, and intermediate path components are not held by directory file descriptors.
- **Safe reproduction:** Static evidence and temporary-directory code review only. No privilege-escalation attempt or writes outside a test root were performed.
- **Root cause:** Path validation is string/path based rather than anchored to directory file descriptors for the full operation.
- **Security/functional impact:** Under an attacker-controlled writable directory and a privileged helper, a race could redirect a file operation outside the selected tree. The impact could include root-owned file creation or modification.
- **Proposed fix:** Follow up with descriptor-relative traversal (`openat`/`mkdirat` with `O_NOFOLLOW`, or `openat2` with an appropriate resolution policy), atomic publication, and race-focused tests for extraction/copy/chmod/delete. Avoid a blanket owner restriction unless product behavior is explicitly accepted.
- **Implemented change:** Not changed in this phase; existing traversal/symlink checks remain effective against non-racing paths. This is carried as a high-priority audit item, not reported as fixed.
- **Tests added:** Existing ZIP-slip, symlink-source, and recursive-copy tests were reviewed; no regression test for the race was added because the safe descriptor-relative design is not implemented.
- **Regression risks:** A descriptor-relative rewrite touches the privileged file manager's core operations and needs isolated review and tests before release.
- **Commit:** Documentation only in the phase report commit.
- **Remaining risk:** Open high-priority security work; phase 3 privileged-helper review must revisit and either fix or provide a tested mitigation.

## Phase tests

- `PATH=/workspace/.cargo/bin:$PATH CARGO_TARGET_DIR=/workspace/target cargo test --locked --all-targets`: **235 passed, 0 failed**.
- `PATH=/workspace/.cargo/bin:$PATH CARGO_TARGET_DIR=/workspace/target cargo check --locked --all-targets`: **passed**.
- `git diff --check`: **passed** before commits.
- `cargo fmt --all -- --check`: **fails on formatting differences across existing unrelated modules**; check-only, no project-wide formatting was applied.
- `cargo clippy --locked --all-targets -- -D warnings`: unavailable in the current toolchain because the `clippy` component is not installed. Baseline report records existing Clippy warnings from its earlier run.
- No database migration was required.

## Review limits and follow-up

- Test conditional marker writes and S3 transfer streaming against isolated AWS-compatible providers before production release.
- Implement and test IP-pinned S3 transport; existing DNS pre-validation is not sufficient to eliminate rebinding.
- Continue FILE-002 in the privileged-helper hardening work with descriptor-relative operations and race tests.
- Exercise cancellation, disk-full handling, and process restart cleanup on a disposable Debian/Ubuntu VM with a loopback S3 test service and a bounded test filesystem.

## Recheck on 2026-10-29

The earlier open status for S3-004 and FILE-002 is superseded by [16-risk-recheck.md](16-risk-recheck.md). Custom endpoint S3 requests now pass through a local, authenticated CONNECT proxy that connects only to the validated IP set and keeps TLS validation against the provider hostname. The proxy capability is HMAC-bound to the exact host, port, validated address set, and private-endpoint policy. A changed DNS result gets a different capability, while repeated identical resolutions reuse the bounded target slot. DNS answers are capped at 16 and proxy connection attempts share a 10-second deadline. Private-endpoint opt-in only permits RFC1918/ULA addresses; loopback, link-local, metadata, CGNAT, and other special-use targets remain blocked. `ListBuckets` and `CreateBucket` use the same pinned transport; an XML fixture covers the custom ListBuckets response parser. No live provider was available, so this does not claim all providers use identical signing or bucket APIs.

Privileged file mutations, ZIP creation/extraction, and editor save now walk directory paths without following symlink components and operate relative to held directory descriptors. The editor passes the original anchor into atomic replacement rather than resolving the path a second time. Regression tests replace a selected directory with a symlink after opening it and verify that operations stay on the original descriptor; deleting a symlink does not touch its target. Read-only listings and disk-usage scans retain their separate canonical-path handling. This change hardens the selected path from symlink escape; it does not reduce the file manager's existing administrator-level filesystem scope.

Copy, cross-filesystem move, ZIP creation, and ZIP extraction also check destination space before writing each file and retain a 64 MiB reserve. The arithmetic boundary is covered by `file_copy_preserves_the_free_space_reserve`; an actual nearly-full filesystem test still requires an isolated bounded-volume environment.
