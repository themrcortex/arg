# 01 — Recheck of the six supplied findings

## Scope and baseline

The six observations were rechecked against development commit
`eec273cde037fa57edc1b5ab042fda7acc26a32c` before edits. Line references in
the evidence column are from that commit. Fixes and regression tests are on
`audit/development-hardening`; no production host or live data was used.

## ARG-001 — Nginx domain-check GET rejected without `Origin`

- **Severity / status / confidence:** P2 / CONFIRMED / High
- **Module, file, function:** Nginx API; `src/nginx.rs:1076-1085`; `check_nginx_domains`
- **Commit SHA:** baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`; fix `dfbda3c`
- **Affected behavior:** ordinary browser GET requests without an `Origin` header were rejected before domain validation.
- **Technical evidence:** the GET handler called `check_origin`; `src/main.rs:432-445` rejects absent `Origin`. The handler separately enforced `require_admin`.
- **Safe reproduction:** call the handler in its test fixture with an authenticated admin and invalid `invalid..example`, once without `Origin` and once with same-origin `Origin`. Before the fix the first request returned 403; after it both reach the domain validator and return 400. No DNS lookup is made for the invalid name.
- **Root cause:** a write-request Origin guard was applied to a read-only GET.
- **Impact:** authenticated admins could not use the browser domain check in clients that omit `Origin`; no privilege boundary was removed by the fix.
- **Implemented change:** removed only the inappropriate Origin check; admin authentication and input validation remain.
- **Tests added:** GET handler with/without Origin, anonymous request (401), and viewer request (403).
- **Regression risk:** low. Browser/API behavior is covered at the handler boundary; a real browser session was not available in this isolated audit.
- **Remaining risk:** none identified for this finding.

## ARG-002 — Stale root-terminal ticket changed the requested workspace scope

- **Severity / status / confidence:** P2 / CONFIRMED / High
- **Module, file, function:** Federation workspace guard and terminal proxy; `src/federation.rs:341-390`, `1957-2048`; `workspace_session_guard`, `proxy_workspace_terminal`
- **Commit SHA:** baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`; fix `c9e3ffb`
- **Affected behavior:** a user-mode terminal request carrying an old `arg_root_ticket` cookie could be checked against `workspace.root_terminal` instead of `workspace.terminal`.
- **Technical evidence:** both the guard and proxy selected the root scope from cookie presence. The actual terminal handler selects root mode from the `mode` query and consumes a short-lived, one-time root ticket only for that mode.
- **Safe reproduction:** compare scope selection for `/api/terminal?mode=user` and `/api/terminal?mode=root`, with an absent or stale ticket. No terminal or external server is started.
- **Root cause:** authorization scope was inferred from a credential's presence instead of the requested operation.
- **Impact:** availability failure for normal workspace terminal sessions with stale root-ticket cookies. No root privilege escalation was demonstrated; the destination still requires admin authorization and consumes a valid, session-bound root ticket for root mode.
- **Implemented change:** scope follows the parsed terminal mode; a root-ticket cookie is forwarded only for root-mode requests.
- **Tests added:** ordinary/default and root query scopes, root authorization route scope, and URL-encoded query parsing.
- **Regression risk:** medium-low. Scope logic is unit-tested; full remote WebSocket reconnect and reauthentication were not exercised against a second ARG instance.
- **Remaining risk:** multi-instance WebSocket behavior remains in the Federation phase audit.

## ARG-003 — File-action body limit disagreed with raw file limits

- **Severity / status / confidence:** P2 / CONFIRMED / High
- **Module, file, function:** File manager API/helper; `src/files.rs:168-194` and `src/files.rs:649-706`; `file_action`, `execute_action`
- **Commit SHA:** baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`; fix `c30dfb8`
- **Affected behavior:** the API compared Base64 text against 3 MiB while claiming a 2 MiB file limit; the helper decoded it and separately limited uploads to 2 MiB and editor saves to 1 MiB.
- **Technical evidence:** uploads use Base64 in the JSON request. A raw 2 MiB file encodes to 2,796,204 bytes, excluding JSON overhead. The route's 3 MiB body cap is sufficient for that encoded content and normal request metadata.
- **Safe reproduction:** local unit test encodes and writes a 2 MiB file through the helper, rejects a raw 2 MiB + 1 byte file, and tests the exact encoded API boundary. No system path is touched; tests use a unique temporary directory.
- **Root cause:** API length validation measured encoded bytes but reported a raw-file limit and did not distinguish upload from editor-save limits.
- **Impact:** oversized content traversed the API before helper rejection, producing inconsistent status/error behavior and needless IPC work.
- **Implemented change:** defined raw upload/editor limits and derived their Base64 ceilings; over-limit upload returns 413 with the matching 2 MiB message, editor content uses the 1 MiB ceiling. The helper checks remain authoritative.
- **Tests added:** exact encoded ceiling, ceiling + 1, 2 MiB helper success, 2 MiB + 1 helper failure, and editor ceiling.
- **Regression risk:** low. Frontend upload limit and route's 3 MiB JSON cap were reviewed and remain compatible.
- **Remaining risk:** the API still buffers Base64 JSON in memory, but the request size is bounded to 3 MiB; streaming upload redesign is outside this finding.

## ARG-004 — Hidden files consumed the application-tree display cap

- **Severity / status / confidence:** P2 / CONFIRMED / High
- **Module, file, function:** Application source browser; `src/app_code.rs:116-168`; `tree`
- **Commit SHA:** baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`; fix `44812e6`
- **Affected behavior:** `take(MAX_ENTRIES + 1)` ran before hidden/sensitive-name filtering, so a directory with many hidden entries could omit visible entries and report `truncated: false`.
- **Technical evidence:** `MAX_ENTRIES` was 500; filtered names include dotfiles, `vendor`, `node_modules`, `storage`, and secret-key names.
- **Safe reproduction:** pure listing test feeds more than 500 hidden entries followed by visible files and confirms the visible files remain; a second test confirms a 500-visible-row cap and accurate truncation reporting.
- **Root cause:** the visible-entry cap was applied to raw directory entries.
- **Impact:** incomplete source browsing and misleading pagination/truncation state. Sensitive names were still filtered and were not returned by the old code.
- **Implemented change:** filter before applying the 500 visible-row cap, sort results deterministically, and impose a separate 10,000 raw-entry scan ceiling. The response reports truncation if either ceiling is reached.
- **Tests added:** hidden-heavy directory ordering, visible-row limit, and raw-scan truncation propagation.
- **Regression risk:** low. Directories over 10,000 raw entries now explicitly return a bounded partial listing.
- **Remaining risk:** the API still has no continuation-token pagination; bounded partial results are indicated by `truncated`.

