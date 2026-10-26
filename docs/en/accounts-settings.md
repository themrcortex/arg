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

- **Panel port:** change the bind port; the browser must use the new endpoint and provider firewall must be updated.
- **Timezone:** an IANA zone such as `Asia/Tehran` for schedules and display; backend timestamps remain epoch/UTC based.
- **TLS/Certificate:** certificate/key paths, hostname or IP, HTTPS port, and TLS activation. Keep private key files in allowed locations with restricted permissions.
- **Monitoring:** history enable/disable, sampling interval, and retention.
- **Alert thresholds:** resource warning levels.

Panel profile settings are distinct from host system settings. The ARG password is not the MySQL or SSH password.

## Settings and access behavior

Choose a setting section by its label for general preferences, monitoring, version, or TLS. Settings are stored by the backend, not just browser localStorage. After saving, inspect the result and new value; on failure, the old value may remain active.

- **Timezone:** use an IANA ID such as `Asia/Tehran`. A city name or fixed UTC offset alone does not model daylight-saving transitions. Backend event time is stored in a standard format and displayed in the chosen zone.
- **Monitoring:** Enabled controls background collection; Interval controls sample frequency and Retention controls how long samples remain. Example: one-minute sampling kept for seven days is lighter than 15-second sampling. Changing values does not fill past history.
- **CPU/RAM/Disk alerts:** a 1–100 percentage is a notification threshold, not a cgroup or service resource limit. Setting CPU to 85% triggers an alert when a sampled value crosses it; it does not throttle the process. Lowering the threshold can trigger an alert immediately.
- **Panel TLS:** configure actual public/fullchain and private-key paths on the server. Take the HTTP warning seriously: public HTTP does not encrypt login passwords or session cookies. Never put a private key in chat, logs, or browser storage. Test HTTPS and certificate renewal after saving.
- **About/updates:** compare SemVer and commit. Version identifies the release line; commit identifies exact installed source. An update notification does not install the update.
- **Interface preferences:** language, numeral style, and theme change display, not server-side units or calculations.

### Threshold example

If one-second spikes cause noisy alerts, keep thresholds appropriate to sustained use and align history sampling with the investigation need. An 85% disk threshold only notifies; it does not clean or resize the disk. Run storage analysis to find the largest directory and choose cleanup separately.

## User profile

The **Account** page manages username/password, active sessions, and logout. The new username must be unique; verify the current session and navbar after changing it. Use a long, unique password and keep it in a password manager. Revoking sessions may log out other browsers; revoke all if a session may have been stolen.
