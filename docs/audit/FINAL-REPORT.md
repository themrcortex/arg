# ARG security, bug, and code-quality audit — final report

> **Current follow-up:** The 2026-10-29 risk recheck below supersedes earlier unresolved S3 DNS-pinning and file-mutation TOCTOU statements in this historical report. See [16-risk-recheck.md](16-risk-recheck.md) for current source SHA, test evidence, and limitations.

## 2026-10-29 recheck addendum

The latest source review started from `c9e87ceb33595299d4978cd31f67058627fdd127` on `audit/development-risk-remediation`. The following remaining findings were reproduced and addressed or reduced:

- **Fixed:** S3 custom endpoints now connect to their exact validated DNS answers through an authenticated loopback CONNECT proxy, preserve TLS hostname verification, and bound DNS answer count and connection time. Explicit private-endpoint opt-in rejects loopback, link-local, metadata, CGNAT, and reserved addresses. Local proxy and address-policy regression tests cover the change.
- **Fixed:** privileged file mutations, copy/move, edit, rename, delete, ZIP creation and extraction now use anchored directory descriptors and no-follow descriptor-relative operations to close the identified path-swap race.
- **Mitigated:** file copy and archive operations preserve a 64 MiB destination free-space reserve.
- **Fixed:** workspace-session renewal retries are idempotent, recover missing/expired session rows, and remove superseded grants.
- **Fixed:** deployment admission is capped at 64 queued jobs and rejects overflow without leaving partial records.
- **Fixed in shared JSON API wrapper:** frontend requests have bounded route-class deadlines and caller abort propagation.
- **Partially mitigated:** migration recovery has module-boundary failure injection and repeated restart coverage; no central schema-version ledger was introduced.

No confirmed P1 finding remains open in the source paths rechecked. The remaining items are not safely closeable from this workspace alone: signed updater provenance needs a maintainer-selected trust root; database contention needs representative VPS measurements; Nginx/Certbot behavior needs a disposable VM; strict Clippy/format gates have repository-wide pre-existing debt. No live server, live S3 provider, ACME endpoint, or remote Node was accessed.

Current verification: 257 Rust tests pass; `cargo check --locked --all-targets`, TypeScript check, 10 offline frontend tests, and frontend production build pass. Strict Clippy and whole-repository rustfmt checks remain red as detailed in [16-risk-recheck.md](16-risk-recheck.md). The rebuilt `frontend/dist` must be committed with its source changes because ARG embeds it in the Rust binary.

## Executive summary

The `development` branch was audited from baseline commit `eec273cde037fa57edc1b5ab042fda7acc26a32c` (`0.7.3-dev.1`) on the isolated `audit/development-hardening` branch. Phases 0–15 have evidence-based reports in this directory.

The work fixed confirmed authorization, CSRF, request-boundary, helper, file/ZIP, deployment cancellation/disk-pressure, S3 transfer, DNS/Nginx, database error handling, frontend stale-state, deployment progress, dependency, and embedded-frontend defects. It also added SHA-pinned CI actions, dependency advisory scans, secret scanning, and regression tests.

No confirmed P0 remains. Three confirmed P1 findings were fixed. Two P1 security-sensitive paths remain `AUDIT_REQUIRED`: S3 DNS is not pinned to the later SDK connection, and privileged file mutations have a possible path/symlink TOCTOU window. They were not reproduced as exploits and are not being called confirmed vulnerabilities, but the current evidence is insufficient to sign them off. System-level tests on supported Debian/Ubuntu VMs and live-provider/browser/multi-node tests were unavailable. The branch is therefore **not cleared for an unqualified production security release**.

All changes are committed locally on `audit/development-hardening`. Nothing was pushed, and `development`/`main` were not modified. No version bump or tag was made.

## Findings summary

The findings index contains 39 findings and two observations marked `NOT_AN_ISSUE`:

| Severity / disposition | Count |
|---|---:|
| P0 confirmed | 0 |
| P1 confirmed and fixed | 3 |
| P1 audit required | 2 |
| P2 confirmed and fixed | 26 |
| P2 likely, mitigated pending VM test | 1 |
| P2 audit required | 5 |
| P3 audit required | 1 |
| P3 confirmed, still open (strict Clippy debt) | 1 |
| `NOT_AN_ISSUE` observations | 2 |

