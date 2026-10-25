# ARG S3 Storage Foundation

This document describes the currently implemented S3 and disk-storage foundation in ARG. It does not claim that every provider offers every S3 operation.

## Included in this release

- Local, remote, cloud/FUSE and virtual mount inventory read from Linux mountinfo and measured with `statvfs` where applicable. Duplicate block-device mounts are excluded from totals; an ambiguous aggregation falls back to the root filesystem KPI.
- Admin-only S3 connection setup for AWS S3, Cloudflare R2, ArvanCloud (Simin/Shahriar and custom endpoint), ParsPack (endpoint entered from the account panel), Backblaze B2, DigitalOcean Spaces, Wasabi, MinIO and generic S3-compatible endpoints.
- Endpoint syntax and DNS checks, HTTPS-only endpoints, private endpoint opt-in, validation of bucket names and pagination limits.
- Provider credential files stored outside SQLite under `/etc/arg/s3`, root-owned with `0600` file and `0700` directory permissions. Connection APIs return only whether credentials are present, never the secret itself.
- Connection test, provider Bucket listing (only on explicit request), local Bucket association, capability-gated Bucket creation, paginated object listing, upload (maximum 512 MiB), download (maximum 2 GiB) and confirmation-protected Object deletion.
- File manager ZIP creation for regular files/directories and ZIP extraction into a new folder. Operations reject symlinks, special files, unsafe archive paths and overwrite attempts; archive input and expanded data are capped at 2 GiB and 50,000 entries.

## Current limits

S3 mount lifecycle (Rclone/S3FS), scheduled copy/sync jobs, usage scans and quota snapshots, contract CRUD/renewal reminders, provider billing, object copy/move, multipart uploads, bucket deletion and advanced bucket policies are not implemented in this release. The summary deliberately reports cloud usage and quota as unknown. MinIO server deployment is a separate future feature. Do not use this release as a replacement for an independent backup system.

The Rust S3 client operations use the privileged helper and fixed action names; no user-supplied shell command is executed. Upload and download use private-mode temporary files and stream file data instead of base64 JSON. S3 secrets are not included in API responses, logs, source files or command-line arguments.

## API routes

- `GET /api/storage/volumes`
- `GET /api/s3/providers`, `GET /api/s3/summary`
- `GET|POST /api/s3/connections`
- `GET|PUT|DELETE /api/s3/connections/{id}`
- `POST /api/s3/connections/{id}/test`
- `GET|POST /api/s3/connections/{id}/buckets` (append `?local_only=1` to read only ARG associations)
- `POST /api/s3/connections/{id}/buckets/create`
- `GET /api/s3/connections/{id}/objects`
- `POST /api/s3/connections/{id}/objects/upload`
- `GET /api/s3/connections/{id}/objects/download`
- `POST /api/s3/connections/{id}/objects/delete`
- `DELETE /api/s3/buckets/{id}` (disconnects the ARG association only; does not delete the remote Bucket)

Write operations require an authenticated administrator, same-origin validation where applicable, and audit events.
