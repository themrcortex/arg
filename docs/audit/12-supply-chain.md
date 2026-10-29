# 12 — Dependencies, installer, updater, and software supply chain

## Scope and revision

- Source baseline: `development` at `eec273cde037fa57edc1b5ab042fda7acc26a32c` (`0.7.3-dev.1`).
- Reviewed Cargo/npm lockfiles, installer and update source acquisition, GitHub Actions, permissions, and installer/update shell regression tests.
- Audit branch: `audit/development-hardening`.
- Phase commits: `7c8d0a8854ea84412899c39c07a2b339a585a34c`, `e8d56d8aae6cbeb536082eb68b1703848ee584e9`, and `c96ba33b492a48d782f9887e95c71fca7d730581` (workflow pinning, advisory scans, credential persistence); `adf5f7870663512a9e845b70d065dba3bde69cee` (ZIP dependency and extraction regression); `1d0932e7b49bc718a94f6c312457161359815983` (pin the remaining checkout action).

## Findings

### SUPPLY-001 — ZIP crate version was covered by extraction advisories

- **ID:** SUPPLY-001
- **Title:** The locked ZIP crate release had advisories for unsafe extraction APIs.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High; RustSec advisory database matched the locked dependency.
- **Module:** Rust dependencies and archive ingestion
- **File:** `Cargo.toml`, `Cargo.lock`, `src/deploy.rs`
- **Function:** `extract_archive`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`; fix `adf5f7870663512a9e845b70d065dba3bde69cee`.
- **Affected behavior:** The lockfile contained `zip 2.6.1`, for which RustSec reported RUSTSEC-2026-0367, RUSTSEC-2026-0368, and RUSTSEC-2026-0369 concerning permission handling and symlink escapes in built-in extraction.
- **Technical evidence:** ARG's deployment code does not call `ZipArchive::extract`; it iterates entries, checks `enclosed_name`, rejects symlinks and special file types, bounds expanded size, creates parent directories, and writes regular files. The vulnerable package was still present in the dependency graph and its safe version range was `>=9.0.0`.
- **Safe reproduction:** Run `cargo audit` against the baseline lockfile; the three advisories are reported. Static search for `ZipArchive::extract` confirms ARG uses its own entry-by-entry extraction path.
- **Root cause:** The ZIP dependency declaration allowed major version 2 and Cargo.lock retained its latest compatible release without a recurring advisory gate.
- **Security/functional impact:** A vulnerable library API was available in the binary dependency graph, but the audited ARG extraction path did not invoke that API. No exploit of ARG's current archive path was demonstrated; presence of the affected dependency still warranted removal of the known vulnerable implementation.
- **Proposed fix:** Move to a patched ZIP release, retain ARG's explicit safe extraction checks, and keep archive regression coverage.
- **Implemented change:** Updated `zip` to `9.1.3` and regenerated the lockfile. Extended `zip_extraction_rejects_traversal_and_tree_copy_excludes_secrets` with a crafted ZIP symlink entry, which must be rejected before extraction.
- **Tests added:** The extended test rejects both traversal and symlink entries; full Rust suite: 246 passed. `cargo check --locked --all-targets` passed. `cargo audit` exited 0 with no vulnerable packages.
- **Regression risks:** The ZIP crate major-version update changes transitive compression dependencies. ARG's ZIP upload, extraction, and creation tests passed; provider-specific or unusually encoded third-party archives were not tested against a live deployment.
- **Commit:** `adf5f7870663512a9e845b70d065dba3bde69cee`
- **Remaining risk:** Continue using ARG's custom extraction path; do not replace it with the crate's convenience extraction API without a separate security review.

### SUPPLY-002 — Release checksums do not independently authenticate provenance

- **ID:** SUPPLY-002
- **Title:** Source downloads and prebuilt assets rely on HTTPS and GitHub without independent signature or artifact attestation verification.
- **Severity:** P2 / Medium
- **Status:** AUDIT_REQUIRED
- **Confidence:** High for the observed acquisition path; impact and acceptable release policy require maintainer decision.
- **Module:** Installer, updater, release process
- **File:** `install.sh`, `scripts/update-helper.sh`, `scripts/release.sh`
- **Function:** `prepare_source`, update selection and source fetch paths
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`.
- **Affected behavior:** A successful compromise of the GitHub account/repository or the trusted HTTPS path could supply code that the installer later builds or installs with service privileges.
- **Technical evidence:** `install.sh` clones/fetches the selected `main` or `development` branch over HTTPS and resets to `FETCH_HEAD`. Release mode verifies a SHA-256 sidecar, ELF architecture, and selected asset/tag version before atomic installation. The release workflow separately checks the executable's `--version` before publishing. The checksum sidecar is published in the same GitHub Release and is not an independent signing trust root. The rustup bootstrap is downloaded over HTTPS without a pinned checksum.
- **Safe reproduction:** Read-only source review of `install.sh:prepare_source`, `scripts/update-helper.sh:check`, and the release tooling. No production or remote host was contacted by this audit.
- **Root cause:** The release trust model currently treats GitHub TLS and repository access control as the provenance boundary.
- **Security/functional impact:** This is a supply-chain trust limitation, not evidence that the current source or release is compromised. A server operator cannot independently verify that fetched source or its built binary corresponds to a signed release identity.
- **Proposed fix:** Decide on a release trust model first. Options include signed annotated tags verified against an embedded maintainer key, signed checksums/provenance attestations, or both. Ensure key rotation and recovery are documented, then add offline-verifiable installer/update checks and tamper tests.
- **Implemented change:** The updater now defaults to prebuilt stable GitHub Release assets, verifies the asset checksum, architecture, and reported version, and retains local compilation as an alternative. The release workflow builds x86_64 and aarch64 musl assets and attaches SHA-256 sidecars. Independent signature/attestation verification remains open because the sidecar shares the GitHub Release trust boundary.
- **Tests added:** Shell regression tests reject checksum, architecture, and version mismatches; workflow pinning and shell parsing checks cover the release workflow. These do not provide independent cryptographic provenance.
- **Regression risks:** Enforcing a signature without a tested key-rotation and offline recovery path could block installs and upgrades during key rotation or network restrictions.
- **Commit:** Pending release commit.
- **Remaining risk:** Open until the release trust model is selected and a verification path is implemented and tested.

