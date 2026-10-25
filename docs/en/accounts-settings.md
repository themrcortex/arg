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
