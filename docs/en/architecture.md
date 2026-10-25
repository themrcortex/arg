## Components

```text
Browser (React + TypeScript)
        │ authenticated HTTP API + terminal WebSocket
        ▼
ARG backend (Rust + Axum, arg system account)
        │ structured request over a private Unix socket
        ▼
arg-privileged.service (restricted root helper)
        │
        ├── systemd / UFW / Nginx / Docker / APT / MySQL
        └── approved file and S3 operations
```

The frontend and backend are one product. The frontend is embedded into the Rust binary at build time. The installer separately provisions systemd services and the privileged helper. The helper validates its request again; the browser does not receive a general-purpose shell endpoint.

## Services

- `arg.service`: panel and API, running as user `arg` with systemd restrictions.
- `arg-privileged.service`: allowlisted system operations requiring root. It does not expose an interactive root shell to the panel.
- Normal terminal and Root Terminal are separate paths. Current Root Terminal access requires an administrator, explicit warning confirmation, and renewed authentication.

## Important paths

| Path | Contents and purpose |
| --- | --- |
| `/usr/local/bin/arg` | CLI command and Bash control center |
| `/usr/local/lib/arg/arg` | Installed Rust binary |
| `/usr/local/lib/arg/update-helper` | Update status, cancellation, and rollback helper |
| `/opt/arg/source` | Source checkout used for updates and compilation |
| `/opt/arg/source/target` | Rust build artifacts; update cache mode controls whether they remain |
| `/etc/arg/arg.env` | Service bind, port, and database path; root-owned configuration |
| `/etc/arg/tls` | Protected panel certificate and private-key copies |
| `/etc/arg/s3` | Provider credentials in restricted files, never public API responses |
| `/var/lib/arg/arg.db` | SQLite users, sessions, settings, and history |
| `/var/lib/arg/rollback` | Previous binary and related database snapshot for rollback |
| `/run/arg-update*` | Temporary lock, cancellation request, and update state; cleared on reboot |

The database location can be changed in `/etc/arg/arg.env`. Use the configured path before copying or restoring a database.

## Lifecycle of a privileged operation

1. The frontend sends an authenticated API request. 2. Rust checks role, origin, payload shape, and size. 3. For system work, the backend sends a specific request to the helper over its Unix socket. 4. The helper executes an allowlisted action and returns a bounded result. 5. The backend writes the appropriate audit event and the UI shows the final state.

## Database and backups

SQLite may use WAL mode. ARG backup uses a consistent snapshot instead of blindly copying a file while it is being written. Keep an independent copy off the VPS; a backup on the same disk is lost if that disk fails.
