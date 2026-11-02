# ARG Control Center — current-checkout review

Date: 2026-11-02 UTC
Repository: `themrcortex/arg`
Branch: `development`
Reviewed starting commit: `df23dca78033d1ffd991048d4d367f40cb76d51b`
Version at review start: `0.7.9-dev.7`
Production systems and user data were not contacted or modified.

This is a fresh, targeted review of the checkout above. Earlier `docs/audit/00`–`16` files describe older snapshots and are retained as historical evidence; their line numbers and test totals are not treated as current results.

## Architecture and operation map

ARG is a Rust/Axum binary with feature modules under `src/`, SQLite state accessed through the shared application state, and a React 19/TypeScript/Vite frontend under `frontend/src/`. `src/routes.rs` registers API routes. Privileged operations use the restricted helper request path and action allowlists in the Rust binary. Frontend routing is hash-based in `frontend/src/navigation.ts`; the shell is in `frontend/src/components/shell/`. Static assets are built into `frontend/dist/` and embedded by Rust, so frontend fixes must include a fresh production build before release.

Relevant paths followed for this review:

| Operation | UI | API / authorization | Privileged or persisted layer |
|---|---|---|---|
| S3 connection check | `frontend/src/S3StoragePage.tsx` | `POST /api/s3/connections/{id}/test`, same-origin and administrator checks in `src/s3/mod.rs::test_connection` | `s3-test` and `s3-list-buckets` helper actions; credentials are read from the root-only vault in `src/secret_vault.rs` |
| ARG update refresh | `frontend/src/App.tsx::forceFreshPanelReload` | update helper status is polled before reload | service worker and Cache Storage are browser-origin scoped |
| Nginx site action | `frontend/src/nginx/SiteActionsPanel.tsx` | per-site action API in `src/nginx.rs`; allowed operations are enumerated | privileged Nginx manager action; site inventory reports each site's enabled state |
| Activity log | `frontend/src/pages.tsx::AuditPage` | `GET /api/audit`, administrator-only in `src/main.rs::audit_log` | SQLite query currently returns the latest 100 rows |
| Fleet rollout | `frontend/src/FleetManagementPanels.tsx` | preview metadata endpoints in `src/fleet.rs` | no rollout execution in this version; UI correctly labels it preview-only |

The repository contains dedicated modules for applications/deployments, Nginx, DNS, S3, Docker, files, backups, databases, federation/nodes, updates, authentication, and the privileged helper. Existing `docs/audit/00`–`16` contain prior route, permission, helper, deployment, storage, database, and supply-chain work; this review does not reassert their historical checks as newly re-run.

## Review matrix

| ID | Severity / type / status | Evidence and behavior | Repair / verification |
|---|---|---|---|
| ARG-S3-001 | P1 · confirmed bug · fixed | `src/secret_vault.rs::read_key_file_contents` rejected every master key whose mode was not exactly `0600`; `get_or_create_key` previously published a newly created key at its final name before writing all 32 bytes. A restrictive root-owned key could be rejected, while interruption or concurrent reads could observe an incomplete key and break S3 credential reads. The key is the encryption root; silently replacing a malformed key would make existing ciphertext unrecoverable. | A valid root-owned 32-byte key is safely normalized to `0600`. New keys are fully written and synced to a temporary file and atomically published create-if-absent; racing callers read the winner. Invalid size/owner remains a hard error with recovery guidance; no key is regenerated. Regression tests cover concurrency, modes, wrong owner and corrupt length. |
| ARG-S3-002 | P2 · confirmed UX/data hygiene · fixed | `src/s3/mod.rs::test_bucket_connection` wrote/reused a fixed `arg-verify` object and intentionally left it in the user's bucket. The UI described this persistent artifact as normal. | Added separate read-only and confirmed read/write tests. Read/write uses a random, create-only object key, verifies content, then attempts deletion. If cleanup is denied/ambiguous, the exact key and possible persistence are reported. Existing objects are not overwritten. |
| ARG-S3-003 | P1 · confirmed workflow gap · fixed | S3 test path always attempted account-wide `ListBuckets`; absence of that optional permission could make a valid bucket-scoped credential appear degraded. | The test now treats account-wide listing as optional, reports it separately, and preserves a connected state when the selected bucket's required operations succeed. Regression tests cover this and the read-only path. |
| ARG-UI-001 | P2 · confirmed isolation issue · fixed | `frontend/src/App.tsx::forceFreshPanelReload` cleared every Cache Storage entry and unregistered every Service Worker for the entire origin after an ARG update. This could disrupt another app sharing that origin. | Cache cleanup is restricted to `arg-shell-*`; worker cleanup matches ARG's exact `/service-worker.js` URL. No unrelated origin cache or worker is targeted. |
| ARG-NGX-001 | P2 · confirmed stale-state UX · fixed | `SiteActionsPanel.run` fetched global Nginx status but rendered its `enabled` prop from the previously loaded site card, then predicted site state from the action name. The displayed service and site states could disagree or claim a change before reading it back. | The dialog now reads global service state and this site's enabled state independently before and after the action. It shows “unknown” if a read fails and does not predict a result. |
| ARG-AUDIT-001 | P2 · UX/scope · fixed in UI, backend gap remains | `src/main.rs::audit_log` returns only 100 events. The page filters locally; CSV previously exported all fetched rows even when filters were active. | CSV now exports the filtered visible subset and the page states the 100-event backend limit. Server-side date/actor filters, pagination, and complete-history export remain unsupported. |
| ARG-NAV-001 | P2 · UX improvement · implemented | Navigation categories were static and did not clearly separate websites, networking, storage, and ARG administration. | Re-grouped existing routes into the requested task-oriented sections. Routes and permission filtering are unchanged. Collapsible groups, workspace selector behavior, and favorites/recent navigation were not changed in this pass. |
| ARG-VERSION-001 | P2 · documentation defect · fixed | `README.md` named `v0.7.2` as stable while GitHub release metadata and `origin/main` identify `v0.7.8` as the stable release. | README stable version updated to `v0.7.8`; current development version remains a prerelease. |
| ARG-FLEET-001 | P3 · not an issue | `src/fleet.rs` and `frontend/src/FleetManagementPanels.tsx` describe rollout plans as preview/metadata and state that no remote install/build/restart occurs. | No execution feature added or claim changed. Keep fleet rollouts clearly preview-only until a real persisted executor and per-node results exist. |

