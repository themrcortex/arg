## Nginx sites page

ARG lists manageable Nginx configuration files and reports enabled state, domains, document root, and service state. “Enabled” generally means a configuration is linked into the enabled path; it does not prove that the application is healthy.

## Create and edit configuration

Create sites from a visual form/preset or a text editor. Options for common project types include domains, `root`, `index`, charset, body-size limit, static paths, PHP-FPM socket, reverse proxy/upstream, security headers, cache, TLS, and redirects. A framework preset is only a starting point; review the final file against the actual app version and layout.

For manual edits ARG runs `nginx -t` before applying changes and restores the prior configuration if validation fails. Reload and restart differ: reload reads configuration with minimal interruption; restart starts the service process again.

## Per-site actions

- **Enable/Disable:** create or remove the Nginx enabled symlink and validate configuration.
- **Rename/Description:** change an internal label or note. Domain changes edit `server_name`/configuration; they do not change DNS.
- **Test/Reload/Restart:** validate syntax and manage the Nginx service.
- **Domain/DNS:** check DNS answers against the server address; update records at the DNS provider yourself.
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
