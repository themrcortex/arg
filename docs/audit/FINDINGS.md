# Audit findings index

This is a running index for completed phases only. It is not a statement that the entire audit is complete. The audit branch is isolated from `development` and `main`.

## P1 / High

| ID | Finding | Status | Resolution | Details |
|---|---|---|---|---|
| AUTH-001 | API keys survived owner demotion/disable | CONFIRMED | Fixed in `dbce1df` | [02-auth-security.md](02-auth-security.md) |
| PRIV-001 | Any process with UID `arg`, including Laravel workers, could invoke root helper | CONFIRMED | Fixed in `d2f5f19` | [03-privileged-helper.md](03-privileged-helper.md) |
| S3-001 | S3 connection test could overwrite a user's existing `arg-verify` object | CONFIRMED | Fixed in `52f224c` | [06-files-and-s3.md](06-files-and-s3.md) |
| S3-004 | S3 endpoint DNS validation is not pinned to the later request connection | AUDIT_REQUIRED | Open; needs a transport connector with IP pinning and TLS hostname preservation | [06-files-and-s3.md](06-files-and-s3.md) |
| FILE-002 | Privileged file operations may be redirected by a concurrent path/symlink race | AUDIT_REQUIRED | Open; descriptor-relative traversal and race tests required | [06-files-and-s3.md](06-files-and-s3.md) |

## P2 / Medium

| ID | Finding | Status | Resolution | Details |
|---|---|---|---|---|
| ARG-001 | Nginx domain-check GET incorrectly required Origin | CONFIRMED | Fixed in `dfbda3c` | [01-known-findings.md](01-known-findings.md) |
| ARG-002 | Stale root-ticket cookie changed normal Federation terminal scope | CONFIRMED | Fixed in `c9e3ffb` | [01-known-findings.md](01-known-findings.md) |
| ARG-003 | Upload request length and file-size message disagreed | CONFIRMED | Fixed in `c30dfb8` | [01-known-findings.md](01-known-findings.md) |
| ARG-004 | Hidden files consumed app-tree visible-entry cap | CONFIRMED | Fixed in `44812e6` | [01-known-findings.md](01-known-findings.md) |
| ARG-005 | Disk scan lacked direct Origin guard and its own concurrency gate | CONFIRMED (missing defense-in-depth) | Fixed in `cce425a` | [01-known-findings.md](01-known-findings.md) |
| AUTH-002 | API responses did not uniformly disable caching | CONFIRMED | Fixed in `d10c425` | [02-auth-security.md](02-auth-security.md) |
| AUTH-003 | Username-only login throttle allowed cross-IP account lockout | CONFIRMED | Fixed in `c310ce8` | [02-auth-security.md](02-auth-security.md) |
| AUTH-004 | App-volume scan POST did not verify Origin | CONFIRMED | Fixed in `2ee2f4a` | [02-auth-security.md](02-auth-security.md) |
| PRIV-002 | Helper accepted unbounded connection tasks and unbounded-time reads | CONFIRMED | Fixed in `d2f5f19`; response cap in `52824d0` | [03-privileged-helper.md](03-privileged-helper.md) |
| PRIV-003 | Deep disk scan retained all paths before truncating output | CONFIRMED | Fixed in `512ba90` | [03-privileged-helper.md](03-privileged-helper.md) |
| FED-001 | Private endpoint opt-in allowed cloud metadata targets and CGNAT classification was incomplete | CONFIRMED | Fixed in `6852cdd` | [04-federation.md](04-federation.md) |
| DEPLOY-001 | Cancelling an active deployment did not stop Git/build work | CONFIRMED | Fixed in `14aa314` | [05-deployment.md](05-deployment.md) |
| DEPLOY-002 | Concurrent ZIP uploads could consume system disk reserve | CONFIRMED | Fixed in `d848967` | [05-deployment.md](05-deployment.md) |
| S3-002 | Concurrent S3 transfers could exhaust disk and leave temporary files | CONFIRMED | Fixed in `52f224c` | [06-files-and-s3.md](06-files-and-s3.md) |
| S3-003 | S3 usage scans could paginate/queue without a finite bound | CONFIRMED | Fixed in `52f224c` | [06-files-and-s3.md](06-files-and-s3.md) |
| FILE-001 | ZIP archive creation had unbounded recursive directory depth | CONFIRMED | Fixed in `bd2e973` | [06-files-and-s3.md](06-files-and-s3.md) |
| DNS-001 | Cloudflare DNS UI/API fetched only the first 100 records | CONFIRMED | Fixed in `f423309`, stale request follow-up in `0fdc55d` | [07-nginx-ssl-dns.md](07-nginx-ssl-dns.md) |
| DNS-002 | CAA presentation strings did not escape quote/backslash characters | CONFIRMED | Fixed in `f423309` | [07-nginx-ssl-dns.md](07-nginx-ssl-dns.md) |
| NGINX-001 | Domain conflict scan skipped included symlinks and silently ignored oversized configs | CONFIRMED | Fixed in `40ab081`, same-name follow-up in `df57120` | [07-nginx-ssl-dns.md](07-nginx-ssl-dns.md) |
| NGINX-002 | SSL issuance accepted a domain that another server block might also claim | LIKELY | Added a preflight conflict check; live Certbot behavior still needs a disposable-host test | [07-nginx-ssl-dns.md](07-nginx-ssl-dns.md) |
| DB-001 | Root password reset could return failure after partially changing account state | CONFIRMED | Fixed atomically in `f8539e3` | [08-database.md](08-database.md) |
| DB-002 | Database row/query errors were hidden as partial or zero-valued API data | CONFIRMED | Fixed in workspace, monitoring, network, S3, trust-key, Federation, doctor and related list readers (`b31c9b7`, `6a98549`) | [08-database.md](08-database.md) |
| DB-003 | Startup migrations lack a central version ledger and full-sequence rollback test | AUDIT_REQUIRED | Open; failure-injection and historical-schema rollback compatibility remain unverified | [08-database.md](08-database.md) |
| PERF-002 | One synchronous SQLite connection serializes all API database access | AUDIT_REQUIRED | Open; lock-wait and representative request latency measurements are unavailable | [10-performance.md](10-performance.md) |
| PERF-003 | Deployment queue has no global admission cap across distinct applications | AUDIT_REQUIRED | Open; expected product capacity policy is unspecified | [10-performance.md](10-performance.md) |
| UI-001 | Older async responses could replace dashboard, workspace, Job-detail, or S3 object-browser state | CONFIRMED | Fixed with request epochs and response-order regression tests in `2d26692` | [09-frontend.md](09-frontend.md) |
| UI-002 | Deployment status polling could start overlapping reads when the endpoint exceeded the poll interval | CONFIRMED | Fixed with a tested single-flight gate, timeout, and abort on cleanup in `2d26692` | [09-frontend.md](09-frontend.md) |
| UI-003 | Shared API wrapper has no route-class timeout policy | AUDIT_REQUIRED | Global change deferred; long streams and job operations need different deadlines | [09-frontend.md](09-frontend.md) |
| UI-004 | Embedded production frontend did not match the frontend source | CONFIRMED | Rebuilt and committed `frontend/dist` from clean locked dependencies (`47c5a82`) | [13-test-coverage.md](13-test-coverage.md) |
| PERF-001 | Deployment SSE revision stopped changing when the retained log count reached 2,000 | CONFIRMED | Fixed in `e4d128e`; latest log ID now advances the revision and unchanged heartbeats avoid full Job reads | [10-performance.md](10-performance.md) |
| SUPPLY-001 | Locked ZIP release had RustSec advisories in built-in extraction APIs | CONFIRMED | Upgraded to `zip 9.1.3`; custom extraction regression covers traversal and symlinks (`adf5f78`) | [12-supply-chain.md](12-supply-chain.md) |
| SUPPLY-002 | Installer/updater do not cryptographically verify source or artifact provenance | AUDIT_REQUIRED | Open; select release trust root and implement signature/attestation verification | [12-supply-chain.md](12-supply-chain.md) |