## ARG-005 — Disk scan lacked a direct Origin guard and scan concurrency bound

- **Severity / status / confidence:** P2 / CONFIRMED (missing defense-in-depth controls) / High
- **Module, file, function:** Disk scan API/helper; `src/files.rs:72-104`; `scan_disk`
- **Commit SHA:** baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`; fix `cce425a`
- **Affected behavior:** this admin-only JSON POST lacked the Origin check used by other writes; concurrent scans could occupy the helper's general request slots.
- **Technical evidence:** no global Origin middleware exists. Session cookies use `SameSite=Strict`; the Axum route expects JSON and the application has no permissive CORS layer, so a browser CSRF exploit was not reproduced. The helper bounds scans by entries/depth and by 12 seconds (`standard`) or 90 seconds (`deep`), but its general concurrency pool is eight operations.
- **Safe reproduction:** isolated handler tests show missing/foreign Origin is rejected before authentication/helper work, same-origin proceeds to the expected 401 without a session, and a second active scan permit is rejected. No disk scan is run by these tests.
- **Root cause:** the endpoint did not use the application's standard write Origin check and had no scan-specific concurrency gate.
- **Impact:** no exploitable browser CSRF was confirmed under the current cookie/CORS/JSON controls. A burst of authenticated scans could consume helper capacity and delay unrelated privileged work.
- **Implemented change:** added the standard Origin check, a one-scan-at-a-time gate in both API and helper execution contexts, and HTTP 429 for concurrent scans. Existing bounded scan modes and admin authorization remain.
- **Tests added:** absent Origin, foreign Origin, same-origin without authentication, and concurrent scan rejection.
- **Regression risk:** low. The workspace API proxy supplies the remote HTTPS Origin; real reverse-proxy/browser behavior remains a deployment test.
- **Remaining risk:** helper scan deadlines are checked during traversal; an individual blocking filesystem syscall on a stalled network filesystem cannot be preempted by the traversal deadline. Such mounts need separate operational testing.

## ARG-006 — Private-network draft status was already truthful

- **Severity / status / confidence:** P3 / NOT_AN_ISSUE / High
- **Module, file, function:** `src/private_networks.rs:29-97`; `list`, `create`; `frontend/src/FleetManagementPanels.tsx:81-90`
- **Commit SHA:** baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`; no code change
- **Affected behavior:** none found. The concern was that a draft might appear to enforce connectivity.
- **Technical evidence:** the list returns `overlay_ready:false` and `enforcement:"not_configured"`; creation stores `enforced=0` and says no traffic was allowed or blocked. The UI calls this a draft, says WireGuard/routes/firewall are not configured, and labels records as waiting for an Overlay.
- **Safe reproduction:** inspect the local handler and rendered copy; no policy is applied to a server.
- **Root cause:** not applicable.
- **Impact:** no discrepancy between backend behavior and the reviewed Persian UI was found.
- **Proposed/implemented change:** none. Keep the capability in the backlog until actual overlay enforcement exists.
- **Tests added:** none required for a verified non-issue; existing tests validate policy input bounds.
- **Regression risk:** the UI's `enforced` badge must continue to be driven only by backend data.
- **Remaining risk:** actual WireGuard, route, and firewall enforcement is not implemented in this version and must not be represented as active.

## Phase result

Four implementation bugs were corrected (ARG-001 through ARG-004), and the
disk-scan hardening in ARG-005 was added after confirming that no global Origin
middleware covers it. ARG-006 was reviewed and left unchanged because the
backend and UI accurately identify the feature as a non-enforced draft.

Focused regression tests passed at the time of each fix. Final phase gate:
`cargo check --locked --all-targets` passed, and `cargo test --locked
--all-targets` passed all 211 tests. No database migration was needed. The
repository-wide rustfmt and Clippy baseline failures remain as recorded in
`00-baseline.md`; they are not represented as passing gates.
