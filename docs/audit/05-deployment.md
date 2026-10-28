# 05 — Application deployment and runtime

## Scope and revision

- Reviewed the `development` source at starting revision `eec273cde037fa57edc1b5ab042fda7acc26a32c`, as continued on isolated branch `audit/development-hardening`.
- Phase 5 code fixes were committed as `14aa314ac77e68de47909b26f6fa2f3ef3ca9595` and `d8489670b72c3e891220706e5d89ec4e1dc13506`.
- Areas reviewed: `src/deploy.rs`, `src/node.rs`, deployment-related privileged action dispatch in `src/main.rs`, and adjacent application/runtime code in `src/apps.rs`, `src/buildpacks.rs`, `src/ssh_keys.rs`, `src/laravel.rs`, and `src/docker.rs`.
- No production host, remote Git repository, user database, or live application was contacted. The cancellation regression test launches only a local `/bin/sleep` child and checks that it exits.

## Deployment flow and trust boundaries

1. Create/upload routes validate an administrator, application ownership, Origin for writes, bounded idempotency keys, and bounded pre-deploy command input. A deployment job and its progress rows are persisted before execution.
2. Sources are selected from constrained public Git URLs, pinned GitHub SSH forms, uploaded ZIPs, or approved server paths. Git hooks and local file protocol are disabled; SSH uses pinned GitHub host keys and an application-specific key path.
3. ZIP upload is capped at 256 MiB. Extraction enforces entry count, expanded-size and compression-ratio ceilings; rejects traversal, symlinks, hardlinks and special entries; and excludes known secret/runtime paths.
4. Build/runtime detection occurs in a per-job release. Node install/build steps use a transient `systemd-run` unit with `DynamicUser`, filesystem restrictions, `NoNewPrivileges`, `MemoryMax=2G`, `CPUQuota=200%`, `TasksMax=256`, and `RuntimeMaxSec=1200`. The resulting application service has separate systemd isolation and a loopback listener.
5. The release is activated by switching an application-local `current` symlink. Health verification can restore the prior release pointer on failure. Recovery examines the active symlink and release before resolving interrupted jobs; release retention keeps the active/previous releases plus a bounded history.
6. Deployment output is stored as bounded persistent job logs and common secret assignments/Bearer values are redacted. User-supplied pre-deploy input is validated and passed as argument tokens, not interpolated into `sh -c`.

## Findings

### DEPLOY-001 — Cancelling an active deployment did not stop its running command

- **ID:** DEPLOY-001
- **Title:** Active deployment cancellation only changed the database flag while the child command continued running.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High; regression test failed against the pre-fix implementation and passes after the change.
- **Module:** Deployment worker and Node build helper
- **File:** `src/deploy.rs`, `src/node.rs`, `src/main.rs`
- **Function:** `cancel`, `command_output`, `command_output_bounded`, `request_build_step_with_live_logs`, `helper_build_cancel`, `run_sandboxed`, `execute_privileged_request`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** The cancel endpoint set `cancel_requested`, but a Git command or a build running under a transient systemd unit could continue until it completed or reached its 15/30/180-second command timeout or 1,200-second unit maximum. The UI could show cancellation requested while CPU, memory, network, or disk work remained active.
- **Technical evidence:** Before the fix, `command_output` only wrapped the process output future in a timeout and did not observe the persisted cancellation flag. Git commands did not opt into Tokio `kill_on_drop`. The Node build helper awaited its `systemd-run --wait` request while only streaming logs; no cancel action targeted the transient unit.
- **Safe reproduction:** `active_git_command_wait_stops_when_job_is_cancelled` runs a local 30-second child, sets the in-memory test job's `cancel_requested` flag after 100 ms, and asserts the operation returns in under one second and the process is no longer running. The test failed before the fix because the process returned normally only after the sleep completed. `build_cancel_helper_rejects_untrusted_unit_inputs_without_running_systemctl` verifies that invalid caller-provided unit data cannot reach `systemctl`.
- **Root cause:** Cancellation state was persisted but was not connected to subprocess lifecycle; transient systemd build units had no specific stop path.
- **Security/functional impact:** A cancelled deployment could continue consuming resources and mutating staging/build state for a long time. This could block subsequent work and contribute to disk or memory pressure.
- **Proposed fix:** Poll the cancellation state while waiting on each Git child, enable `kill_on_drop`, terminate only the transient build unit derived from validated application/job UUIDs and an allowlisted step, and report a cancellation as cancelled rather than failed.
- **Implemented change:** Added a cancellable process wait loop for Git operations, `kill_on_drop(true)` to all five deployment Git command paths, a cancellation check during bounded fetch monitoring, and a helper action that constructs only `arg-node-build-{app8}-{job8}-{step}` after strict UUID/step validation. The build-log loop requests stop for the exact unit when cancellation is observed. Source acquisition, Node build and pre-deploy cancellation errors now end in the `cancelled` state. No arbitrary systemd unit or shell command is accepted.
- **Tests added:** `active_git_command_wait_stops_when_job_is_cancelled`; `unit_and_service_group_names_are_uuid_derived` (extended for deployment unit derivation); `build_cancel_helper_rejects_untrusted_unit_inputs_without_running_systemctl`.
- **Regression risks:** Stop requests are asynchronous and the deployment helper may take time to report the stopped unit. The worker retries the bounded stop request while waiting. A real systemd transient-unit cancellation/restart test was not possible in this workspace.
- **Commit:** `14aa314 fix(deploy): stop active commands when jobs are cancelled`.
- **Remaining risk:** Git cancellation kills the direct Git child through Tokio. A full VPS test should verify that the Git remote helper is also reaped in all supported Git/network configurations. Real systemd cgroup termination and cancellation during package lifecycle scripts remain release-gate tests.

