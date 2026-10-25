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
