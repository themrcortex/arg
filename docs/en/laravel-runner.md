# Laravel management

The Laravel Runner appears on application cards whose framework is Laravel. It inspects the active release, keeps environment data and `storage` across releases, installs dependencies from a lockfile, and executes a closed set of Artisan operations.

## Requirements

1. Register an application with the Laravel framework in **Applications**.
2. Deploy a Git, ZIP, or approved local-directory source from that card.
3. The release must contain `artisan`, `composer.json`, `composer.lock`, and `public/index.php`. A lockfile is required for repeatable dependency installation.
4. The selected PHP CLI version must be installed. Composer failures report platform or extension requirements when available.

Before the first deployment there is no active release to inspect. ARG reports that state instead of inventing readiness.

## Inspect readiness

Select **Check**. ARG reads Laravel and Composer files, the `public` path, the PHP constraint, and required `ext-*` packages. It shows environment variable names only; `.env` values are never returned by the API or UI.

- **Dependencies missing:** the lockfile or `vendor/autoload.php` is absent.
- **Environment required:** APP_KEY or database connection settings are incomplete.
- **Ready:** inspected files are present. HTTP and services are checked separately after web activation.

## Configure the environment file

Enter values such as `APP_ENV`, `APP_URL`, `DB_CONNECTION`, `DB_HOST`, `DB_DATABASE`, `DB_USERNAME`, and `DB_PASSWORD`, then select **Save variables**. Empty fields are left unchanged, so do not re-enter a stored password just to retain it. Values are written to `shared/.env` with private mode `0600`; the API never returns them. When PHP-FPM is enabled, an ACL grants the web service read access to the file without making its values available through ARG.

**Generate APP_KEY** is available only while the key is empty. A cryptographically random key is written directly to the private file. The button will not replace an existing key because changing APP_KEY can make Laravel-encrypted data unreadable.

`shared/.env` and `shared/storage` live outside release directories. New releases link to the same files and storage tree, and cannot replace the existing environment. Avoid putting secrets in Git or ZIP files. If the first source includes `.env`, ARG moves it into the persistent directory.

## Install Composer dependencies

If `vendor/autoload.php` is absent, select **Install from lockfile** and confirm. ARG runs a fixed `composer install` command using `composer.lock` with:

- `--no-dev` for a production dependency set
- `--no-scripts` and `--no-plugins` to keep package code from running during installation
- A 20-minute timeout and bounded stored output
- A preflight requirement of at least 512 MiB free disk space

This action connects to public package sources. Raw Composer output is withheld on failure because private repository URLs or credentials may be present. Review the error summary, PHP version, required extensions, and server connectivity.

## Artisan actions

Custom commands are not accepted. The Runner allows only:

| Action | Purpose | Data changes |
| --- | --- | --- |
| Laravel information (`about`) | Show application and environment details | None |
| Migration status | List applied and pending migrations | None |
| Clear cache | Run `optimize:clear` | Application caches are cleared |
| Build config cache | Run `config:cache` | Rebuilds cached configuration |
| Build route cache | Run `route:cache` | May fail when routes use Closures |
| Build view cache | Run `view:cache` | Compiles Blade views |
| Restart Queue | Run `queue:restart` | Signals workers using the cache to restart after the current job |
| Run migrations | Run `migrate --force` | Database schema or data may change |

Migrations require a separate confirmation dialog. Check the database backup before proceeding. A release rollback restores code only; it does not reverse a database migration. Operation history is persisted with bounded output and common secret patterns and values from `.env` redacted. Concurrent operations for one application are rejected.

## Web and background services

After the release and `.env` are ready, **Create and enable website** creates a dedicated PHP-FPM pool and socket for the app and a managed Nginx site named `arg-app-<id>`. The document root is only `public`; only the fixed `public/index.php` is executable through FastCGI. Hidden files and sensitive extensions are denied. PHP-FPM and Nginx configuration tests run before the site is enabled, with previous configuration restored on failure. Existing Nginx site names are checked for domain overlap.

ARG uses ACLs to give Nginx/PHP-FPM limited access to the release, `.env`, and storage paths for the app. If `setfacl` is missing, install the system `acl` package and retry. Storage ACL traversal is capped at 20,000 entries. Each PHP-FPM pool uses `pm=ondemand`, at most eight workers, and an `open_basedir` scoped to the app. Pools still share the operating-system account `www-data`; this is not full OS-user isolation between customers.

Disabling the site removes it from Nginx without deleting releases, `.env`, or files. The initial config serves HTTP on port 80. Issue HTTPS using the existing SSL tool for that site in the Nginx page.

**Queue Worker**, **Scheduler**, **Horizon**, **Reverb**, and **Octane** have independent, ARG-managed systemd controls when their Composer dependency is present in the active release. Horizon cannot run alongside ARG's separate Queue Worker for the same app. Reverb and Octane bind only to loopback on deterministic per-app ports; they are not exposed directly to the public network. When Octane is enabled for a live site, ARG validates and switches that site's Nginx upstream from PHP-FPM to the Octane loopback port. Disabling Octane switches the site back to PHP-FPM before stopping Octane. A failed Nginx/service transition is reported and the previous mode is restored when possible.

Service commands are fixed; ARG does not accept a shell command from the form. Each unit uses the selected PHP CLI, runs as the `arg` system user, restarts after a non-zero exit, and writes output to the system journal. Runtime status is re-read from systemd. When a release is deployed or rolled back, ARG refreshes the site configuration and restarts active app services. If this fails, deployment restores the previous code pointer; manual rollback also restores the previous pointer and makes a best-effort refresh. Rollback never reverses database changes.

The **Create database and user** action can provision a deterministic database and a separate account with grants scoped only to that database. MySQL must be installed, running, and unlocked once from the MySQL page by re-authenticating the panel administrator. ARG generates the password, stores it only in the app's private `shared/.env` file, and never returns it to the browser or audit log. If the app already has database values, ARG will not overwrite them. If writing `.env` fails, ARG attempts to remove only the database and account it just created and reports any incomplete cleanup.

The generated site starts on HTTP/80. Issue HTTPS with the existing SSL action for this site in the Nginx page. The site config remains ARG-managed and can be inspected there.

## API and data

- `GET /api/apps/{id}/laravel`: inspect a release and read environment key names.
- `GET /api/apps/{id}/laravel/operations`: read operation history.
- `POST /api/apps/{id}/laravel/operations`: enqueue an allowlisted action, for example `{ "action": "migrate", "confirmed": true }`.
- `PUT /api/apps/{id}/laravel/environment`: write a variable-name/value map; the response omits values.
- `POST /api/apps/{id}/laravel/environment`: generate APP_KEY only when it is empty.
- `GET /api/apps/{id}/laravel/web`: read site, PHP-FPM, and status for Queue, Scheduler, Horizon, Reverb, and Octane, including loopback ports.
- `POST /api/apps/{id}/laravel/web`: `{ "enabled": true|false }` to configure/enable or disable the site.
- `POST /api/apps/{id}/laravel/services`: `{ "service": "queue"|"scheduler"|"horizon"|"reverb"|"octane", "enabled": true|false }`.
- `POST /api/apps/{id}/laravel/database`: `{ "confirmed": true }` to provision the app-scoped database user after MySQL re-authentication.

All operations require an administrator and a valid Origin and are audited. `laravel_operations` persists history; work interrupted by a panel restart is marked `interrupted`.
