# Dashboard: complete server overview guide

The dashboard is a compact view of the VPS's current state and recorded resource trends. It does not provide arbitrary command execution: actions are explicitly typed and constrained by ARG. Live values come from the Rust API; historical samples are stored by the ARG service in SQLite. Missing data is shown as unknown or an error, never invented as zero.

## Open the page and refresh

Select **Overview** in the sidebar. The page begins with the host name, a **Refresh** button, and a welcome banner. The host name helps identify the selected VPS; it is not necessarily the website domain or public IP. Compare it with your provider or the **System & Resources** page if you may be viewing another server.

Refresh rereads current dashboard data and the selected history range. These are separate requests, so one can fail while the other succeeds. Successful cards remain visible and an alert identifies what could not be loaded. Current status and history refresh automatically every 15 seconds. Firewall status, managed-rule count, and S3 summary are fetched independently every 30 seconds; failure of one does not break the rest of the page.

## Summary cards

| Card | Meaning | Do not infer |
|---|---|---|
| CPU | Latest stored CPU sample as a percentage; `—` means no sample exists | It is not one CPU core or one website's usage |
| RAM | `(total RAM - available RAM) / total RAM`; consumed and total sizes are also shown | OS cache can affect usage definitions; inspect kernel logs when diagnosing OOM |
| VPS storage | Filesystem inventory; aggregation is shown only when independent mounts can be summed reliably | Duplicate mounts, overlay, and pseudo-filesystems must not be counted twice; use System for detail |
| S3 storage | Healthy connections, measured usage, and known quota when the provider or a contract supplies it | Do not treat an unknown quota as a percentage or a stale scan as current usage |
| Uptime | Seconds since the host's last boot, formatted as days or a shorter duration | This is not ARG service uptime |
| Load average | Linux load values for 1, 5, and 15 minutes; the first value is highlighted | Load is not CPU percentage and includes runnable or I/O-waiting tasks |
| ARG process | CPU and RSS reported for the ARG service process | It does not include websites, Docker, or database processes |

**RAM example:** If the card says `3.2 GiB of 8 GiB` and `40%`, the first number is used memory and the percentage is the calculated ratio. If RAM reaches 95% without an OOM event, inspect `free -h`, high-usage processes, and `journalctl -k` before increasing the VPS size.

## Charts, ranges, and timestamps

The dashboard charts CPU, RAM, and network rate. CPU and RAM use percent. Network shows sampled receive/send **rates in bytes per second**; it does not by itself show monthly total transfer or traffic for an individual site. Use the Network page for interface details and traffic windows.

Available ranges are **5 minutes, 1 hour, 24 hours, 7 days, and 30 days**. Selecting one requests retained samples for that range; it does not recreate old samples. For a custom range, select **Custom**, enter start and end date/time, and choose **Apply range**. Start must precede end, end cannot be in the future, and the range cannot exceed 366 days. Example: to inspect a CPU spike today from 10:00 to 10:30, enter both timestamps in the panel/browser timezone; only already-recorded points can appear.

Every point belongs to its stored timestamp. Multi-day axes may combine dates and times; hover a point for its exact time. A fresh installation, long host outage, disabled history, or retention cleanup leaves no reconstructable past data. Refresh cannot synthesize historical samples.

### Background collection and retention

ARG records samples in the background, so the dashboard need not stay open. Monitoring settings control whether history is enabled, its interval, and retention. Disabling history stops future collection but does not necessarily delete existing rows immediately. Shorter intervals increase chart detail, SQLite rows, disk writes, and computation. Longer retention consumes more disk. Keep the default on small VPSs unless an incident requires finer detail; after changing it, check the database size.

If **History is paused** appears, check the monitoring setting. If a graph is empty, narrow the range or confirm collection status; no samples is not the same as 0%.

## Network and security summaries

### Network interfaces

The card shows up to the first four interfaces returned by the API: interface name, first reported address, and link state. `up` means the interface is up, not that it is reachable from the Internet. Use **View all** and the Network page for active-interface filters and details. Private ranges such as `10.x`, `172.16–31.x`, and `192.168.x` are not public addresses by themselves.

### Listening ports

Up to four listeners are shown with port, protocol, and IPv4/IPv6. A listener means a process is bound on the host. Internet access also depends on DNS, the VPS firewall, provider security groups, and routing. If ARG appears here but cannot be reached externally, check the Ports page and provider firewall. Omitted ports are not necessarily closed.

