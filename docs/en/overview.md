## ARG at a glance

ARG is a self-hosted control center for Linux VPS management. The browser panel is backed by a Rust service; the web interface uses React and TypeScript. Pages focus on operational tasks: inspect real status, change configuration, run a guarded action, and review its result.

ARG is not a cloud-provider console, enterprise observability platform, independent backup system, or multi-server fleet manager. Values are shown only when the operating system or service can report them. Planned work is not described as an available feature.

## Available capabilities

- Dashboard and background SQLite history for CPU, memory, disk, and network traffic rates.
- Network interface and listening-port inventory plus local TCP connectivity probes.
- UFW management, TCP/UDP rules, and protection for detected SSH and ARG ports.
- Nginx site management, configuration editing, domains, TLS, maintenance mode, site actions, and resource limits for isolated systemd services.
- Docker and Compose management for resources reported by the installed Engine.
- Server file manager, transfers, safe ZIP operations, and disk usage analysis.
- S3-compatible connections, object browser, VPS-to-Bucket transfers, Rclone mounts, and Bucket usage scans.
- Basic MySQL/MariaDB management, database users, bounded SQL operations, and password reset.
- APT, systemd services, terminal, panel users, sessions, audit history, and ARG backup/restore tools.

Each feature guide describes its actual limits. For example, a cloud quota is only displayed when a provider reports it or an administrator records it; otherwise it remains unknown.

## Design principles

1. **Measured data:** unknown is shown as unknown, never replaced with zero or sample data.
2. **Privilege boundaries:** normal operations are validated in Rust. Root-only work is delegated through a restricted allowlisted helper.
3. **Traceability:** administrative actions are audited; passwords, keys, and confidential content must not enter logs.
4. **Recoverability:** updates preserve the previous binary and a database snapshot for rollback. This is not an independent backup.
5. **Visible risk:** network, firewall, TLS, file, and service changes should make their target and impact clear.

## Supported scope

The installer targets Debian and Ubuntu systems that use systemd. Package names and service behavior vary across releases. Test on a disposable VPS and keep a provider snapshot before production changes. ARG does not replace out-of-band SSH access.

## Version and branches

`main` is the stable branch. `development` is a prerelease branch and may change or be unstable. At the time of writing, stable is `0.6.5` and development is `0.6.6-dev.1`.