Thus 29 confirmed findings have fixes committed. One likely P2 issue has a code mitigation and still needs runtime verification. Eight `AUDIT_REQUIRED` findings and one confirmed P3 quality-gate issue remain open. Full details, status and evidence: [FINDINGS.md](FINDINGS.md) and [REMEDIATION-STATUS.md](REMEDIATION-STATUS.md).

## Important implemented changes

- API keys are invalidated when an owner is disabled or loses administrator access; login throttling is scoped to client address; sensitive API responses are not cacheable; selected write endpoints now enforce Origin validation.
- The privileged helper checks its caller's systemd cgroup, applies request/response and concurrency limits, and returns bounded scan results.
- Deployment cancellation terminates active Git/build work; ZIP upload admission preserves disk reserve; the SSE revision advances as retained logs change.
- S3 verification no longer overwrites unrelated `arg-verify` data; transfer and usage scans have bounded admission, duration, object, disk, and cleanup rules. DNS pinning remains an open P1 audit item.
- ZIP archive creation/extraction has traversal, symlink, special-file, size, count, and depth protections. The `zip` crate moved from vulnerable 2.6.1 to patched 9.1.3 with a regression test.
- Database write atomicity and row/query error propagation were fixed in selected critical paths. No schema migration was required by this audit.
- Nginx/DNS conflict scanning, Cloudflare pagination, CAA display escaping, and stale frontend response handling were fixed and tested.
- The stale checked-in `frontend/dist` bundle was rebuilt from `npm ci` output and is now synchronized with source. This avoids publishing an older embedded UI than the reviewed frontend.
- CI now verifies offline frontend tests, embedded bundle consistency, action SHA pinning and permissions, Rust/npm dependency advisories, and full-history secrets using a checksum-verified, redacting Gitleaks binary.

## Test and build results

### Passed

- `cargo test --locked --all-targets` — 246 passed, 0 failed on final source.
- `cargo check --locked --all-targets` — passed.
- `cargo build --locked --release -j 2` — passed in 3m13s in this workspace. This timing is not representative of a production VPS.
- `npm ci`, `npm run check`, `npm run test:offline` — passed; 5 frontend tests passed.
- `npm run build` — passed; production frontend bundle was refreshed and `scripts/verify-embedded-frontend.sh` passed.
- `cargo audit` — exit 0, no vulnerable packages. The RustSec database loaded; crates.io index refresh was unavailable in this environment.
- `npm audit --audit-level=moderate` in `frontend/` and `docs-site/` — passed with zero advisories.
- All `scripts/tests/test-*.sh`, `bash -n` for installer/updater/helper scripts, workflow YAML parse, docs-content tests, workflow security tests, and Gitleaks scan — passed.
- Gitleaks 8.30.1 release checksum verified; 312 commits scanned; no findings remained after exact-fingerprint exclusions for three reviewed SQL/scope false positives. Secret values were not printed.
- `git diff --check` — passed for the audit changes.

### Failed or unavailable

- `cargo fmt --all -- --check` fails due broad pre-existing formatting drift across many Rust modules. No mass format was applied.
- `cargo clippy --locked --all-targets -- -D warnings` fails with 50 unique primary diagnostics (48 production-target and 50 test-target reports, including test-only diagnostics). No blanket suppression was applied.
- Vite emitted a bundle-size warning for the 504.49 kB minified entry chunk; this is a build warning, not a failed build.
- No Debian/Ubuntu VM, live Nginx/ACME operation, external S3 provider, browser E2E/accessibility device, load benchmark, full migration failure-injection environment, or two-panel Federation setup was available.

## Performance and architecture

- Deployment SSE now performs a small state/revision read on heartbeat and loads full job details only when state, steps, or latest log ID changes; the fixed 2,000-log retention cap no longer stalls streamed logs.
- Existing one-connection SQLite serialization and cross-application deployment queue admission were measured only by source review; without representative workload and capacity policy, no risky architecture rewrite was made.
- No broad backend rewrite or database migration was introduced.
- Frontend production bundle size is recorded in [13-test-coverage.md](13-test-coverage.md); chunk splitting or terminal lazy-loading remains a performance follow-up.

## Remaining risks

