## Status and installation

The MySQL/MariaDB page first checks whether the server and service are installed. If missing, installation runs from the OS package repository after administrator confirmation. The systemd unit may be named `mysql` or `mariadb`.

## Unlock management

Before database operations, ARG asks once for the current panel password to re-authenticate. The management grant is short-lived and stored server-side; the panel password is not saved in browser storage. Open MySQL management only in a trusted administrator session.

ARG connects locally using temporary credentials or protected helper configuration. Database passwords are not returned through API responses or logs. A MySQL root/admin account may have broad access; review commands before execution.

## Databases and users

After access is granted, the catalog can inspect databases and users. A new user should receive only the required grants on the selected database. The newly set password is shown at creation/reset so an administrator can deliver it securely; ARG cannot recover an existing MySQL password from its hash.

**Reset password** sets a new password; it does not retrieve the old one. Applications using the previous credentials must be updated and their connection pools restarted.

## Version compatibility

MySQL and MariaDB releases do not expose identical columns in `mysql.user`; ARG first includes the lock state; if the server does not expose `account_locked`, it retries with the shared `user`, `host`, and `plugin` columns.

## Execute SQL

The query editor applies execution-time and output limits. Arbitrary SQL on production is risky. Run `DROP`, `TRUNCATE`, `DELETE`, schema changes, or privilege changes only after review and backup. DDL is not guaranteed to roll back transactionally on every engine.

Results may be truncated or constrained by row/time limits; a success status does not validate application logic. Store exported sensitive data securely.

## Troubleshooting

- `unknown option '--defaults-extra-file'`: the option must be directly joined to its path with `=`, e.g. `--defaults-extra-file=/path/file.cnf`; the current manager uses this form.
- Access denied: check service state, socket, local authentication method, and root grants.
- New password fails: inspect exact `user@host`, auth plugin, application secret, and connection pool.
- Installation fails: check `apt-get update`, `/var` capacity, OS repositories, and service logs.

## Unlock and management areas

On first opening **MySQL Management**, ARG requests the panel password once for the current session. This is not the MySQL password. After confirmation, administrative queries remain unlocked for that session and access expires when you log out. Read an incorrect-password error and retry; never put a database credential in this field.

Separate **installed?**, service-active, version/provider, and helper-access states. Installation uses the host's APT repositories; selecting MySQL Server or MariaDB Server installs and enables that service and changes disk/service state. Do not reinstall an existing database as a generic repair step.

- **Databases:** create/list/drop catalogs. Verify the name and application dependencies before Drop; dropping a schema destroys its data.
- **Users:** inspect user and Host. `localhost` and `%` represent different connection scopes. For a new user, create a strong password and grant only the database privileges it needs.
- **Change/forgotten password:** reset is performed by the database administrator and does not require the old DB password, but the one-time panel unlock is required. Update the application's secure configuration and restart its connection pool.
- **SQL Console:** run SELECT first and inspect returned rows. DDL/DML such as `DROP`, `TRUNCATE`, and `UPDATE` can permanently change data. Transactions only roll back for supported engines/tables; multiple queries are not guaranteed atomic.
- **Query errors:** system schema differs between MySQL and MariaDB releases. For example, `account_locked` is not present in every version; ARG has a compatibility fallback. If a manual query fails, start with `SELECT VERSION()` and `SHOW COLUMNS FROM mysql.user`.

### Example: least-privilege database user

1. Create a lowercase database such as `shop_prod`.
2. Create a separate `shop_app` user restricted to `localhost` and generate a unique password.
3. Grant only required permissions on `shop_prod.*`, not `*.*` or `GRANT OPTION`.
4. Store credentials in a secret manager or a permission-restricted configuration file.
5. Test the app connection; do not use ARG's internal admin account for the application.

Refresh rereads status. If MariaDB is installed and a MySQL-specific query fails, check version/support and preserve the complete error; do not alter system tables without understanding the release.