## P3 / Low

| ID | Finding | Status | Resolution | Details |
|---|---|---|---|---|
| FED-002 | Repeated workspace-session renewal can create unbounded valid session rows | AUDIT_REQUIRED | Open; retry-safe idempotency design needed | [04-federation.md](04-federation.md) |
| CQ-001 | Strict Clippy gate fails on repository-wide lint debt | CONFIRMED | P3 maintainability debt; 50 unique diagnostics remain, no mass autofix applied | [11-code-quality.md](11-code-quality.md) |

## Not an issue

| ID | Observation | Result | Evidence |
|---|---|---|---|
| ARG-006 | Private-network UI/API reports a draft with no active enforcement | NOT_AN_ISSUE | [01-known-findings.md](01-known-findings.md) |
| SUPPLY-003 | Secret scanner flagged SQL and permission-scope strings as generic keys | NOT_AN_ISSUE | Confirmed false positives; only exact historic fingerprints are ignored in `.gitleaksignore`, and the full-history scan is clean | [14-ci-quality-gates.md](14-ci-quality-gates.md) |

## Open audit coverage

Phases 0–15 have been reviewed and documented. The final disposition and release recommendation are in [15-release-audit.md](15-release-audit.md) and [FINAL-REPORT.md](FINAL-REPORT.md). Two P1 items remain `AUDIT_REQUIRED`; they are not represented as proven exploits or cleared findings.
