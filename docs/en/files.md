## Browse server files

The file manager shows a server path and supports folder navigation, name search, file/folder creation, text editing, upload, and download. Paths refer to the server; a site's display name is not its document root. Read the Nginx root before opening a site's files.

## Operations

- **Copy/Paste:** copy a file or directory to a chosen destination. Confirm destination names to avoid overwriting important data.
- **Cut/Move:** relocate a file or directory. Across filesystems this can become copy-then-delete and take time.
- **Rename:** change a name within a directory.
- **Delete:** remove a file or directory tree. It is not moved to Trash and is not reversible.
- **Upload/Download:** transfer between browser and server; size and timeout depend on server and browser limits.
- **Edit:** modify text files. Do not edit binary files. Back up a live configuration first.
- **ZIP:** create an archive from selected files or folders and download it.
- **Unzip:** extract into a new folder. Path traversal, symlinks, overwrite, and excessive archive size are rejected.

The current release enforces archive size and entry-count limits. A limit error does not necessarily indicate a corrupt ZIP file. For very large files, use SSH or rsync.

## Security and permissions

The file manager is powerful. Change keys, `/etc`, databases, and service files only when necessary. A symlink may point outside the visible path; some destructive/ZIP operations reject symlinks, but always verify the real destination. Wrong permissions can stop a service or expose secrets.

## Transfer a directory to S3

Use the S3 page's VPS-to-Bucket transfer. Choose a path, destination, and review object count/size. The transfer copies data; do not delete the source until you have checked the destination listing and files. Moving a live service or container to object storage is not a consistency-safe application migration.

## Step-by-step file manager guide

The **File Manager** browses the VPS filesystem under the authenticated ARG account. The path field is LTR. Enter an absolute path such as `/var/www/example` and choose **Go**; **Parent** moves up one directory. Access is constrained by the helper's allowed paths and permissions. Seeing a path in the UI does not mean you are authorized to read or modify it.

The listing shows name, type, size, permission, modification time, and actions. Click a directory to enter; open a text file in the editor; never confuse a symlink with its target. Do not save binary data in the text editor. Copy/back up a config before editing. Refresh only rereads the listing.

### File and directory actions

- **Create file/folder:** verify the parent and choose a simple relative name before confirming.
- **Upload:** data flows from your browser to the VPS; respect size/time limits. For large files, use direct VPS↔S3 transfer or SCP/rsync.
- **Download:** retrieves the selected server file. Inspect a symlink target first.
- **Rename/Move:** verify the complete destination. Moving across filesystems may be Copy/Delete and is not atomic.
- **Copy/Cut/Paste:** Copy or Cut enters the selection in the app clipboard; Paste targets the current directory. Check the destination carefully before Cut.
- **ZIP:** archive selected files/folders and download the result. If an archive is created inside its own source tree, a later scan may include it again; use a separate destination such as `/tmp/arg-exports`.
- **Unzip:** inspect archive and destination first. Extraction must remain inside the target; avoid unintended overwrites. Check free destination capacity before extracting.
- **Delete:** treat as irreversible. Review path and item count; do not delete service roots, sensitive symlinks, or mount points.
- **Permissions:** grant only necessary access; `777` is not a general fix. Check the web-service user/group so files are both secure and readable.

### Disk usage analysis

The analysis section accepts `/` or another starting path and lets you expand subdirectories. Each row is the recursive size of that path, so a parent includes its children. Adding parent and child sizes double-counts data. Start at the largest branch and expand it until you identify a large file/cache. `du` can consume time and I/O; avoid repeatedly scanning `/` under load, and do not confuse `/proc`, `/sys`, `/dev`, or cloud mounts with local disk.

Example: if `/var/lib/containerd` is large, drill into `.../io.containerd.snapshotter.v1.overlayfs`, compare with Docker's page and `docker system df -v`, and identify active images/containers before cleanup. Never manually remove active containerd snapshots or build cache. For logs, inspect `journalctl --disk-usage` and rotation instead of truncating a file currently being written.

### Safe ZIP and restore workflow

Select the source folder and a destination outside it; preserve the source if the operation fails. After ZIP creation, inspect its size/list and download it. To restore, upload the ZIP, navigate to the destination, and extract; then compare the extracted paths and item count with expectations. A ZIP is not a database backup or service snapshot.

### Permissions and editing

Modes are shown in octal: for example, `0644` allows owner read/write and others read; `0755` on a directory permits traversal. Coordinate mode, owner, and group with the service. PHP-FPM/Nginx does not need write access to every project file. Only writable directories such as `storage` or `uploads` should be writable. Before changing `nginx.conf`, `.env`, or a unit file, download/copy the old version and inspect a diff.

The editor is for bounded text/config files, not SQLite databases, images, archives, large files, or binary content. A large file may be limited. After saving, check metadata and syntax: `nginx -t` for Nginx; a parser for JSON; format and permissions for env files.

### Transfer directory example

To move `/var/www/site/public/uploads` to S3, scan the path and identify symlinks first. Copy to a unique prefix such as `site-assets-2026/`, compare item count/size, configure the application to use the object adapter, then test reads/writes. Archive the local source only after verifying cache/CDN/permissions. Do not move sessions, transient cache, or a live database as an ordinary ZIP transfer.
