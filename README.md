<div align="center">

# ARG

### Self-hosted VPS Control Center

Manage Linux servers, services, websites, storage, and databases from a single web panel.

[![Rust](https://img.shields.io/badge/backend-Rust-DEA574?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/frontend-React-DEA574?style=flat-square&logo=react&logoColor=white)](https://react.dev/)
[![Platform](https://img.shields.io/badge/platform-Debian%20%7C%20Ubuntu-DEA574?style=flat-square&logo=linux&logoColor=white)](#requirements)
[![Docs](https://img.shields.io/badge/docs-bilingual-DEA574?style=flat-square)](https://themrcortex.github.io/arg/)

**Stable release:** `v0.7.2`

[Documentation](https://themrcortex.github.io/arg/) · [Report an issue](https://github.com/themrcortex/arg/issues) · [Maintainer](https://t.me/mrc0rtex)

</div>

---

ARG is a self-hosted control panel for administering Linux VPSs. A lightweight Rust service provides the API and privileged operations; the web interface is built with React and TypeScript. Manage the local VPS directly, or securely pair compatible ARG Nodes and switch the active workspace to administer a remote server. Cross-server rollout plans remain previews; they do not run fleet-wide deployments.

## Contents

- [Features](#features)
- [Quick install](#quick-install)
- [First login](#first-login)
- [CLI](#command-line-management)
- [Updates](#updates)
- [Architecture](#architecture)
- [Requirements](#requirements)
- [Development](#development)
- [Documentation and support](#documentation-and-support)
- [Security notes](#security-notes)

## Features

- **Dashboard and monitoring:** CPU, memory, swap, storage, network interfaces, traffic history, and system health.
- **Network and firewall:** interface and port inventory, UFW status and rules, and TCP/UDP port management.
- **Nginx sites:** inspect and edit site configurations, manage enablement, domains, TLS, maintenance pages, site actions, and supported resource limits.
- **Docker:** manage containers, images, networks, volumes, and Compose projects exposed by the installed Docker Engine.
- **Server files:** browse, edit, upload, download, copy, move, rename, ZIP, and safely extract files; analyze disk usage.
- **S3-compatible storage:** connect supported providers, browse objects, transfer files and folders, mount with Rclone, and scan bucket usage.
- **MySQL and MariaDB:** inspect databases and users, run bounded SQL operations, and manage credentials.
- **System administration:** manage APT packages and systemd services; use the web terminal, including a separately confirmed root terminal.
- **Application management:** register projects and environments, deploy supported sources into retained releases, inspect deployment logs, configure implemented runtimes, and roll back code releases.
- **Multi-server management:** pair ARG Nodes, inspect their health, switch server workspaces, and update eligible compatible Nodes through the controller. Availability depends on pairing compatibility and granted permissions.
- **Panel administration:** manage accounts, sessions, audit history, and ARG backup and restore operations.
- **Installer and service manager:** install, repair, update, cancel an update, roll back, configure TLS, and uninstall ARG.

Availability depends on installed system tools, permissions, and provider capabilities. The documentation describes feature-specific constraints; ARG does not present unsupported data as real measurements.

## Quick install

The installer is downloaded directly; cloning the repository is not required. Review the script before running it on a server:

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  https://raw.githubusercontent.com/themrcortex/arg/main/install.sh \
  -o arg.sh
less arg.sh
sudo bash arg.sh install
```

The installer targets Debian and Ubuntu systems using systemd. It installs build dependencies, compiles ARG, and configures its systemd services. Installation requires root or `sudo`, network access, and temporary free disk space for compilation.

HTTPS setup is optional during installation. Enter `skip` to continue without a certificate and configure TLS later. Until HTTPS is enabled, do not expose the login page over an untrusted public connection; use an SSH tunnel or a trusted private network.

## First login

After installation, the installer prints the panel address and a one-time setup code for creating the first administrator. Keep the code private. If the panel is bound to the server's local interface, connect through an SSH tunnel:

```bash
ssh -N -L 8282:127.0.0.1:8282 <SSH_USER>@<SERVER_IP>
```

Then open `http://127.0.0.1:8282` on your computer. Replace `8282` with the configured panel port if it has changed. For direct public access, configure HTTPS and allow the panel port in both the VPS firewall and the hosting provider's firewall.

## Command-line management

Once installed, use `arg` to manage the service:

| Command | Action |
| --- | --- |
| `sudo arg` | Open the interactive management menu |
| `sudo arg status` | Show service status |
| `sudo arg start` / `stop` / `restart` | Control the ARG service |
| `sudo arg logs [lines]` | View recent service logs |
| `sudo arg ssl` | Configure HTTPS for the panel |
| `sudo arg update` | Select branch and update method interactively; prebuilt Release is recommended |
| `sudo arg update main complete release` | Install the verified prebuilt stable binary |
| `sudo arg update development fast source` | Compile the development branch locally, retaining Fast cache |
| `sudo arg cancel-update` | Cancel a running update or clear a stale update lock |
| `sudo arg rollback` | Restore the previous saved ARG binary |
| `sudo arg password` | Reset an account password as root |
| `sudo arg delete` | Uninstall while preserving application data |
| `sudo arg delete --purge` | Uninstall and remove ARG data |

The installer menu is also available before installation:

```bash
sudo bash arg.sh
```

## Updates

ARG updates default to a prebuilt GitHub Release binary. The updater matches the selected source version to the release tag, verifies the asset's SHA-256 and CPU architecture, then swaps it in atomically. The release workflow checks the executable's reported version before publication. This avoids local compilation and build-cache disk use. To compile locally instead, select **Compile from source** in the panel or run `sudo arg update main fast source`. Fast/Complete cache choices apply only to source compilation. Stable `main` versions use `vX.Y.Z` tags and GitHub Releases; run `scripts/publish-release-tag.sh` from a pushed stable `main` commit to publish the tag and start the release workflow. Development version tags use `vX.Y.Z-dev.N` and publish as prereleases.

ARG supports two source branches:

- **`main`** — stable releases.
- **`development`** — prerelease changes; use on test systems if stability matters.

Choose a build-cache mode when updating:

- **`fast`** keeps Rust build artifacts to make later updates faster, at the cost of disk space.
- **`complete`** clears the build cache before and after compilation. It saves persistent cache space but takes longer; compilation still needs temporary disk space.

Examples:

```bash
sudo arg update main fast
sudo arg update development complete
```

Updates retain a previous binary for rollback. Rollback is a recovery aid, not a substitute for an independent database and server backup. Do not interrupt power or remove files while an update is replacing the service.

## Architecture

```text
React + TypeScript web interface
              │ authenticated API
              ▼
        Rust / Axum service ─── SQLite application data
              │
              ▼
   Restricted privileged helper ─── system services and host operations
```

The privileged helper is intended to expose validated operations rather than execute arbitrary shell strings from the browser. The panel and helper run as systemd services. Application data and configuration are stored under `/var/lib/arg` and `/etc/arg` on standard installations; consult the [installation guide](docs/en/install.md) for current paths and details.

## Requirements

- Debian or Ubuntu Linux with systemd.
- Root or `sudo` access for installation and host-level administration.
- Internet access to GitHub and the operating system's package repositories during installation and updates.
- Free disk space for Rust dependencies and a release build. The amount and build time vary with cache state and dependencies.
- Network access to the configured panel port. HTTP-01 certificate issuance also requires inbound TCP port 80 during validation.

Other distributions may work, but are outside the installer's supported target.

## Development

The application uses Rust for the backend and React, TypeScript, Vite, and Tailwind CSS for the frontend. The frontend build is embedded into the Rust release binary.

```bash
# Frontend checks and production build
cd frontend
npm ci
npm run check
npm run build
cd ..

# Backend tests and release build
cargo test --locked
cargo build --locked --release
```

See the [development guide](https://themrcortex.github.io/arg/development/) for local setup, project layout, testing, and versioning conventions. Stable changes belong on `main`; prerelease work belongs on `development`.

## Documentation and support

The bilingual documentation site follows a guide-oriented structure and covers installation, security, the dashboard, networking, Nginx, Docker, files, S3, databases, terminal access, packages, updates, and troubleshooting:

- [Open ARG documentation](https://themrcortex.github.io/arg/)
- [English documentation source](docs/en/overview.md)
- [Persian documentation source](docs/fa/overview.md)
- [Open an issue](https://github.com/themrcortex/arg/issues)

## Security notes

ARG can change firewall rules, manage services, edit files, and perform other high-impact server operations. Install it only on systems you administer, keep SSH access available, and review confirmations before applying destructive changes.

- Enable HTTPS before using the panel over the public internet. Plain HTTP does not protect passwords or session cookies in transit.
- Use a strong, unique administrator password and keep the initial setup code private.
- Back up important data independently before upgrades or administrative changes. ARG rollback does not restore all external services or user data.
- Limit access to the panel port at both the host and provider firewall layers.
- S3 provider features and reported capacity vary. Unknown quotas remain unknown; a scan of object sizes is not a provider billing statement.

---

<div align="center">

Built by [𝓜𝓻.𝓒𝓸𝓻𝓽𝓮𝔁](https://t.me/mrc0rtex) · [GitHub repository](https://github.com/themrcortex/arg)

</div>
