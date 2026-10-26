## APT

The package manager runs supported Debian/Ubuntu APT operations such as updating metadata, upgrading, installing, removing, autoremove, and cleaning. Each action card describes the system command and reports operation progress/results.

- `apt update`: refresh repository metadata; it does not upgrade packages.
- `apt upgrade`: install available package updates; services may restart.
- `apt install`: install a package and its dependencies.
- `apt remove`: remove a package; package configuration may remain.
- `apt purge`: remove the package and its package-managed configuration.
- `autoremove`: remove automatically installed packages no longer needed; review the removal set.
- `clean` and `autoclean`: remove downloaded package caches, not installed software.

Distribution upgrades and arbitrary repository changes are not ordinary panel APT actions. Before production upgrades, keep a provider snapshot and console access.

## systemd services

The service list reads active/enabled state from systemd. Supported UI actions include start/stop/restart/reload/enable/disable as permitted. `active` means running now; `enabled` means configured to start at boot. They are independent states.

ARG blocks disruptive actions against critical services such as SSH, networking, and ARG itself. Treat other services carefully too: stopping a database, reverse proxy, or firewall can interrupt applications or remote access.

```bash
systemctl status nginx
systemctl is-enabled nginx
journalctl -u nginx -n 100 --no-pager
```

## Optional dependencies

Docker, Rclone, Certbot, and other tools may be optional dependencies. Opening a page should not install every dependency. Package versions depend on the OS repositories; check the installed version and error message.

## APT operations

The packages page lists installed packages and repository state. Read the system command beside an action before running it:

- `apt update` refreshes repository metadata; it does not install packages by itself.
- `apt upgrade` installs available upgrades and may restart services.
- `apt install <package>` installs a package and dependencies.
- `apt remove <package>` removes the package but may leave configuration behind.
- `apt purge <package>` removes package-managed configuration; unrelated data may remain.
- `apt autoremove` proposes/removes dependencies no longer needed; review the list.
- `apt clean` clears downloaded package archives, not installed software.

Match a package row to its exact name/version. Progress on an action card reports current work/output; do not refresh or rerun before a final state/exit code. OS upgrade differs from ARG update: the former changes Debian/Ubuntu packages, while the latter replaces ARG itself.

## systemd services

Read the exact unit name such as `nginx.service`, `docker.service`, `mysql.service`, or `ssh.service`. Start starts a stopped service; Stop cuts off dependent access; Restart causes brief downtime; Reload rereads configuration only if supported; Enable/Disable controls startup on future boots and may not change current state. Disabling Nginx, for example, can make a site unavailable after reboot even if it is running now.

ARG, SSH, and network services are critical. Check dependencies and recovery access before stopping/disabling them; never stop ssh from your only remote session. If an action fails, inspect `systemctl status <unit>` and systemd logs. Repeating Restart without understanding the error may worsen the situation.
