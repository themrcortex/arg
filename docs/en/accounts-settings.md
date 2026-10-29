## Panel users and roles

An ARG panel account is not a Linux user. ARG roles govern panel permissions; they do not create or alter Linux accounts or sudoers.

- **Admin:** administrative access to sensitive features.
- **Operator:** routine actions permitted by the relevant UI/API policy.
- **Viewer:** read-only pages/data as enforced by each endpoint.

Administrators can create users, change username/role/enabled state, or revoke access. Be careful not to lock out the last administrator.

## Personal account

The profile menu contains username changes, password changes, sign-out, and active sessions. Panel password changes do not require the old password or a second entry in the current workflow; use a strong, unique password. CLI reset is a root-level recovery route and should not be exposed on a public browser.

Active sessions show device/time metadata and allow revoking another session. Signing out invalidates that session. If an account is compromised, change its password, revoke unknown sessions, and review SSH/access logs too.

## Audit and history

Audit associates recorded administrative actions with an actor and time. It is not a full kernel-level command recorder; operations outside ARG may not appear. Treat CSV exports as sensitive.

Metric history differs from Audit: metrics store CPU/RAM/disk/network samples; Audit stores administrative activity. Retention can delete old metric rows.

## Panel and host settings

- **Panel ports:** HTTP and HTTPS ports are configured separately. HTTP changes run in the background; ARG checks for a free port and an HTTP/HTTPS collision, then probes the new endpoint. If the restart or health check fails, the prior port is restored. When TLS is active, HTTP is the backend listener; continue using the HTTPS address.
- **Timezone:** an IANA zone such as `Asia/Tehran` for schedules and display; backend timestamps remain epoch/UTC based.
- **TLS/Certificate:** certificate/key paths, hostname or IP, HTTPS port, and TLS activation. When both paths point to one Certbot lineage under `/etc/letsencrypt/live/<name>/`, ARG enables a twice-daily renewal timer and a lineage-specific deploy hook that copies renewed files into `/etc/arg/tls` and restarts `arg.service`. The page reports whether automatic renewal is configured. A certificate copied manually to `/etc/arg/tls` cannot be renewed by Certbot; renew it at its issuer and configure the replacement. Keep private key files in allowed locations with restricted permissions.
- **Monitoring:** history enable/disable, sampling interval, and retention.
- **Alert thresholds:** resource warning levels.

Panel profile settings are distinct from host system settings. The ARG password is not the MySQL or SSH password.

## Settings and access behavior

Choose a setting section by its label for general preferences, monitoring, version, or TLS. Settings are stored by the backend, not just browser localStorage. After saving, inspect the result and new value; on failure, the old value may remain active.

### Changing the panel ports

Go to **Settings → Panel access** to change the HTTP port. ARG checks availability, rejects a collision with HTTPS, and verifies the service and any legacy reverse proxy after restart. If a local check fails, it restores the previous port and proxy configuration. For a public HTTP listener, open the new port in the VPS provider firewall as well. Change HTTPS separately under **Panel SSL certificates**; the dedicated action keeps the installed certificate and does not require entering its paths again. ARG checks for a port conflict and updates local UFW rules. Once the service restarts, the browser probes the new endpoint and confirms the new session; if the browser cannot reach it within 90 seconds, ARG restores the previous HTTPS port.

For example, if HTTPS currently uses `8443` and Nginx owns `443`, first allow TCP `9443` in the VPS provider firewall, then enter `9443` and choose **Change HTTPS port safely**. The page waits for TLS on the new address, navigates there, and confirms the session. If external access fails, ARG restores the prior port after 90 seconds and the old page reloads. You can select `443` when no other service is listening there. ARG updates local UFW rules automatically; the VPS provider or cloud firewall must be configured separately. HTTP and HTTPS cannot share a port.

- **Timezone:** use an IANA ID such as `Asia/Tehran`. A city name or fixed UTC offset alone does not model daylight-saving transitions. Backend event time is stored in a standard format and displayed in the chosen zone.
- **Monitoring:** Enabled controls background collection; Interval controls sample frequency and Retention controls how long samples remain. Example: one-minute sampling kept for seven days is lighter than 15-second sampling. Changing values does not fill past history.
- **CPU/RAM/Disk alerts:** a 1–100 percentage is a notification threshold, not a cgroup or service resource limit. Setting CPU to 85% triggers an alert when a sampled value crosses it; it does not throttle the process. Lowering the threshold can trigger an alert immediately.
- **Panel TLS:** configure actual public/fullchain and private-key paths on the server. For a Certbot lineage, Settings installs the renewal hook and ensures a timer is enabled; renewal copies the latest certificate into ARG's private TLS directory and restarts the panel. Manual files do not renew automatically. Take the HTTP warning seriously: public HTTP does not encrypt login passwords or session cookies. Never put a private key in chat, logs, or browser storage.
- **About/updates:** compare SemVer and commit. Version identifies the release line; commit identifies exact installed source. An update notification does not install the update.
- **Interface preferences:** language, numeral style, and theme change display, not server-side units or calculations.

### Threshold example

If one-second spikes cause noisy alerts, keep thresholds appropriate to sustained use and align history sampling with the investigation need. An 85% disk threshold only notifies; it does not clean or resize the disk. Run storage analysis to find the largest directory and choose cleanup separately.

## User profile

The **Account** page manages username/password, active sessions, and logout. The new username must be unique; verify the current session and navbar after changing it. Use a long, unique password and keep it in a password manager. Revoking sessions may log out other browsers; revoke all if a session may have been stolen.
