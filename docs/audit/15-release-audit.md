# 15 — Final audit and release disposition

## Revision and scope

- Audited branch: `audit/development-hardening`.
- Required source baseline: `development` at `eec273cde037fa57edc1b5ab042fda7acc26a32c`, version `0.7.3-dev.1`.
- Current audit source includes the fixes and test gates listed in [FINAL-REPORT.md](FINAL-REPORT.md). No production server, user database, real S3 bucket, public domain, or external Node was accessed.
- Only the two known untracked workspace directories `.arg-audit-target/` and `generated_images/` remain; they pre-existed this audit and were not modified or removed.

## Final acceptance review

| Criterion | Result | Evidence / limitation |
|---|---|---|
| P0 confirmed issues | None found | This is a scoped audit, not proof that no P0 exists outside reviewed paths. |
| P1 confirmed findings | Three fixed | API key owner eligibility, privileged helper caller identity, and S3 verification marker overwrite have regression tests. |
| P1 items requiring more audit | Two open | S3 DNS pinning and privileged file path TOCTOU are `AUDIT_REQUIRED`; neither was exploited or declared safe. |
| Core Rust tests | Pass | 246 tests on final source, `cargo test --locked --all-targets`. |
| Rust compile | Pass | `cargo check --locked --all-targets` and optimized release build passed after ZIP dependency change and bundle refresh. |
| Frontend checks | Pass | TypeScript check, 5 offline tests, production build, and embedded-bundle consistency check. |
| Dependency audit | Pass with environment note | Cargo audit returned zero advisories; it could not refresh the crates.io index in this environment. npm audit in frontend and docs site returned zero moderate-or-higher advisories. |
| Secret scan | Pass | Gitleaks 8.30.1 checksum verified, 312 commits scanned, no remaining findings after exact-fingerprint suppression of three documented false positives. |
| Formatting and strict Clippy | Fail / known debt | Rustfmt reports broad pre-existing drift; `clippy -D warnings` reports 50 unique diagnostics. These are not hidden and are not passing gates. |
| Runtime acceptance | Not verified | No Debian/Ubuntu VM, systemd boot/restart, real Nginx/ACME, live S3 provider, browser E2E, or two-node Federation test was available. |

## Open risk disposition

The open items below prevent an unqualified production security sign-off. They require follow-up engineering or explicit maintainer risk acceptance; this audit does not silently close them.

1. **S3-004 / P1 / AUDIT_REQUIRED:** endpoint DNS is checked before constructing a client, but the S3 HTTP client can resolve the endpoint independently when it connects. DNS rebinding between those steps has not been eliminated. Implement an IP-pinned transport that preserves TLS hostname verification and test it with a local DNS/HTTP fixture before claiming SSRF resistance.
2. **FILE-002 / P1 / AUDIT_REQUIRED:** privileged filesystem operations validate paths before later operations that follow paths again. A concurrent symlink/path replacement could create a TOCTOU window. Use descriptor-relative traversal (or an equivalent kernel-enforced beneath/no-symlink policy) and race tests before claiming the path boundary is race-safe.
3. **SUPPLY-002 / P2 / AUDIT_REQUIRED:** installer/updater trust GitHub HTTPS and mutable branch references; there is no separately trusted signed commit/tag, checksum, or build attestation. A release signing trust root and rotation/recovery process need a maintainer decision.
4. **DB-003 / P2 / AUDIT_REQUIRED:** migration functions are largely additive/idempotent, but full startup-chain interruption and restore behavior lacks failure-injection proof.
5. **PERF-002 / P2 / AUDIT_REQUIRED:** one synchronous SQLite connection under a global mutex serializes access; no representative VPS lock-wait or latency benchmark was available.
6. **PERF-003 / P2 / AUDIT_REQUIRED:** deployment work is limited per application but the global queue has no admission cap across distinct applications; required product capacity policy is unspecified.
7. **UI-003 / P2 / AUDIT_REQUIRED:** no route-class request deadline policy exists for the shared API wrapper; long streams and durable job endpoints need distinct behavior.
8. **FED-002 / P3 / AUDIT_REQUIRED:** repeated workspace renewal can create additional session rows; retry-safe idempotency design is pending.
9. **CQ-001 / P3 / CONFIRMED:** strict Clippy remains red on repository lint debt; formatting check also fails broadly. No bulk reformat or blanket lint suppression was applied.
10. **NGINX-002 / P2 / LIKELY, mitigated:** preflight rejects known duplicate domain ownership, but actual Certbot/Nginx selection and rollback still need a disposable Debian/Ubuntu host test.

## Release recommendation

The branch is technically buildable and the automated local suites pass, but it is **not cleared for an unqualified production security release** while S3-004 and FILE-002 remain high-priority audit items and system-level acceptance remains untested. No version bump or release tag was made. The source baseline `development` and `main` were not changed, and this audit branch was not pushed.

Recommended release sequence:

1. Resolve or formally accept S3-004 and FILE-002 with evidence and regression tests.
2. Run the install/upgrade/rollback suite on disposable supported Debian and Ubuntu VMs.
3. Run isolated provider and two-node tests, then browser/accessibility checks.
4. Decide whether to make formatter and strict Clippy clean required gates; do not mass-reformat the current review diff.
5. Select a source/artifact signing policy before describing updates as independently authenticated.

## Phase result

All requested audit phases have an evidence-based record. The project has targeted fixes for confirmed bugs, stronger CI and a synchronized embedded frontend. Remaining uncertainty is explicit; the result is a hardening branch and audit package, not a claim that every runtime environment or every attack path is proven safe.
