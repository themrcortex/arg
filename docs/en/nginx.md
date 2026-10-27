## Nginx sites page

ARG lists manageable Nginx configuration files and reports enabled state, domains, document root, and service state. “Enabled” generally means a configuration is linked into the enabled path; it does not prove that the application is healthy.

## Create and edit configuration

Create sites from a visual form/preset or a text editor. Options for common project types include domains, `root`, `index`, charset, body-size limit, static paths, PHP-FPM socket, reverse proxy/upstream, security headers, cache, TLS, and redirects. A framework preset is only a starting point; review the final file against the actual app version and layout.

For manual edits ARG runs `nginx -t` before applying changes and restores the prior configuration if validation fails. Reload and restart differ: reload reads configuration with minimal interruption; restart starts the service process again.

## Per-site actions

- **Enable/Disable:** create or remove the Nginx enabled symlink and validate configuration.
- **Rename/Description:** change an internal label or note. Domain changes edit `server_name`/configuration; they do not change DNS.
- **Test/Reload/Restart:** validate syntax and manage the Nginx service.
- **Domain/DNS:** check DNS answers against the server address; update records at the DNS provider yourself. For Cloudflare, see [Cloudflare DNS management](?page=cloudflare-dns).
- **SSL:** request a certificate for the selected site and configure Nginx. DNS, ports 80/443, external reachability, CA limits, and current config all matter. Site certificates are separate from ARG panel certificates.
- **Maintenance mode:** select a default maintenance response or a configured project/static file. It affects new requests; it does not necessarily stop in-flight requests. Force mode can interrupt active work.
- **Custom Actions:** add site-specific buttons backed by supported operations; this is not arbitrary root shell access.
- **Logs/Traffic:** view access/error logs and estimates based on sampled log lines; these are not authoritative network totals.

## Site resources

CPU/RAM limits can only isolate an application running in its own systemd unit. Limiting a shared Nginx or PHP-FPM unit affects multiple sites. ARG warns that limits apply to the whole unit. The selected unit must actually run that site's application; its name alone is not enough.

Live CPU is sampled from systemd/cgroup accounting. If accounting is unavailable or the wrong unit is chosen, usage remains unknown. Set CPU/RAM limits conservatively: low limits can cause timeouts, OOM, or restarts. Clearing a limit removes that resource cap; it does not delete or move site files.

## Common issues

- **Enabled but unreachable:** check DNS, provider firewall, listener ports, UFW, `nginx -t`, logs, and upstream.
- **PHP fails:** verify installed PHP-FPM version and actual socket under `/run/php/php*-fpm.sock`.
- **Certificate not issued:** DNS must resolve, port 80 must be reachable, and any CDN/proxy must support the challenge.
- **Resource usage unavailable:** verify systemd accounting and a dedicated app unit; shared Nginx/PHP-FPM cannot be attributed to one site.

## Practical workflow for one site

The Nginx page reads service state and enabled/disabled site configuration files. On each card, verify the filename and domain. **Disable site** removes only the active symlink; it does not delete the configuration file. Enabling activates the existing file and validates Nginx before/after the change. If you have duplicate server blocks or `default_server`, inspect `nginx -t` and `listen` before modifying anything.

### Visual builder and project type

The create-site builder offers presets for Static, PHP, Laravel, Reverse Proxy, and Redirect. A preset is a starting point, not a definitive framework detector. Review the generated preview before saving. If ARG infers a type from file content, `root`, `server_name`, or PHP locations, confirm it matches the actual project and change it if necessary.

