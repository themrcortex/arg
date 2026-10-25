## Open the menu

```bash
sudo arg
```

Move with the arrow keys, type an option number, and press Enter. On startup the menu checks service status, panel address, and the selected branch's update status. After a successful update it relaunches the menu using the newly installed version.

## Main commands

| Command | Purpose |
| --- | --- |
| `sudo arg` | Open the interactive control center |
| `sudo arg status` | Show ARG's systemd service status |
| `sudo arg start` / `stop` / `restart` | Control the panel service |
| `sudo arg logs [count]` | Show service logs; defaults to 100 lines |
| `sudo arg manage port` | Change the panel port through the menu |
| `sudo arg ssl` | Configure a certificate and panel TLS |
| `sudo arg update` | Choose a branch and cache policy, then update |
| `sudo arg update main complete` | Fetch stable main and clear build cache before/after compilation |
| `sudo arg update development fast` | Fetch prerelease development and retain the cache |
| `sudo arg cancel-update` | Request cancellation or clear a stale update lock |
| `sudo arg rollback` | Restore the previous binary and database snapshot |
| `sudo arg password` | Reset an account password as root |
| `sudo arg delete` | Remove ARG and choose whether to retain its database |
| `sudo arg delete --purge` | Remove ARG and its data after explicit confirmation |

Use the exact English values `main`, `development`, `fast`, and `complete`. If branch or build mode are omitted in an interactive TTY, the installer prompts for them.

## Management menu entries

- **Install / repair:** fresh install or service repair. A repeated run can fetch source from the default branch or an environment-selected branch.
- **Service status:** systemd health and process information.
- **Start / Stop / Restart:** control `arg.service`. Stop makes the panel unavailable but does not shut down Linux.
- **View service logs:** `journalctl` output for diagnosis.
- **Change panel port:** update the bind port and synchronize managed firewall rules. Keep a safe SSH path before changing network access.
- **Configure HTTPS / SSL:** request a certificate and configure Rust TLS, or continue with manual certificate configuration.
- **Update ARG:** choose Git branch and cache policy, build, and install while retaining the previous release.
- **Uninstall:** remove ARG-owned files and optionally keep the database.
- **Get one-time setup code:** show the first-admin code while initial setup is incomplete.
- **Reset account password:** perform a root-level reset without the old password.
- **Cancel running update / Rollback:** control an update or restore the last saved release.

## Exit output and logs

Preserve stdout and error output when troubleshooting. For example:

```bash
sudo arg update main complete 2>&1 | tee /tmp/arg-update.log
```

This file may contain compiler output and system paths. Review it and redact sensitive information before sharing.