1. **P1 S3-004:** SDK connection may independently resolve DNS after endpoint validation; implement/test IP-pinned transport while preserving TLS hostname validation.
2. **P1 FILE-002:** path validation followed by a later privileged path operation leaves a possible concurrent symlink race; use descriptor-relative operations and race tests.
3. **P2 SUPPLY-002:** installer/updater do not verify independently signed source commits or build artifacts; select a signing trust root and rotation/recovery policy.
4. **P2 DB-003:** full migration chain lacks interruption/failure-injection proof.
5. **P2 PERF-002/PERF-003:** SQLite lock contention and global deployment queue policy need measurement and product capacity decisions.
6. **P2 UI-003:** shared API wrapper lacks request class deadlines.
7. **P3 FED-002:** workspace renewal may create duplicate session records.
8. **P3 CQ-001:** strict Clippy and full Rustfmt check remain red.
9. **P2 NGINX-002:** automated domain conflict guard is in place; Certbot selection/rollback needs a disposable-host integration test.
10. VM install/upgrade rollback, browser accessibility, real provider, and multi-node integration remain untested.

## Release readiness

**Status: Not cleared for unqualified production security release.** The automated source/build gates pass, and no P0 was confirmed, but two P1 security boundaries are unresolved and no host-level verification was possible. Do not describe this audit as proving complete security. Follow the ordered recommendations in [15-release-audit.md](15-release-audit.md) before release or record explicit maintainer acceptance of residual risks.

## Commits

The following commits were created after the required baseline, newest first. The final-report commit is the current audit HEAD and is identified in the completion message.

```text
4ec7a84 docs(audit): record CI and secret scanning gates
fb8aa62 ci: add frontend regression and secret scanning gates
61bd296 docs(audit): record test coverage and bundle drift
47c5a82 fix(frontend): refresh embedded production bundle
38d5a3e docs(audit): record dependency and supply-chain review
1d0932e ci: pin remaining checkout action
adf5f78 fix(deps): upgrade zip past extraction advisories
c96ba33 ci(security): avoid persisting checkout credentials
e8d56d8 ci(security): audit Cargo and npm advisories
7c8d0a8 ci(security): pin actions and scope Pages permissions
fe589a7 docs(audit): record phase 11 code quality review
e7d8d6c refactor(deploy): simplify event stream option handling
6a5d289 docs(audit): record phase 10 performance review
e4d128e fix(deploy): advance SSE revisions past log retention
cea99e6 docs(audit): record frontend review and race fixes
2d26692d fix(frontend): ignore stale async page responses
911fd28 docs(audit): record SQLite and migration review
6a98549 fix(db): propagate SQLite read failures
b31c9b7 fix(db): propagate workspace query decode errors
f8539e3 fix(db): make root password reset atomic
ca1273c docs(audit): record nginx ssl and dns review
0fdc55d fix(dns): discard stale record page responses
df57120 fix(nginx): scan same-named conf fragments
40ab081 fix(nginx): reject ambiguous SSL domain ownership
f423309 fix(dns): paginate records and escape CAA values
2466500 docs(audit): record file and S3 review
52f224c fix(s3): protect marker data and bound transfers
bd2e973 fix(files): cap ZIP archive directory recursion
0a1f2a6 docs(audit): record deployment review
d848967 fix(deploy): reserve disk space for archive uploads
14aa314 fix(deploy): stop active commands when jobs are cancelled
bbfc35b docs(audit): record federation review
6852cdd fix(federation): block cloud metadata endpoints
0f412f0 docs(audit): record privileged helper findings
52824d0 fix(security): bound privileged socket responses
512ba90 fix(files): bound disk scan result memory
d2f5f19 fix(security): bind privileged helper to panel service
eb7a692 docs(audit): complete authentication and API access matrix
2ee2f4a fix(security): check origin before application volume scans
c310ce8 fix(auth): scope login throttles to client address
d10c425 fix(security): prevent caching API responses
dbce1df fix(auth): revoke API keys when owner loses admin access
d3c8c3d docs(audit): record rechecked known findings
cce425a fix(files): protect disk scans from cross-origin bursts
44812e6 fix(apps): filter hidden files before applying tree limit
c30dfb8 fix(files): align base64 request and file size limits
c9e3ffb fix(federation): scope terminal sessions by requested mode
dfbda3c fix(nginx): allow same-origin domain check GET requests
ef006ff docs(audit): record development baseline
```

## Suggested next release work

1. Resolve S3-004 and FILE-002 with code and regression tests.
2. Add disposable Debian/Ubuntu install/upgrade/systemd tests and S3/ACME/two-node fixtures.
3. Decide and implement signed source/artifact provenance.
4. Reduce formatter/Clippy debt in small reviewed commits before enabling those gates.
5. Split the largest frontend chunks if measurements show slow initial load on target VPS browsers.
