# Runtime and language version management

Runtime management reports which PHP, Node.js, Python, and build tools are installed on **this VPS**, which versions are available from its configured APT sources, and which version an application record requires. It does not create provider accounts or contact an external runtime service.

## Getting started

1. Open **APT management**.
2. Review **Runtime status and server preflight**. It reports the operating system, architecture, installed runtimes, supporting tools, available memory, deployment-path disk space, and whether APT sources are present.
3. In **Language and Runtime packs**, select a version actually listed by the configured repositories.
4. Choose **Preview**. ARG runs APT simulation and shows packages that would be installed, upgraded, or removed.
5. Review the output, then choose **Confirm and install**. The real operation uses the configured APT sources.
6. After the operation completes, ARG refreshes the installed-runtime inventory.

## Status meanings

- **Installed:** a system executable was found by running a fixed version command without a shell.
- **Missing:** the inventory did not find the tool in the system paths it checks. This does not by itself mean APT is broken.
- **No APT sources:** no usable source file was found; the available version list may be empty.
- **Unsupported distribution:** the runtime installation path is preflighted for Ubuntu and Debian only.
- **PHP CLI; no FPM:** command-line PHP exists, but the matching PHP-FPM version was not found; it cannot serve a PHP website on its own.
- **FPM inactive:** PHP-FPM is installed, but its systemd unit was not active at the time of the check.

Resource values are a point-in-time measurement. Memory and disk space may change before installation. Unknown values are not presented as zero.

## Installable packs

ARG reads package names from the server's configured APT repositories and groups available PHP, Python, Node.js, Java, and Ruby versions. PHP packs include available CLI, FPM, and extension packages. Other packs likewise contain only real package names returned by the configured sources.

ARG does not silently add NodeSource or another third-party repository, and it does not run `curl | bash`. If a needed version is not listed, configure an official distribution or vendor source according to your server policy, refresh APT metadata, then load the catalog again.

The preview is simulated output; it does not install anything. Check that it does not remove a critical package or add an unexpected dependency. After confirmation, the real APT operation can still fail due to repository connectivity, a package lock, or low disk space; the actual result appears in the operation report.

## Assigning a version to an application

In an application form, choose a framework to identify its runtime family:

- Laravel and WordPress: PHP
- Node.js: Node.js runtime plus a separate build version
- Python: Python
- Static and custom applications: no runtime is required by the template

The selector is populated from installed versions on that VPS. PHP choices require the matching PHP-FPM version to be installed. **System default** leaves the version unpinned. Node's build version is saved separately from its runtime version for applications that need different build and execution toolchains.

This preference is stored in the application record. Additive migrations give existing records the default `system`. ARG does not yet run the application's build script or create its process service in this phase. Treat it as a requested version, not proof that a runtime process is active. Later runner phases use it for preflight and restricted execution.

## Supporting tools

The inventory checks Composer, npm, pnpm, Yarn, Corepack, Nginx, and the Docker CLI. It does not execute pnpm/Yarn/Corepack shims: their versions are read from dpkg metadata so opening the page cannot make a package-manager shim download itself. If a binary exists but no matching dpkg package is known, the UI reports **present; version unverified**. Missing tools are reported independently of PHP/Node/Python status. Opening the page never installs software.

## API

- `GET /api/runtimes`: administrator-only host inventory and preflight from the restricted helper.
- `GET /api/packages/runtimes`: runtime packs available from current APT sources.
- `POST /api/packages/preview`: validates package names and simulates installation.
- `POST /api/packages/action`: APT install and maintenance operations with package allowlists and bounded input counts.
- `POST /api/apps` and `PUT /api/apps/{id}`: store `runtime_version` and `build_runtime_version`. Each accepts only a numeric version or `system`.

All routes require an administrator session. The helper invokes fixed program names and arguments; user input is never passed to a shell.

## Troubleshooting

### A version is missing from the pack list

The configured repositories may not provide it, or their metadata may be stale. Run **Update package lists** and load the packs again. ARG does not install an external repository for you.

### PHP is visible but the website does not run

Check that the matching PHP-FPM version is installed and active, and that Nginx points to its socket. PHP CLI alone cannot answer web requests.

### The preview fails

The error usually points to a repository, invalid package/version pin, dpkg lock, or dependency conflict. Do not confirm installation until the preview succeeds. If preview succeeds but installation fails, inspect the APT report and free disk space.

### An application version is not in inventory

Install that version from the packs available on this VPS, then reopen the application form. A previously stored value is preserved and shown explicitly if it is still unknown.

## Current limits

- Runtime installation uses the existing APT configuration on Debian/Ubuntu; ARG does not add third-party repositories.
- Installing a package does not configure Nginx/PHP-FPM or create an application systemd service.
- Selecting a version does not run framework builds or start applications; those belong to later runner phases.
- This page does not infer which sites depend on a runtime before removing it. Package removal can affect other sites and must be reviewed separately in APT management.
