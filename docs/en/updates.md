## Branches

- **`main`:** stable and the default source.
- **`development`:** prerelease; it may be unstable or contain unpublished changes. Do not use it on a sensitive server without a snapshot and a second access path.

The selected branch determines which GitHub ref is fetched and which version/API is built.

## Build cache modes

- **Fast:** retains Rust `target` and incrementally compiles ARG's application crate. The first build after a complete update, an empty cache, or changed Rust dependencies can still take a while because changed dependencies must be rebuilt. Later builds are usually shorter while dependencies stay unchanged. This mode keeps build artifacts on disk.
- **Complete:** clears the old cache before building and removes the new cache after build completion. Compilation still needs temporary disk space; this reduces persistent cache at the cost of longer updates.

Cache size depends on Rust target, architecture, and dependencies; there is no universal fixed value. Complete mode does not delete source, installed binary, or rollback files.

### Build resource safeguards

Before compiling, the installer checks free disk space on the source filesystem: it requires at least 2 GiB for a complete build and 1 GiB for a fast build. Complete mode clears the old Cargo target before this check. It refuses to start a build that is likely to fill the filesystem.

The installer also sums currently available RAM and free swap and detects the CPU cores available to the build. Below 2 GiB it uses one Cargo job, one codegen unit, and optimization level 0 to limit memory use. From 2 GiB to 4 GiB it keeps Cargo serial and scales codegen units by RAM and CPU, up to eight. At 4 GiB and above, Cargo jobs scale with the available CPU and memory budget (up to one job per 3 GiB); per-job codegen units are also capped by RAM, each job's CPU share, and 16 units maximum. This uses the available cores without starting several memory-heavy compiler processes at once. Hosts below 8 GiB use optimization level 2. Below 2 GiB the installer also attempts to add a temporary swap file (up to 2 GiB) under `/var/tmp`; the file is disabled and removed after the build. Swap files are only attempted on ext2/3/4, XFS, and supported Btrfs filesystems, and only when enough disk space is free. If the kernel or filesystem does not allow swap, the installer continues with the minimum-memory build profile and reports why. The running panel binary is not replaced until compilation succeeds.

If compilation fails, ARG does not replace the active binary. The updater records a failed result, removes temporary files, releases its update reservation, and clears the incomplete Cargo target even in fast mode so a retry can reclaim disk space. Complete mode also clears the target after a successful build.

## SSH updates

Select branch and mode in the SSH menu. Non-interactive examples:

```bash
sudo arg update main complete
sudo arg update development fast
```

Arguments are branch followed by mode. Only `main|development` and `fast|complete` are accepted. Running `sudo arg update` in an interactive TTY prompts for missing choices.

## Update from the panel

The update dialog shows branch/version/commit, mode, progress phase, and available logs. A valid update acquires a server-side lock so concurrent actions from other users do not conflict; other sessions can see the active update state.

The web service may restart during the update. The process builds, installs the binary, restarts the service, and performs a health check. An overlay blocks conflicting UI actions. On completion the UI attempts a cache-busting reload.

## Cancellation and rollback

`cancel-update` or the cancel button requests a safe stop; the process may need to reach a safe phase. If the new binary is installed but health checking fails, the installer restores the previous binary and database snapshot. If the service still cannot start because the host is damaged, use SSH logs and an independent backup.

Rollback also restores the previous database snapshot; database changes made afterward can be lost. Keep an independent backup and provider console access before important updates.

## Update troubleshooting

```bash
sudo arg status
sudo arg logs 200
sudo journalctl -u arg -n 200 --no-pager
sudo arg cancel-update
sudo arg rollback
```

If a lock appears stale, check for a real process or systemd job before clearing it. Never manually remove `/run/arg-update*` during a live build. Separate GitHub network, disk exhaustion, Rust toolchain, and Cargo failures using the logs.

## Branch and build mode

The **main** branch tracks the stable line; **development** receives earlier changes and has higher regression risk. Use main in production. The selected branch is the updater's source for the next revision; inspect the fetched branch/commit and release notes after switching. Choose development only to test a specific fix and keep rollback available.

**Fast update** keeps release artifacts and Rust dependency caches, then incrementally compiles ARG's application crate. Source-only updates are usually shorter, at the cost of disk space. The first Fast build after a **Complete update**, an empty cache, or a `Cargo.lock` change can still take a long time while dependencies are compiled. Fast uses a lower optimization level for ARG's application crate; dependencies keep the normal Release profile. **Complete update** clears the old cache, makes a fully optimized Release build, then removes the new cache. It takes longer but retains less build data. Retained `target` output can consume several GiB. Choose Full on low-disk hosts and Fast for frequent deployments with sufficient free space.

## Progress, cancel, and rollback

Distinguish Fetch, Compile, helper/file install, restart, and health check. Build percentage is an estimate because Cargo does not provide reliable percentages for all phases; stage text and last activity are more meaningful. Closing the modal does not stop the process.

**Cancel update** requests cancellation and releases the reservation/lock. If the binary is being swapped, wait for rollback/cleanup; do not manually delete the lock. **Rollback previous version** restores the saved binary and restarts ARG, but does not necessarily roll back database migrations or data files. Make a separate DB/config backup before important updates.

### Safe release workflow

1. Record current version and commit in Settings/About.
2. Choose the branch and Fast/Complete mode; check free disk and maintenance time.
3. Enable Maintenance and create a backup if users are doing sensitive work.
4. Start Update and follow Fetch/Build/Install/Health stages.
5. Treat only success plus health check as completion; after the new UI loads, perform a cache-busting refresh.
6. Verify version, login, main pages, and service logs. Save failure details and rollback when appropriate.
