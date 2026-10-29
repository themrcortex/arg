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

The installer checks available RAM/free swap, the memory limit imposed by cgroups, and the CPU cores available to ARG. In **Fast** mode, the Rust profile stays stable for that VPS: dependencies use optimization level 2 and a codegen-unit count based on the host's memory capacity and CPU count; only Cargo's job count changes with currently available RAM. From 4 GiB available, it schedules about one job per 2 GiB, capped by available CPU cores. Below 4 GiB it uses one job to reduce the chance of another OOM. ARG's application crate uses optimization level 1 in Fast mode. Temporary RAM pressure therefore does not invalidate the dependency cache. **Complete** mode uses the project's regular Release profile with conservative memory safeguards. Below 2 GiB, the installer also attempts a temporary swap file (up to 2 GiB) under `/var/tmp` and removes it after the build. If the kernel or filesystem does not allow swap, the installer reports why and continues with a limited job count. The running panel binary is not replaced until compilation succeeds.

If compilation fails, ARG does not replace the active binary. The updater records a failed result, removes temporary files, and releases its update reservation. Fast mode preserves successfully compiled dependency artifacts so a retry does not start from zero; only Complete mode clears the target to reclaim disk space.

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

**Fast update** retains Rust dependency caches and incrementally compiles ARG's application crate. Its dependency profile stays stable on each VPS, so changing free RAM changes job scheduling but does not trigger a full dependency rebuild. With enough memory, Cargo runs parallel jobs up to the server's allowed CPU count; low-memory hosts intentionally stay at one job to avoid OOM kills. The first Fast build after a profile change, empty cache, or `Cargo.lock` change still compiles new dependencies. ARG's crate uses optimization level 1 and dependencies level 2. **Complete update** clears its cache before the build and removes it after completion; it takes longer but retains less build data. Retained `target` output can consume several GiB. Choose Complete on low-disk hosts and Fast for frequent updates.

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
