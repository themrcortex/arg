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
