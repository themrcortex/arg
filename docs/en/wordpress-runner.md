# Managed WordPress first deployment

The **WordPress** template can create and deploy a new site on the local VPS. This is a managed first-install workflow; registering a generic application with framework `wordpress` does not opt it into this workflow.

## Requirements

- Use the WordPress template and provide a valid hostname, site title, administrator username, email, and password (12–128 characters).
- The server needs outbound package and source access. ARG installs Nginx, MariaDB when one is not installed, PHP-FPM/CLI and the WordPress PHP extensions with the system package manager. It does not run a general package upgrade.
- The privileged helper and `setfacl` must be available. The normal ARG deployment root is `/var/lib/arg/deployments`; for a custom root, the parent must already be traversable by `www-data`.
- Public DNS must point to this VPS before automatic Let's Encrypt issuance can succeed. A DNS or ACME failure does not discard a healthy HTTP installation; the job records a warning.

## What the first deployment does

1. Fetches the selected exact version from the official WordPress source and validates the core files and version.
2. Installs/checks the bounded runtime package set, then enables Nginx, PHP-FPM, and MariaDB.
3. Creates an app-scoped MariaDB database and account. It refuses to take over a pre-existing database without ARG's ownership marker.
4. Writes a root-owned, PHP-FPM-readable `wp-config.php`; generated database credentials and the initial administrator form are stored in ARG's encrypted root secret vault. The administrator form secret is deleted after the installer succeeds.
5. Keeps `wp-content` outside releases so uploads survive a redeploy, and grants PHP-FPM only the traversal/read access needed to reach this app. The `/var/lib/arg` ACL grants `www-data` execute-only access: it does not grant directory listing.
6. Creates/enables the ARG-managed Nginx site, attempts SSL, updates the WordPress URL only after local TLS health succeeds, then checks `/wp-login.php` through Nginx and PHP-FPM. The job succeeds only after an HTTP 200 health response.

Each operation is visible in the deployment job (`wordpress-runtime`, `wordpress-database`, `wordpress-ssl`, `wordpress-health`). Open the job details for the actual step output and final recovery message.

## SSL and reachability

The SSL attempt uses the template email and the Nginx/Certbot integration. A certificate is not guaranteed just because deployment is queued: DNS, external port 80 reachability, ACME limits, and provider validation must succeed. On failure, the site remains on HTTP if its local Nginx health check passes, and the warning is retained in the job. Review DNS and Nginx firewall access, then issue SSL from the site's Domains & SSL tab.

The local health check proves that this VPS serves the WordPress login route through its own Nginx/PHP-FPM stack. It does not prove public DNS, firewall reachability, or external TLS trust. Check those separately from a client outside the VPS.

## Security and recovery

- Only an administrator can submit the first-install form. The helper accepts fixed WordPress operations rather than a general root command.
- PHP-FPM serves the app as `www-data`; the ARG panel account is not used as the website runtime user.
- Database identifiers are derived from the app UUID. SQL values are validated/escaped and sent on stdin or through a mode-0600 temporary client file, not command arguments.
- Unknown `wp-config.php`, symlinks in the downloaded core, foreign databases, and mismatched ownership markers stop provisioning without overwriting those resources.
- Release activation is atomic. If Nginx or health validation fails, ARG restores the previous release and disables the new site when there was no previous release.
- Custom deployment roots are not opened up automatically. Put them under a path already traversable by `www-data` or use the standard `/var/lib/arg/deployments` root.

This workflow is local-VPS only. It does not configure a WordPress site on a remote ARG Node. It also does not configure mail delivery, backups, plugins/themes, external DNS records, or a separate staging domain automatically.