### UFW status

The firewall summary reports whether UFW is installed, active, and how many ARG-managed rules exist. That count is not a count of every manually created system rule. Choose **Details** to open UFW management. Before enabling UFW, allow the current SSH port and panel port to avoid losing remote access.

### S3 summary

The dashboard reads the S3 summary independently. Capacity appears only when a reliable measurement or known quota exists. Usage scans may paginate through Objects; do not treat stale usage as today's value. Open **S3 Storage** for provider details and scan time.

## Fast Actions: create, run, and verify

Fast Actions appears below the banner for administrators. Choose **Add action** to save a constrained operation. Each card runs only its configured operation:

| Operation | Use and example | Limitation |
|---|---|---|
| Test Nginx configuration | Validate syntax before reload, e.g. a “pre-release Nginx check” | Does not change configuration |
| Reload Nginx | Apply loaded configuration with minimal interruption | Fix syntax errors first and inspect the dialog result |
| Restart Nginx | Restart the service process | Web connections may briefly drop |
| Start systemd service | Start a selected allowed unit, e.g. a project worker | Choose a listed service; arbitrary unit names are not shell commands |
| Reload systemd service | Ask the selected unit to reload supported configuration | Not every unit supports reload |
| Restart systemd service | Restart a selected worker or app service | Check dependent services and websites first |
| Custom action | Run a saved short command as the ARG service user; example: `php artisan queue:restart` where the project supports it | Not root, limited to 30 seconds, and does not provide arbitrary root shell or privilege escalation |

In the editor, **Button title** is limited to 48 characters and **Operation** controls which target fields appear. Service actions require a listed **systemd unit**. A custom command accepts up to 2,048 bytes; the command and path must be accessible from ARG's execution context. For example, `php artisan queue:restart` is valid only if the required project directory/environment is available to that context. Keep commands short, self-contained, and free of secrets.

Two output switches have different meanings:

- **Show execution logs** includes stdout/stderr from that custom command, up to 64 KiB per stream. It does not read an application's log file.
- **Show site Error Log** attaches the selected Nginx site's error log after the operation. Select a site; if its log cannot be read, that part of the report will show an error.

Example: create an Nginx health check by naming it `Check Nginx`, selecting **Test Nginx configuration**, optionally selecting a site and enabling its Error Log, then saving. Click Play to run it. The modal reads the pre-operation server/Nginx/service state, shows the current stage, executes the action, rereads state, and presents result, error, and selected logs. Keep the modal open until completion; rerunning can have side effects. If the after-state is not what you expected, open the relevant service page and inspect it.

**Edit** changes the saved title, type, or target. **Delete** removes only the action definition; it does not remove a service or project file. Custom actions request a separate confirmation before execution. Because they run with the ARG service account's permissions, use auditable commands and avoid broad redirects, `sudo`, recursive deletion, remote script download/execution, or embedded credentials.

## Troubleshooting

1. **Current status does not load:** choose Retry. If only graphs fail, check the history service and SQLite access.
2. **Graph is empty:** shorten the range, enable History, and inspect the latest sample time. Past data before collection was enabled cannot be created.
3. **Load is high but CPU is low:** I/O wait may be involved; inspect System and disk read/write history.
4. **A listener is shown but the panel is unreachable:** listener state only confirms local binding. Check UFW, provider firewall, TLS, and address separately.
5. **Fast Action fails:** read its current stage and exit code. For Nginx, run the config test first. For systemd, inspect the unit state on Services. For a custom action, verify its execution user and working directory.
6. **Storage differs from another tool:** compare measurement definitions. Dashboard storage uses filesystem inventory; Docker overlay, cloud mounts, and virtual filesystems may be excluded from reliable physical totals.

## Example: investigate a slow website

1. Change the chart range from 5 minutes to 1 hour to distinguish a brief spike from sustained load.
2. Compare CPU and RAM with Load; high Load alone does not diagnose a CPU bottleneck.
3. Check RX/TX on Network and disk/I/O on System.
4. Verify listeners on ports 80/443 and firewall rules.
5. Test Nginx configuration, then inspect the affected site's logs.
6. If needed, create a narrow Fast Action and review its before/after state. Do not make a risky change based on a single chart.

## Related pages

- [System and storage](system-storage.md) · [Network and firewall](network-firewall.md) · [Nginx sites](nginx.md)
- [S3](s3.md) · [Monitoring settings](accounts-settings.md) · [Audit log](security.md)