## Dependency and workflow controls

- GitHub Actions references are pinned to full commit SHAs. `actions/checkout` uses `persist-credentials: false`; workflow tests enforce both properties.
- The documentation Pages workflow grants `contents: read` globally and scopes `pages: write` and `id-token: write` to its deploy job.
- Dependabot checks Cargo, npm in both frontend/docs site, and GitHub Actions weekly.
- A scheduled and lockfile-triggered workflow runs `cargo audit` plus `npm audit --audit-level=moderate` for both JavaScript projects. It runs with `contents: read` only.
- `cargo audit` against the patched lockfile exited successfully and reported zero vulnerable packages. The audit tool could not refresh the crates.io index in this environment, but it loaded the RustSec advisory database and completed the lockfile scan.
- Frontend and docs-site `npm audit --audit-level=moderate` each completed with zero advisories.
- CI uses `npm ci`, `cargo --locked`, and a stable Rust toolchain. CI is not a reproducible binary build: the stable toolchain is moving, and release provenance/attestation is not produced.
- Systemd hardening was reviewed in the baseline: the panel service has filesystem and capability restrictions; helper restrictions are documented in [03-privileged-helper.md](03-privileged-helper.md). No additional unit restrictions were applied without Debian/Ubuntu runtime verification because helper operations require carefully scoped privileged access.

## Verification

- `cargo test --locked --all-targets` — passed, 246 tests.
- `cargo check --locked --all-targets` — passed.
- `cargo audit` — exit 0, zero vulnerable packages in the current lockfile.
- `npm audit --audit-level=moderate` in `frontend/` and `docs-site/` — both passed with zero advisories.
- `bash scripts/tests/test-workflow-security.sh` — passed.
- Python YAML parse of all workflow files and Dependabot configuration — passed.
- Installer/update/UFW/versioning shell tests and `bash -n` checks — passed in the earlier Phase 12 verification; see the CI and installer audit notes in [14-ci-quality-gates.md](14-ci-quality-gates.md) when Phase 14 is completed.
- No clean Debian/Ubuntu install, upgrade, interruption, or rollback VM was available. Those paths remain unverified at runtime.

## Phase result

One vulnerable dependency version was removed and its custom extraction contract gained regression coverage. CI action pinning and recurring advisory scans are in place. Cryptographic release provenance remains an explicit open risk; it was not silently treated as fixed.
