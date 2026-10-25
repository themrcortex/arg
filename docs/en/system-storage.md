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
