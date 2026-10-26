## System information

The System page groups host information, uptime, CPU, RAM, Swap, and mounted filesystems. It distinguishes local, virtual, and remote mounts; repeated mounts of one filesystem must not be counted as extra capacity.

## Disk capacity terms

- **Total:** all blocks in a filesystem.
- **Used:** occupied blocks.
- **Available:** blocks available to an unprivileged user; it can be less than free because some blocks are reserved for root.
- **Reserved/unavailable:** total minus used and available, when it can be calculated.
- **Mount point:** where a filesystem is attached in Linux's directory tree, such as `/`, `/var`, or `/mnt/data`.

`/proc`, `/sys`, `tmpfs`, `overlay`, cgroups, and pseudo-filesystems are not independent physical disks and are excluded from local physical totals. FUSE/S3 mounts appear as remote/cloud storage rather than local capacity. Rclone's local cache still consumes VPS disk space.

## Find what occupies disk space

The disk analyzer ranks directories and files using a bounded scan. A directory includes its descendants, so do not add a parent total to its child totals. Scanning other mounts or bypassing scan limits can consume more time, I/O, and CPU. Virtual directories, changing files, and concurrent writes can affect results.

Read-only SSH alternatives:

```bash
sudo df -hT
sudo du -xhd1 /var /opt /home 2>/dev/null | sort -h
sudo docker system df -v
```

`du -x` avoids crossing into another mounted filesystem. Before removing caches, images, logs, or old releases, verify that no running service depends on them.

## Read/write chart

Read/write rates are shown when block-device counters exist. They are not application network traffic and generally cannot isolate I/O for one site or container. Values may not be directly comparable across reboot or device replacement; rate and capacity are different metrics.

## Cleanup risk

Do not manually `rm -rf` `/var/lib/docker`, `/var/lib/containerd`, databases, `/opt/arg`, or site directories to free space. Identify ownership and service dependencies first; review candidates before using Docker prune commands.

## Reading System and Mounts

CPU, RAM, and disk cards show the latest host reading. Refresh rereads current metrics; history ranges only filter existing samples. CPU is a sampled percentage, RAM is derived from total and available values. Load average includes runnable and I/O-waiting work and is not a percentage.

### Filesystem inventory

In **Disks and Mount Points**, each volume has a mount path, device, filesystem, kind, total/used/available, percentage, read-only state, and status. Filter Local, Cloud, Virtual, or All. Expand a row for measurement time and errors. `statvfs` measures a mount; an S3/FUSE total may be unknown because object APIs do not promise POSIX free-space data. Unknown is not zero.

Summary aggregates only when non-overlapping filesystems can be identified. Duplicate/bind mounts, tmpfs, overlay, `/proc`, `/sys`, and `/dev` should not be counted again as separate physical disks. If the root KPI differs from the total capacity, compare it with `/`; the KPI may show only root when aggregation is unreliable.

### Capacity and I/O trends

The Capacity graph plots used/total for the selected filesystem. Read/Write graphs show disk byte rates and may be unavailable for some mounts/drivers. Interpret data with its sample time; a zero is different from no measurement. Use point tooltips for dates on multi-day ranges.

### Find what fills the disk

Start disk analysis at `/` and expand the largest branches. A parent size already includes descendants; do not add parent and child totals. `du` creates I/O, so avoid repeated scans under load. For `/var/lib/containerd`, use the Docker page and the read-only `docker system df -v`. Do not treat dangling images like volumes or required images; a database volume may contain unique data. Remove caches, old releases, and build outputs only after confirming ownership, rebuild feasibility, and rollback requirements.

Example: a large `/opt/arg/source/target` is likely Rust build output. Fast updates may retain it for subsequent builds; full updates save disk but take longer to compile. Deleting `target` does not directly remove the installed program, but the next update must perform a full build.

### When is cleanup safe?

First distinguish live files, rebuildable cache, required backup, and reproducible build output. Check free space and last measurement; do not start a build/update while root is at 95%. A lower-risk cleanup order is:

1. Use APT clean for `/var/cache/apt`, which contains downloaded package archives.
2. Manage journald with vacuum/retention settings and compare size before/after; preserve required security logs.
3. Inspect `docker system df -v` to identify unused build cache/images; use controlled Docker cleanup and retain volumes.
4. Remove an old application release only after confirming the current version and rollback path.
5. Identify ownership for every large path; `/home/deploy/apps/site1` may contain releases, a database, or real uploads.

`du` walks directory entries and can differ from `df` for hard links, sparse files, deleted-but-open files, or separate mounts. If `df -h` reports a full disk but `du` does not account for it, inspect deleted files still held open by a process or exhausted inodes. Re-deleting an open file does not help; identify its owner and plan a safe service restart.

### Root and cloud mount example

Suppose `/` is at 90% and `/mnt/archive` is an S3 mount with unknown quota. Do not count the cloud mount as physical root capacity; Rclone's local cache under `/var/cache/arg-s3` does consume root. Copying to S3 frees no local space until you verify the copy and deliberately remove the local source. A safer sequence is copy → verify size/checksum → switch application reads → observe → controlled local deletion.

### I/O and Load

If CPU is low but requests are slow, inspect Read/Write trends and paths such as `/var/lib/mysql`, `/var/lib/docker`, or noisy logs. Read/Write is a sampled rate; range totals come from retained history. Null values can mean the kernel/driver does not expose counters. Do not change storage backends without downtime planning, backup, and mount dependency checks.
