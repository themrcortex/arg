## ARG at a glance

ARG is a self-hosted control center for Linux VPS management. The browser panel is backed by a Rust service; the web interface uses React and TypeScript. Pages focus on operational tasks: inspect real status, change configuration, run a guarded action, and review its result.

ARG is not a cloud-provider console, enterprise observability platform, or independent backup system. ARG does support secure pairing between panels and can switch the workspace to a compatible remote Node. This does not provide general fleet-wide rollouts: rollout previews and private-network drafts do not apply network policy or deploy to servers. Values are shown only when the operating system or service can report them. Planned work is not described as an available feature.

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
- Application management with durable jobs, separate releases, step logs, implemented runtime runners, and code rollback. Available actions depend on the supported platform.
- ARG Node pairing, health inspection, and workspace selection; remote updates are limited to compatible Nodes with the required permission.

## Choose a guide by task

- **Install the panel safely:** start with [Install & quick start](./install.md), then review [Security](./security.md).
- **Create and deploy a website:** start with the [application registry](./application-registry.md), then read [deployment jobs](./deployment-jobs.md) and the [Node.js](./node-runner.md) or [Laravel](./laravel-runner.md) guide for the selected runtime.
- **Manage an existing website:** use [Nginx and TLS](./nginx.md); review prerequisites and rollback behavior before changing configuration or issuing a certificate.
- **Connect another VPS:** [Federation and Node management](./federation.md) explains controller/Node roles, one-time pairing codes, permissions, and compatibility limits.
- **Find disk usage:** use [File management](./files.md) for directory analysis and [System & disks](./system-storage.md) for mounts and measured capacity.
- **Move files to object storage:** read [S3 storage](./s3.md); scanned object usage, contractual quota, and provider billing are different values.
- **Update or recover ARG:** review [Updates](./updates.md) and [Maintenance & backups](./maintenance.md) before changing versions.

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

`main` is the stable branch; `development` is prerelease. The documentation site's version label comes from the Cargo version on the branch that built GitHub Pages. See [`CHANGELOG.md`](https://github.com/themrcortex/arg/blob/main/CHANGELOG.md) for stable releases. Do not assume feature or API parity between branches; for remote Nodes, verify version compatibility and negotiated capabilities.
