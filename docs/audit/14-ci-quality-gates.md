# 14 — CI/CD and quality gates

## Scope and revision

- Source baseline: `development` at `eec273cde037fa57edc1b5ab042fda7acc26a32c` (`0.7.3-dev.1`).
- Reviewed all workflow files, action references and permissions, dependency audit triggers, script-test inventory, embedded frontend verification, and local gate results.
- Phase commits: `7c8d0a8854ea84412899c39c07a2b339a585a34c`, `e8d56d8aae6cbeb536082eb68b1703848ee584e9`, `c96ba33b492a48d782f9887e95c71fca7d730581`, `1d0932e7b49bc718a94f6c312457161359815983`, and `fb8aa62403762b467913523e712936e8a90bcaec` (frontend offline tests and secret scanning).

## CI coverage

The main workflow now enforces these checks on pushes to `main`/`development`, pull requests, and manual dispatch:

- Rust `cargo test --locked`, `cargo check --locked`, and optimized release build.
- Frontend `npm ci`, TypeScript check, offline Node tests, Vite build, and synchronization check against the frontend embedded into the Rust binary.
- Documentation `npm ci`, type/check, build, and docs-link/content test.
- Installer, updater, UFW, public-IP/ACME retry, and workflow security shell tests.
- A separate full-history Gitleaks scan. It downloads the pinned `8.30.1` Linux binary into a temporary directory, checks its hard-coded SHA-256 before execution, enables full redaction, and removes the temporary files on exit. The secret-scan checkout disables persisted credentials.

The dependency audit workflow runs RustSec and npm advisory checks for lockfile changes, pull requests, weekly schedule, and manual dispatch. It uses read-only workflow permissions.

Actions are pinned to complete commit SHAs; Dependabot is configured for Cargo, both npm lockfiles, and GitHub Actions. Pages deployment write permissions are scoped to the deploy job. The workflow regression script verifies action pinning, checkout credential persistence, and Pages permission scopes.

## Secret-scan findings and suppression policy

- The baseline full-history scan identified three `generic-api-key` false positives: one SQL statement in `src/ssh_keys.rs` and two Federation scope-name lists in `src/federation.rs`. Sanitized source-shape review confirmed these are query/scope strings, not credential values.
- `.gitleaksignore` suppresses only the three exact commit/path/rule/line fingerprints. It does not allowlist either source file or disable the generic API-key rule for future changes.
- Gitleaks `8.30.1` was downloaded from the official release URL and its SHA-256 matched the GitHub release API digest. The report was inspected through metadata only; secret/match fields were never printed.
- Re-running the full-history scan after adding the narrow ignore file scanned 310 commits and reported no remaining findings.

## Gates that remain non-green

- `cargo fmt --all -- --check` fails on formatting drift across thousands of lines in the existing repository. No mass reformat was run because it would bury security-related diffs. It is not enabled as a required gate yet.
- `cargo clippy --locked --all-targets -- -D warnings` fails with 48 production-target and 50 test-target diagnostics. Most are complexity/style findings (for example argument count, type complexity, and simplifiable expressions); no broad lint suppression or automatic rewrite was added. CQ-001 remains open.
- A release build in CI validates compilation but does not create a signed binary, provenance attestation, or signed source tag. See SUPPLY-002 in [12-supply-chain.md](12-supply-chain.md).
- CI uses GitHub-hosted Linux and has no clean Debian/Ubuntu systemd VM, real Nginx/ACME service, S3 provider, browser, or two-node Federation fixture.

## Verification

- `bash scripts/tests/test-secret-scan.sh` — passed; checksum verified; 310 commits scanned; no findings after exact-fingerprint exclusions.
- `bash scripts/tests/test-workflow-security.sh` — passed.
- Workflow and Dependabot YAML parse — passed.
- `frontend/npm run test:offline` — passed, 5 tests.
- `npm run check` and `npm run build` in frontend — passed; bundle-size warning recorded in [13-test-coverage.md](13-test-coverage.md).
- `cargo build --locked --release -j 2` — passed in 3m13s in this workspace.
- `scripts/verify-embedded-frontend.sh` — passed after the generated bundle was refreshed.
- All shell test scripts — passed.
- Strict formatting and Clippy gates — fail as described; no failure was hidden with `continue-on-error` or `|| true`.

## Phase result

CI now checks offline frontend behavior and scans full Git history for secrets using a checksum-pinned tool. Known baseline formatter/linter debt and the missing release provenance policy remain open and visible.
