## Requirements and detection

The page detects installed Docker Engine and Compose. Opening it should not automatically install Docker. If unavailable, follow the dependency status and installation action in the manager.

## Docker resources

- **Container:** isolated process based on an image; status, uptime, port mapping, and reported resource usage.
- **Image:** read-only template. Removing an image with references can be rejected.
- **Volume:** persistent data outside a container's writable layer. Removing it can destroy database or user data.
- **Network:** connectivity and internal DNS between containers; removing one can break service communication.
- **Compose:** a group of services defined by a Compose file; `down`, `pull`, or `up` can recreate containers.
- **Build cache:** BuildKit layers that use disk but can accelerate later builds.

## Management actions

The page reads Docker status/resources and reports action progress/results for operations supported by the current UI. Common container actions include start/stop/restart/logs; image, volume, network, and Compose actions depend on current release support. Confirm project, service, image, and path before acting: Docker management usually grants near-root access to the host.

## Disk usage

`docker system df -v` separates images, containers, volumes, and build cache. `docker system prune` can delete unused images or networks. `docker compose down -v` also removes project volumes. Do not manually delete `/var/lib/docker` or `/var/lib/containerd`.

## Usage and limits

Container usage comes from Docker daemon or cgroups and depends on Engine version, accounting, and runtime. A usage number is not a configured resource limit. Use container/Compose resource settings or an appropriate systemd unit to enforce limits, while considering effects on persistent data and service availability.

## Diagnostics

```bash
docker info
docker ps -a
docker compose ls
docker system df -v
journalctl -u docker -n 100 --no-pager
```

If the Docker socket is unavailable, inspect `docker.service`, permissions, and group membership. Membership in the `docker` group effectively grants root-level control.

## Tabs and operations

Page data uses a short cache to make navigation lighter; Refresh replaces the snapshot with a new request. While an operation is busy, do not repeat conflicting actions. A running Docker Engine does not mean every container is healthy.

- **Containers:** inspect name, image, state, ports, and Start/Stop/Restart/Inspect/Logs actions. Stop may interrupt active requests. Before removal, inspect attached volumes and bind mounts. Important data usually lives in volumes, and deleting a volume can be irreversible.
- **Images:** pull, list, remove, or tag images. Pull uses network and disk. Docker refuses to remove an image in use; removing an image is not the same as removing a database volume.
- **Networks:** inspect Docker networks; a network cannot be removed while containers remain attached.
- **Volumes:** persistent storage. Inspect consumers and back up databases before removal. Measuring a volume may take time and require permissions.
- **Compose:** view/run a Compose file managed by ARG. Check the file and project name before `up`; it can recreate or restart multiple services. Do not expose secrets in UI or shared logs.

### Create a simple container

The form accepts **Name**, **Image**, **Port mapping**, Environment, Network, and Restart policy. Example: name `web-demo`, image `nginx:alpine`, mapping `127.0.0.1:8081:80`, restart=`unless-stopped`. Binding to `127.0.0.1` prevents direct public access and is often appropriate behind Nginx. Mapping `0.0.0.0:8081:80` binds all interfaces; check UFW and whether public access is intended. Environment metadata may be inspectable; do not put database passwords there unless you have a protected secret mechanism.

### Build, run, and cleanup

Build uses a **Context directory** and tag. Files in that context may be sent to Docker, so add `.dockerignore` entries for `.env`, `.git`, dumps, and `node_modules`. Use a clear tag such as `local/my-app:latest`. Build cache can consume substantial disk; inspect Images/`docker system df -v` before pruning. Prune may remove reusable layers and increase later build time. Never select volume cleanup without understanding its data impact.

### Troubleshooting example

If a container exits, inspect Logs and Exit code, then check environment, mount path, port conflicts, and healthcheck. Repeated restarts without fixing the cause only create a loop. If a service is unreachable externally, compare `docker ps` port mapping, host listener, firewall, and Nginx upstream. Dashboard CPU/RAM are host totals; use Docker and the container's cgroup for container-level usage.

## Understand state and preserve data

An **Image** is an immutable container template; a **Container** is its running instance; a **Volume** is persistent data; a **Bind mount** is a host path; a **Network** connects containers. Removing a container does not necessarily remove its image; removing an image does not delete a volume unless the explicit operation targets it. Before upgrading, note the current image tag and back up data; `latest` is not a pinned release identifier.

Read status and healthcheck separately. `Up` means the main process still runs; `healthy` reflects a healthcheck. Repeated restart count can mean a crash loop. Logs have timestamps/context but can contain connection strings or tokens, so redact before public sharing. Inspect shows bound ports, mounts, command, and image; treat Environment as sensitive.

### Compose and version changes

Compose projects are identified by file and project name. Before Start/Apply, review `ports`, `volumes`, `environment`, `restart`, `depends_on`, and image tag because they affect the host. Relative volume paths generally resolve from the Compose file directory. Changing a volume path may create an empty new database and look like data loss. Record current volume paths and backup before update; afterward verify healthcheck and the Nginx connection.

### Disk usage

Docker image layers, writable layers, volumes, and build cache are separate. Read `docker system df -v` to find the source. A small container can have a multi-gigabyte database volume. Build cache reuses prior work; deleting it is not runtime data loss, but slows the next build. Image prune targets unused images; prune-all and volume cleanup have a wider impact.