### DEPLOY-002 — ZIP uploads could consume the disk reserve and race each other

- **ID:** DEPLOY-002
- **Title:** Streaming deployment ZIP uploads did not check free space while writing and concurrent uploads could consume the same available-space estimate.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High; the upload loop had no `statvfs` check before its fix and the new reserve arithmetic and exclusive-writer behavior are covered by regression tests.
- **Module:** Deployment archive upload
- **File:** `src/deploy.rs`
- **Function:** `upload_archive`, `ensure_space`, `archive_upload_chunk_required_space`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`
- **Affected behavior:** A valid upload up to 256 MiB could fill the filesystem used by ARG and its temporary ZIP. Concurrent authenticated uploads could independently observe the same free space and both write.
- **Technical evidence:** The old streaming loop enforced a byte cap and request timeout but did not call `ensure_space` before each write. The deployment upload directory is shared by the panel service; there was no exclusive upload admission guard.
- **Safe reproduction:** `archive_upload_preserves_minimum_free_disk_reserve` checks the 64 MiB reserve calculation at representative chunk sizes. `archive_upload_gate_rejects_a_second_concurrent_writer` verifies the second writer is rejected while an upload holds the exclusive permit. No filesystem was filled during testing.
- **Root cause:** The upload path guarded total request size but had no free-space policy or serialization across concurrent uploads.
- **Security/functional impact:** Exhausting the filesystem could disrupt ARG, deployments, logs, and other services on the host.
- **Proposed fix:** Admit a single ZIP upload at a time and verify that each incoming chunk can be written while leaving a fixed safety reserve. Remove the incomplete temporary file on the insufficient-space path.
- **Implemented change:** Added a non-waiting process-wide upload permit (`429` for a competing upload), checks `statvfs` for each chunk plus a 64 MiB remaining reserve, and returns `507 Insufficient Storage` after closing/removing the temporary archive when space is insufficient. Existing total-size, idle timeout, overall timeout, and digest verification remain in place.
- **Tests added:** `archive_upload_preserves_minimum_free_disk_reserve`; `archive_upload_gate_rejects_a_second_concurrent_writer`.
- **Regression risks:** Only one archive upload per panel process is admitted at a time; clients that try parallel uploads must retry after the current upload finishes. The 64 MiB margin is a conservative floor, not a guarantee against unrelated processes writing to the same filesystem after the check.
- **Commit:** `d848967 fix(deploy): reserve disk space for archive uploads`.
- **Remaining risk:** The free-space check is inherently a point-in-time check and cannot reserve blocks against other system processes. A cross-process upload lock would be needed only if multiple ARG service instances can write to one deployment root; the supported installation runs one `arg.service` instance.

## Controls reviewed

- Database schema adds deployment state fields idempotently and contains uniqueness/active-job rules; no schema change was needed in this phase.
- Job creation and recovery are persistent and bounded; interrupted work is recovered based on cancellation state and active release pointer.
- Git source validation rejects credentials and unsupported hosts; SSH deployment-key support is limited to GitHub and pinned host keys.
- ZIP handling rejects traversal and non-regular/symlink entries, caps total archive bytes and expanded output, and cleans temporary files on ordinary error paths.
- Build and runtime execution are isolated by transient/persistent systemd units. Manual user-defined pre-deploy commands are bounded and argument-tokenized.
- Health checks and atomic release activation preserve the prior release on validation failure. Deletion excludes active/previous releases and checks canonical ancestry.
- Deployment logs have byte/line caps and secret redaction.

## Verification

- `cargo test --locked --all-targets`: **226 passed, 0 failed**.
- `cargo check --locked --all-targets`: **passed**.
- `git diff --check`: **passed**.
- Tests ran locally in the isolated workspace. No GitHub/network clone, package installation, systemd unit, Nginx activation, user data, or production service was exercised.
- No database migration was required.
- Full formatter/linter limitations are recorded in [00-baseline.md](00-baseline.md); this phase did not claim those unavailable checks as passed.

## Review limits and follow-up

- Validate transient systemd unit cancellation and cgroup cleanup on a disposable Debian/Ubuntu VM, including cancellation during dependency install and package lifecycle scripts.
- Verify Git child-process cleanup with real public Git and pinned SSH fixtures in a network-isolated integration environment.
- Exercise upload cancellation at the HTTP body level and disk-full behavior using a loopback-mounted test filesystem rather than filling the host filesystem.
- The application deployment worker is serialized with other app mutations at route/job boundaries; load and fairness characteristics are part of phase 10.
