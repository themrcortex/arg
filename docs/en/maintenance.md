## Before maintenance

The **Server & ARG management** page is available to administrators. Before repair, restore, reboot, or removal, make sure you have SSH or provider-console access. A backup stored on the same VPS does not protect against server loss or removal; download important backups to a separate destination.

## Read-only ARG Doctor report

ARG Doctor reads status only. It does not install, repair, or restart anything, and it does not actively probe remote servers. It reports the operating-system family, presence of systemd, Docker installation indicators, free root-disk space, configured panel TLS files, and selected executables. For paired Nodes it lists the saved public name, endpoint origin, compatibility, last saved connection state, last successful contact, last check time, and saved error. `healthy` and the legacy `connected` state count as connected. It separately lists enabled controller panels by display name and last contact, without exposing credentials.

This is not a full audit: finding a Docker binary does not prove that the Engine or containers work; finding a runtime does not verify its version or application health; S3 and node status may come from the last saved check. Doctor does not contact providers or read private-key contents. For a fresh result, open the relevant feature page and run its own check.

## Create, download, and upload a backup

Backups from this page contain the **ARG database**, including accounts, panel settings, history, and connection records. They do not include installation files, TLS certificates or keys, or application files. **Create backup** makes a SQLite copy of the database. Download it and keep it off the server.

## Automatic backup schedule

In **Automatic backup schedule**, enable or disable the scheduler, choose an interval from 1 to 720 hours, and retain between 1 and 100 automatic copies. The first scheduled copy is due after the chosen interval. The worker runs independently of the browser and resumes from the saved due time after an ARG service restart. It validates each SQLite copy before registering it. When the retention limit is exceeded, only files created by the automatic scheduler are removed; manual and uploaded backups remain untouched. The page shows the last attempt, last successful run, next due time, current automatic-copy count, and the latest error. Failed runs are retried after about one hour.

Automatic copies remain on the same VPS in the ARG backup directory. They are useful restore points, not an off-site disaster-recovery copy; download important backups to another server or storage service.

To transfer an existing backup, upload its `.db` file. ARG checks the SQLite format and integrity and verifies that it contains ARG account data. Files larger than `512 MiB` are rejected. Uploading registers a backup; it does not replace the current database.

## Restore a backup

1. Check the backup filename and date to confirm it is the version you intend to restore.
2. Select **Restore** for that file and read the warning about replacing the current database.
3. Type `RESTORE ARG` and confirm.
4. Before replacement, ARG creates a protective backup of the current database. After success, the page reloads so sessions read the restored data.

Restore changes only the ARG database. Installation files and TLS certificates are not restored from this backup. If restoration fails, keep the error and check file access and service status before retrying.

## Repair, reboot, and remove ARG

- **Repair ARG files** queues a complete rebuild through the protected updater. The panel may be temporarily unavailable; follow progress in the update overlay. If the rebuild fails, the updater attempts to restore the previous version.
- **Reboot server** restarts the whole VPS. The panel and SSH will be unavailable for a while. Wait for the host to return before retrying an operation.
- **Remove ARG** deletes the ARG service, installation files, settings, and database. Local backups are removed with the installation, so download them before confirmation. This operation is not intended to remove other software from the server.

Enter the exact confirmation phrase shown for each action. Repair is different from restarting the ARG service; reboot also restarts the whole server.

## Mobile offline shell

In a browser that supports web-app installation, open ARG over HTTPS and use **Add to Home Screen**. The service worker retains only the shell and same-origin static assets for reopening. It does not cache `/api` routes, authenticated responses, or server operations. Offline mode does not provide offline dashboard or VPS management. Reconnect and reload to see current data.

## If maintenance does not finish

- After repair or reboot, allow time for services to start. Then use SSH to check `sudo arg status` and `sudo journalctl -u arg -n 100 --no-pager`.
- If restore or upload fails, keep the full error and verify that the file is a real ARG SQLite backup. Do not pass it through tools that alter binary contents.
- If you intend to remove ARG and still need its data, stop and download a backup before confirming removal.
- For support, provide the version and error text. Do not publish the database, keys, cookies, or backup file.
