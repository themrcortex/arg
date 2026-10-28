# 09 — Frontend state, requests and interaction audit

## Scope and revision

- Reference source: requested `development` baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c` (`0.7.3-dev.1`).
- Frontend changes were made only on `audit/development-hardening`.
- Reviewed the hash route registry and lazy page map, the shared API wrapper, dashboard data hook/cache, deployment detail and job polling, the application registry across workspace modes, S3 connection and object browsing, the federation topology view, terminal lifecycle, service-worker caching rules, and polling/effect cleanup in the routed pages.
- No browser session, live ARG API, remote Node, S3 provider, or user data was used. This environment does not have a React component/E2E test runner installed.

## Route and interaction map

The application uses hash navigation with these page IDs: `dashboard`, `system`, `nodes`, `apps`, `ssh-keys`, `maintenance`, `mysql`, `network`, `ports`, `firewall`, `nginx`, `docker`, `files`, `s3`, `dns`, `terminal`, `packages`, `services`, `users`, `audit`, `settings`, `account`, and `bug-report`. `App.tsx` lazy-loads major feature modules; `pages.tsx` still hosts several system and Nginx surfaces. Application details use a second hash route. Most management pages share `lib/api.ts`, which carries same-origin cookies, routes workspace requests through the selected Node, and normalizes HTTP errors into `ApiError`.

| Area reviewed | Evidence in current source | Result / limit |
|---|---|---|
| Route and lazy loading | `navigation.ts`, `App.tsx`, `pages.tsx` | Route map matches the baseline inventory. No route was removed by this phase. |
| API wrapper and workspace routing | `lib/api.ts` | Same-origin credentials and Node proxy selection are explicit. There is no default request timeout; some calls supply a timeout or an abort signal, while other operations intentionally remain open-ended. A broad default cannot safely be added without separating long-running operations. |
| Dashboard and history range | `pages.tsx`, `lib/heavy-cache.ts` | The range changes the history query key and cache entries are generation-guarded, but the React state writer was not guarded against an older range response. Fixed below. |
| Application registry and workspace switch | `ApplicationsPage.tsx` | Remote registry requests have an 8-second timeout, but a completed old workspace/archived-mode request could still replace the current list. Fixed below. |
| Deployment detail and logs | `ApplicationsPage.tsx` | Detail and log requests were awaited together but wrote unconditionally; rapid selection could display the prior Job in the current dialog. Fixed below. Active Job polling had an interval independent of completion; bounded below. |
| S3 connections and object browser | `S3StoragePage.tsx` | Bucket-list effects had cancellation guards; object pagination and provider/connection refresh did not reject stale responses. Fixed below. |
| Terminal lifecycle | `TerminalPage.tsx` | WebSocket, timers, xterm listeners, resize observer and fullscreen listener cleanup are present in the reviewed effect. No terminal runtime/browser integration test was available. |
| Federation topology | `AllServersPage.tsx`, `FederationTopologyWidget.tsx` | Polling is visibility-aware and serialized with an in-flight guard; node probes are bounded to two workers and have a 7-second request deadline. The visual pulse is based on actual request start/response payload size, while animation duration is presentation timing. |
| Service worker | `main.tsx`, `service-worker.ts`, offline tests | Only same-origin assets/navigation are cache candidates; API paths and mutations are excluded by the existing tests. |
| Accessibility / responsive behavior | Shared Radix components and page source | Focus/dialog behavior is inherited from Radix in the reviewed dialogs. Keyboard, screen-reader, RTL and viewport behavior were not exercised in a browser or automated accessibility runner; this is not marked as fully verified. |

## Findings

### UI-001 — Stale asynchronous responses could overwrite the currently selected view

- **ID:** UI-001
- **Title:** Several list/detail views accepted results from requests started for an older selection or filter.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High. The baseline handlers wrote state after `await` without checking that their resource context was still current. A deferred-response test reproduces the ordering condition.
- **Module:** Dashboard, applications, deployments and S3 object browser
- **File:** `frontend/src/pages.tsx`, `frontend/src/ApplicationsPage.tsx`, `frontend/src/S3StoragePage.tsx`
- **Function:** `useDashboard.refresh`; `ApplicationsPage.load`; `DeploymentPanel.openJobDetails`; `S3StoragePage.refreshObjects` and `load`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** If a user changed the dashboard history range, workspace, selected Deployment Job, S3 connection, bucket or prefix before the earlier request completed, the old response could replace state belonging to the new selection. In the Job dialog, an earlier request's `finally` could also clear the loading state for the newer selection.
- **Technical evidence:** At baseline `ApplicationsPage.tsx:300-315`, `openJobDetails` wrote the detail/log response and cleared busy state without a request identity. At `ApplicationsPage.tsx:886-923`, the application registry wrote after local and remote requests regardless of workspace changes. At `S3StoragePage.tsx:157-166`, object rows were assigned without binding the result to the selected connection/bucket/prefix. At `pages.tsx:140-156`, the dashboard history query was derived from `range`, but response state was unguarded.
- **Safe reproduction:** Use deferred mock responses: start request A, switch context and start B, resolve B, then resolve A. The old handlers assign A last. The added regression test exercises this response order without contacting an API or server.
- **Root cause:** Async requests had no common request-generation ownership check, so completion order was treated as selection order.
- **Security/functional impact:** The UI could show the wrong bucket objects, logs, history range or server's application list. This could mislead an administrator when reviewing or acting on a resource. No cross-account authorization bypass was found in this behavior.
- **Proposed fix:** Associate each replaceable view load with a monotonically increasing request epoch; only the latest request may write results, errors or loading state. Invalidate the epoch immediately when a bucket/connection changes and on effect cleanup.
- **Implemented change:** Added `createRequestEpoch`. Applied it to dashboard metrics/history, workspace-aware application registries, deployment Job details/list reads, S3 connection refresh and object pagination. Bucket selection invalidates the prior object request synchronously. Closing the Job dialog invalidates its active request.
- **Tests added:** `an older asynchronous response cannot replace newer page state`; `changing the resource invalidates its in-flight request`. `npm run check`, `npm run test:offline`, and production build pass.
- **Regression risks:** Stale responses are ignored but some ordinary GET requests are not actively aborted; their network work continues until completion. The selected S3 object list and application registry now protect display state, but this does not create a backend cancellation contract.
- **Commit:** `2d26692d5662f8952dd3def79e738d9d3eed182a` (`fix(frontend): ignore stale async page responses`).
- **Remaining risk:** No component-level browser test was available. The response-order test validates the shared guard used by the handlers, not Radix dialogs or rendered pages.

### UI-002 — Deployment status polling could overlap slow reads

- **ID:** UI-002
- **Title:** The active deployment Job list was polled on a fixed 1.8-second interval without preventing a second read while the previous read was pending.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High from the baseline interval and request control flow; the network-delay condition was tested at the guard level rather than against a running panel.
- **Module:** Deployment status and persistent logs UI
- **File:** `frontend/src/ApplicationsPage.tsx`
- **Function:** `DeploymentPanel.loadJobs` and active-job polling effect
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** When `/deploy/jobs` took longer than 1.8 seconds, each timer tick could start another list request. This needlessly increased load and allowed responses to arrive out of order.
- **Technical evidence:** Baseline `ApplicationsPage.tsx:260-280` did not track an in-flight read; the active-state effect at `ApplicationsPage.tsx:362-367` called it from `setInterval(..., 1800)`.
- **Safe reproduction:** Keep a mock `/deploy/jobs` response unresolved for more than one interval tick. The baseline callback starts another request before the first settles. No VPS or deployment was used.
- **Root cause:** Poll cadence was independent of request completion and request lifetime.
- **Security/functional impact:** Excess API work during slow connections and a higher chance of stale Job state. It did not execute duplicate deployments; this is a read-polling issue.
- **Proposed fix:** Permit one active list request, bound the read duration, and abort/invalidate it when the panel unmounts.
- **Implemented change:** Added a token-based single-flight gate, a 10-second read timeout, and cleanup abort/invalidation. Stale request cleanup cannot release a newer gate lease.
- **Tests added:** `single-flight polling rejects overlap and stale releases cannot unlock a newer request`. The broader frontend checks also pass.
- **Regression risks:** A slow status endpoint can delay the next poll until it finishes or times out; this trades a small amount of freshness for bounded concurrency.
- **Commit:** `2d26692d5662f8952dd3def79e738d9d3eed182a` (`fix(frontend): ignore stale async page responses`).
- **Remaining risk:** No live worker or browser was available to measure polling latency under VPS load.

### UI-003 — The shared API wrapper has no default deadline

- **ID:** UI-003
- **Title:** A number of one-shot API reads do not pass an `AbortSignal`, so the browser may remain in a loading state while a connection is stalled.
- **Severity:** P2 / Medium
- **Status:** AUDIT_REQUIRED
- **Confidence:** High that `fetch` has no built-in deadline; impact depends on route and transport behavior. A global timeout could break legitimate long operations and was not added.
- **Module:** Shared frontend networking
- **File:** `frontend/src/lib/api.ts`
- **Function:** `requestApi`, `api`, `apiForWorkspace`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** A stalled network can leave calls without an explicit timeout pending. Pages vary: some attach `AbortSignal.timeout`, while many short reads rely on server/proxy completion.
- **Technical evidence:** `requestApi` forwards the optional `RequestInit` to `fetch` and does not construct a deadline when `signal` is absent. `apiForWorkspace` allows the caller to pass a signal but does not supply one itself.
- **Safe reproduction:** Browser Fetch semantics establish that a request has no application-defined timeout when no signal is supplied. A real outage or proxy stall was not induced.
- **Root cause:** Timeout policy is selected at call sites; the wrapper has no route-class deadline policy.
- **Security/functional impact:** Some pages may remain loading during a stalled network, but globally aborting is unsafe for long uploads, downloads, terminal handshakes and operations with durable job IDs.
- **Proposed fix:** First classify API paths into short reads, streams and durable asynchronous operations; add explicit deadlines and cancellation only to short reads. Keep stream and job-following lifetimes distinct.
- **Implemented change:** No global change. The deployment Job list read now has a 10-second timeout and the component aborts it on cleanup.
- **Tests added:** No wrapper-level timeout contract test was added because no global deadline was introduced.
- **Regression risks:** Applying one default timeout to all APIs could interrupt S3 transfers, backup downloads or long-lived operations.
- **Commit:** `2d26692d5662f8952dd3def79e738d9d3eed182a` (limited Job polling deadline only).
- **Remaining risk:** Route-class timeout policy and corresponding API contract tests remain open.

## State and lifecycle review notes

- The frontend uses React state/effects and a small in-memory request cache; no Redux/Zustand store was found in the reviewed dependency/source map.
- Event/timer owners inspected in `TerminalPage`, the dashboard monitor, topology pages, deployment polling and S3 scan/mount polling have cleanup paths. This is source review only, not a heap or browser leak test.
- Dashboard metrics cache invalidation is generation-aware. The new epoch guard is separate: it prevents an old caller from rendering a result even when an underlying cache request is shared.
- API errors are normalized as `ApiError`; individual pages do not uniformly preserve a recoverable form's data, and form-by-form browser validation was not exhaustively exercised.
- The build reports the existing main JavaScript bundle above Vite's 500 KiB warning threshold (about 504.5 KiB minified, 156 KiB gzip in this run). No chunking change was made in this functional-audit phase.

## Verification and limits

- `npm run check` — passed.
- `npm run test:offline` — passed, 5 tests total (3 request-guard tests and 2 service-worker tests).
- `npm run build` — passed; Vite emitted the existing large-chunk advisory.
- `git diff --check` — passed.
- A rendered desktop/mobile browser pass, screen-reader/keyboard pass, React component tests, route-by-route API integration, WebSocket disconnect simulation, and real multi-Node workspace testing were not available. Those checks remain required before describing the UI as fully verified.
