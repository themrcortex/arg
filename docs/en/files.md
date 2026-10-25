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
