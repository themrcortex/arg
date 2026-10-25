## Status and installation

The MySQL/MariaDB page first checks whether the server and service are installed. If missing, installation runs from the OS package repository after administrator confirmation. The systemd unit may be named `mysql` or `mariadb`.

## Unlock management

Before database operations, ARG asks once for the current panel password to re-authenticate. The management grant is short-lived and stored server-side; the panel password is not saved in browser storage. Open MySQL management only in a trusted administrator session.

ARG connects locally using temporary credentials or protected helper configuration. Database passwords are not returned through API responses or logs. A MySQL root/admin account may have broad access; review commands before execution.

## Databases and users

After access is granted, the catalog can inspect databases and users. A new user should receive only the required grants on the selected database. The newly set password is shown at creation/reset so an administrator can deliver it securely; ARG cannot recover an existing MySQL password from its hash.

**Reset password** sets a new password; it does not retrieve the old one. Applications using the previous credentials must be updated and their connection pools restarted.

## Execute SQL

The query editor applies execution-time and output limits. Arbitrary SQL on production is risky. Run `DROP`, `TRUNCATE`, `DELETE`, schema changes, or privilege changes only after review and backup. DDL is not guaranteed to roll back transactionally on every engine.

Results may be truncated or constrained by row/time limits; a success status does not validate application logic. Store exported sensitive data securely.

## Troubleshooting

- `unknown option '--defaults-extra-file'`: the option must be directly joined to its path with `=`, e.g. `--defaults-extra-file=/path/file.cnf`; the current manager uses this form.
- Access denied: check service state, socket, local authentication method, and root grants.
- New password fails: inspect exact `user@host`, auth plugin, application secret, and connection pool.
- Installation fails: check `apt-get update`, `/var` capacity, OS repositories, and service logs.
