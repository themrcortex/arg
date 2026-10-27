# Application networks, persistent data, S3, and logs

This page describes what the application resource controls actually do. Open an application card and expand **Persistent storage, network, and S3**. These APIs are admin-only and important changes are audited.

## Application networks

### Docker private bridge

Enter a name such as `backend-private` and choose **Docker private bridge**. ARG creates a generated, ARG-owned network with `--internal`; it has no public external route. To attach a container, create it from the Docker page and select the Docker-backed ARG application. ARG adds the `com.arg.application_id` label. Then choose the network, container, and alias from the app card. The privileged helper checks both the network ownership label and the container's app label.

ARG does not attach arbitrary containers by matching a name. A container not created with the selected ARG application ID must be recreated from that application. Detach all members before deleting a network; deletion is blocked while members remain.

### Loopback for native services

For a Node/Next/Nuxt service running under systemd, **Loopback** records an alias for `127.0.0.1:<port>`. The port must match the app's configured port. This is single-host service-discovery metadata. It does not put the process in Docker, create a firewall or namespace, or isolate one app from another.

## Persistent application data

Create a named volume such as `uploads`. Inside the Node service, its stable path is `/shared/data/volumes/uploads`. The data lives outside release directories and survives deployment and rollback. An app-specific group grants write access while the parent directory remains protected from replacement by the app process.

**Measure** runs an explicit scan capped at 100,000 entries; symlink targets are not followed. Repeated scans can cause disk I/O. An optional quota is an administrative note only: ARG does not configure or enforce a kernel filesystem quota.

**Detach** removes the ARG record and retains files. **Delete data** requires a separate confirmation and removes the directory. The helper refuses a volume on a separate mounted filesystem. Back up data before purging it.

## S3 Bucket binding

Choose an existing Bucket and a prefix such as `customer-a/assets/`, then select a purpose: app files, backup, artifact, assets, or an existing mount. `rclone-mount` is accepted only when ARG sees an active mount for that Bucket.

The binding is a safe reference to an existing ARG connection. Credentials are not copied into app APIs or returned to the browser. Read/write mode in ARG is metadata and does not change S3 permissions. For real isolation, create a provider key restricted to the app's Bucket/prefix. Check the mount options and filesystem permissions when consuming a mounted path.

An S3 connection cannot be deleted while app bindings reference it. Removing a binding never deletes a Bucket or object.

## Node service logs

**Show runtime logs** reads recent entries from this app's systemd unit. Each request is capped at 500 lines and returns a cursor for resuming with newer entries. Closing the browser or restarting ARG does not remove journald entries.

Retention follows the host-wide journald policy, not a per-app setting. `journalctl --disk-usage` reports total journal use; vacuum operations affect multiple services and are not run from this app card. Avoid printing secrets from the application.

## API and deletion behavior

- `GET/POST /api/app-networks`: list or create a network on the current server.
- `GET/POST /api/app-networks/{id}/members`: view or add members.
- `DELETE /api/app-networks/{id}/members/{app_id}`: detach a member.
- `GET/POST /api/apps/{id}/volumes`: list or create persistent data paths.
- `POST /api/app-volumes/{id}/scan`: run a bounded usage scan.
- `DELETE /api/app-volumes/{id}`: remove metadata while keeping files.
- `POST /api/app-volumes/{id}/purge`: purge files after explicit confirmation.
- `GET/POST /api/apps/{id}/s3-bindings` and `DELETE /api/app-s3-bindings/{id}`: manage Bucket references.
- `GET /api/apps/{id}/node/service/logs?limit=100&after=...`: page through journald using the previous cursor.

## Host integration boundary

Automated checks cover validation, schema, and type checking. Docker networking, Linux group permissions, S3 mount permissions, and journald cursor behavior still need a Debian/Ubuntu integration host. Cross-VPS application networking is outside this feature.
