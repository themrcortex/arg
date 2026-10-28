# 13 — Test coverage and regression strategy

## Scope and revision

- Source baseline: `development` at `eec273cde037fa57edc1b5ab042fda7acc26a32c` (`0.7.3-dev.1`), with audit fixes on `audit/development-hardening`.
- Reviewed test layout and ran the existing backend, frontend, documentation, and shell tests without contacting production systems.
- Verification was run on the audit branch after Phase 12 commits `adf5f7870663512a9e845b70d065dba3bde69cee` and `1d0932e7b49bc718a94f6c312457161359815983`. No standalone source-only test change was needed in this phase; follow-up tests are added alongside each concrete remediation.

## Existing automated coverage

- Rust tests are colocated with feature modules under `#[cfg(test)]`; `cargo test --locked --all-targets` currently runs 246 tests. Reviewed modules include authentication/API keys, app/deployment/runtime, archive and file handling, SQLite migrations, federation, helper request validation, Nginx/DNS, S3, backups, update behavior, and route construction.
- Frontend has a small Node test suite (`frontend/tests/*.test.mjs`) with five tests for stale-response epochs, single-flight polling, and offline service-worker request policy. Most React UI components do not have component-level or browser-driven tests.
- Shell regression tests cover updater cancellation/state, update branch selection, UFW behavior, public-IP detection, docs links/content, and GitHub workflow pinning/permissions.
- Module migration tests establish idempotence and selected preservation behavior. There is no single test that opens an old complete ARG database, runs the complete startup migration chain with injected failure at each boundary, then verifies recovery.
- Route tests ensure selected route groups can be assembled without collision; they are not full authenticated HTTP integration tests against a running Axum service.

## Verification run

| Check | Result | Notes |
|---|---|---|
| `cargo test --locked --all-targets` | Pass, 246 tests | Includes the ZIP traversal/symlink extraction regression. |
| `cargo check --locked --all-targets` | Pass | Also run after the ZIP crate update. |
| `npm run test:offline` in `frontend/` | Pass, 5 tests | Node's built-in test runner; no browser or network dependency. |
| `npm run check` in `frontend/` | Pass | TypeScript no-emit check. |
| `npm run build` in `frontend/` | Pass | Vite production build and service-worker stamp completed. Vite warned that the largest JS chunks exceed 500 kB minified. |
| `bash scripts/tests/test-docs-content.sh` | Pass | Canonical article count, localized page links, and legacy guide prefixes resolve. |
| `bash scripts/tests/test-workflow-security.sh` | Pass | Enforces SHA-pinned actions, checkout credential policy, and Pages write scopes. |
| Workflow/Dependabot YAML parse | Pass | Parsed all workflow files and Dependabot configuration. |

Some tests were run in the immediately preceding Phase 12 verification, with unchanged source state for those checks. The frontend build emits generated `frontend/dist` artifacts, which are part of the embedded application and must stay synchronized with source.

## Confirmed issue found by the release-artifact check

### UI-004 — Embedded frontend bundle was stale

- **Severity / status:** P2 / CONFIRMED; fixed.
- **Evidence:** The tracked `frontend/dist` was clean before the audit build, yet `bash scripts/verify-embedded-frontend.sh` reported that it was out of date. Running `npm ci && npm run build` generated a different application entry, lazy-page chunks, service worker and asset manifest. ARG embeds this directory in the Rust binary, so users installing/updating that binary would receive the old UI even though the checked source was newer.
- **Fix:** Rebuilt and committed the generated bundle in `47c5a82fedea9242f5e796ae883a37057f54dfac`.
- **Regression check:** `bash scripts/verify-embedded-frontend.sh` now reports `Embedded frontend is current.` CI already executes this check after frontend build.
- **Risk:** Re-run the same source/build/verification sequence whenever frontend source or its locked build toolchain changes; the CI gate prevents merging stale embedded output.

## End-to-end acceptance matrix

| Scenario | Available evidence | Status |
|---|---|---|
| Clean install, repair install, upgrade from a previous release | Bash syntax and helper-level shell tests | **Not end-to-end verified**; requires disposable Debian/Ubuntu VMs and systemd. |
| Admin login, key revocation, role checks | Unit tests for throttling, API-key owner state, session tickets, and role guards | **Partial**; no full HTTP/browser auth session test. |
| App source upload, runtime detection, build, health check, rollback | Unit tests for ZIP limits/traversal, buildpack detection, process cancellation, health path validation and release pointer rollback | **Partial**; no real Node/PHP project deployment or systemd runtime test. |
| Nginx domain and SSL flow | Parser, domain conflict and validation tests | **Partial**; no live Nginx reload or ACME issuance test. |
| S3 connection, upload, cross-bucket transfer | Endpoint validation, bounded transfer, missing-object behavior, cleanup and marker tests | **Partial**; no live AWS, ArvanCloud, ParsPack, MinIO, or R2 credentials/provider test. |
| Node pairing, remote workspace, update relay | Capability, endpoint and authorization-related unit tests | **Partial**; no two-panel integration test or network interruption fixture. |
| Terminal reconnect and root authorization | Root ticket and terminal scope unit tests | **Partial**; no browser WebSocket reconnect E2E. |
| Backup and restore | Consistent SQLite snapshot and metadata tests | **Partial**; full service restore/restart not tested. |
| Update cancellation and rollback | Shell helper and state unit tests | **Partial**; no compiler kill, service restart, and old-binary recovery VM exercise. |
| Responsive and accessibility behavior | TypeScript/build only | **Not verified** in a browser, screen reader, or viewport matrix. |

## Coverage gaps and required follow-up

- Add a local Axum integration harness with temporary database and fake privileged helper before claiming endpoint-level permission coverage.
- Add browser E2E for login, deployment progress/log persistence, file operations, S3 object navigation, Node selection, modal close/keyboard behavior, and update overlay lifecycle.
- Add local mock HTTP S3 and Cloudflare servers to verify signed requests, pagination, cancellation, error mapping, and SSRF boundaries without external credentials.
- Add a two-process Federation test with generated local TLS identities and controllable disconnects; current unit tests cannot prove reconnect or cross-version behavior.
- Add disposable systemd VM tests for install/upgrade rollback, Nginx reload rollback, mount recovery and backup restore.
- Add full-chain migration interruption tests using database copies and failure injection; preserve the original DB for every destructive trial.
- Measure test coverage by module and critical-flow assertions before setting a numeric coverage threshold; current setup has no coverage tool or stable baseline.

## Phase result

Unit and offline suites are green. Coverage is broad at the helper and module level but does not establish production lifecycle correctness. The missing network, browser, systemd, and multi-panel E2E checks are explicitly open and are not represented as passed.