## Design specification applied in this pass

The active dark palette was updated to **ARG Operations UI — Copper Precision** using shared CSS variables in `frontend/src/index.css`: background `#101820`, sidebar `#131F29`, surface `#1C2934`, elevated surface `#243340`, border `#344451`, text `#F4F6F8`, muted text `#92A2B1`, copper `#D4A373`, success `#39A985`, warning `#D9A441`, danger `#D66A70`, and info `#7F9BD2`. The existing light theme remains available with its own variables. Dark-page backgrounds now use the shared background token.

Sidebar groups now follow task categories: overview/health; websites/applications; infrastructure/network; storage/data; ARG administration. Existing icons, route IDs, access checks, collapsed mode, and RTL layout remain in place. This is a scoped foundation update, not a claim that all page-level styles have been migrated away from legacy hard-coded colors.

S3 actions distinguish a non-writing check from a write/read/delete test. Any leftover test object is exposed as a recovery detail, and the write test is confirmed before running. Nginx action state reads are independent from app/runtime state. Audit export communicates that it covers at most the current server-returned 100-event window.

## Verification record

Final local verification on the changed source:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked --all-targets` | Pass |
| `cargo test --locked --all-targets` | Pass, 320 tests |
| `cargo test --locked s3::tests -- --nocapture` | Pass, 28 tests |
| `cargo test --locked secret_vault::tests -- --nocapture` | Pass, 8 tests |
| `cd frontend && npm run check` | Pass |
| `cd frontend && npm run test:offline` | Pass, 25 tests |
| `cd frontend && npm run build` | Pass; Vite reports the pre-existing large-chunk advisory |
| `cd docs-site && npm run check && npm run build` | Pass; unresolved `/arg/fonts/...` references are retained for runtime resolution |
| `bash scripts/tests/test-docs-content.sh` | Pass: 32 canonical pages, 64 localized articles, internal links and legacy prefixes |
| installer/update/release/UFW/workflow shell test suite | Pass |
| `bash scripts/tests/test-secret-scan.sh` | Pass; 399 commits scanned, no leaks |
| `cargo clippy --locked --all-targets -- -D warnings` | Blocked: the local stable toolchain does not have the `clippy` component installed; no Clippy pass is claimed |

The production-asset consistency check is run by CI after commit; before commit its intended behavior is to reject the locally changed `frontend/dist`. GitHub Actions will also run the locked release build. No live provider or browser automation was available for this pass.

## Remaining review and release limits

No live S3 provider credentials were available, so AWS/R2/ArvanCloud/ParsPack/MinIO network behavior and provider-specific delete policies were not exercised. A key with the wrong length or owner cannot safely be reconstructed; restore the matching `/etc/arg/s3/.master-key` from a trusted backup, as encrypted credentials depend on it. The database/API still lacks audit pagination and server-side filters. The full module-by-module security and responsive visual review requested in the larger audit remains broader than the findings revalidated here; earlier audit documents should be revisited against the release SHA before being treated as current. This review adds no schema migration and does not alter any production VPS.