- **Domain / server_name:** enter real hostnames and point DNS at this server. Use wildcards only with matching DNS and certificates.
- **Listen:** port/address binding. HTTPS requires a certificate listener; an open UFW port alone does not bind Nginx.
- **Document root:** absolute web root. Laravel usually needs `.../public`, not the project root, to keep `.env` and source code outside the web path.
- **Index:** default files such as `index.html` or `index.php`, in priority order.
- **Proxy pass:** upstream such as `http://127.0.0.1:3000`; ensure the application actually listens there.
- **PHP-FPM socket:** installed version's socket, e.g. `/run/php/php8.2-fpm.sock`. Inspect the host; do not copy a placeholder blindly.
- **Redirect to:** destination and, when needed, Nginx variables such as `$request_uri`. Avoid HTTP/HTTPS redirect loops.
- **Maximum body size:** request/upload ceiling such as `32m`; coordinate it with the app and PHP `upload_max_filesize`/`post_max_size`.
- **Timeout and buffering:** upstream settings. Excessive timeouts can occupy workers and VPS resources.
- **Rate limit:** rate and burst settings must match a zone defined globally in Nginx; an empty field does not create a missing global zone.
- **Logs:** per-site Access/Error log paths; preserve shared logs before changing them.
- **Server tokens and security headers:** hide version details or add baseline headers. Do not enable CSP/HSTS without understanding the app impact.
- **SSL certificate/private key:** paths to public `fullchain.pem` and private `privkey.pem`; the private key contents are never read into or exposed by the form. Use the file picker and verify Nginx can read the files.

Manual sections for `server`, primary `location`, and `TLS` require directives valid in that Nginx context. A `server {}` block inside a location or a duplicate `proxy_pass` can invalidate the configuration. Review the preview, save, then test. Do not reload after a failed test; fix the reported line first.

### Example: Laravel with HTTPS

1. Point the domain to the VPS IP and verify the project lives under `/var/www/league` with a `public` directory.
2. Select the Laravel preset, set root to `/var/www/league/public`, and index to `index.php`.
3. Select the real PHP-FPM socket under `/run/php/`. Execute only the intended `index.php` and deny paths such as `.env`, `.sql`, and `.log`.
4. Save and run `nginx -t` on HTTP first. After obtaining a certificate, add HTTPS and fullchain/privkey paths, then verify HTTP-to-HTTPS redirection.
5. Before applying, verify DNS, TCP ports 80/443, UFW, and certificate file permissions.

### Site maintenance mode

A site's Maintenance control offers off/on and a response source: ARG default page, an HTML file inside the project root, or an allowed static file on the server. The project-root option requires a relative file inside the configured root and limits it to 2 MiB; static paths are restricted. Inspect page content before making it public.

Entering maintenance can drain in-flight requests before switching. Force mode bypasses the wait and may interrupt user requests. For a schema change or incompatible deployment, enable maintenance, inspect long jobs, deploy, and disable it after verification. After forcing, separately audit transactions/uploads/payments that may have been interrupted.

### Per-site resources, traffic, and actions

Each site's resource panel expands inside its card. CPU/RAM can only be attributed per site when the application runs in its own systemd unit; shared Nginx or PHP-FPM cannot be reliably constrained to one site. Select the actual application unit. CPU uses systemd's percentage limit and memory uses MiB; an empty field removes that resource limit. Applying limits changes/restarts the unit and can affect every site relying on shared FPM.

Traffic comes from available site/log data; read its source and time and do not treat it as provider billing. Per-site action buttons (Site Actions) save a label, icon, supported operation, and valid target. They are bounded Nginx/systemd or form-defined controls, not arbitrary shell. Enable the relevant execution/site log options, run the action, and inspect result/output. An action does not implicitly remove the site's files or service data.

### Automated SSL and recovery

The certificate tool uses selected domains. Before starting, verify DNS A/AAAA, port 80, outbound ACME access, and any Nginx/service occupying port 80. If a certificate already exists, ensure the requested domains align with the current certificate/expansion behavior. On failure, the previous site file should remain intact; read logs and address DNS, timeout, rate-limit, or hostname issues. After issuance, choose fullchain/private key paths in the visual form, test, and then reload. Verify the HTTPS URL in a browser.

## Disk file versus database/editor state

If ARG reports a mismatch between the live config file and the builder/database representation, compare each side by label and sample. **Current file on disk** is what Nginx reads; the DB/form is saved management state. Choosing DB→file overwrites the operational file; file→DB imports the current content into the editor model. Back up the file and inspect the diff before choosing. If saving appears stuck in Loading, do not click twice; refresh the file and API result first.
