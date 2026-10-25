## Panel does not load

1. Run `sudo arg status`; if the service is stopped, start it and inspect the journal.
2. Check the actual port with `sudo ss -lntp`.
3. Test on the VPS: `curl -v http://127.0.0.1:8282/api/state` or the configured port.
4. If loopback works but remote access fails, inspect provider firewall/security group, UFW, public bind, and routing.
5. For HTTPS, inspect certificate SAN for the hostname/IP, TLS port, proxy headers, and VPS clock.

## First administrator cannot be created

Use the same origin as the login page. If the page says only a local tunnel is allowed, enable HTTPS or keep an SSH port-forward open. Retrieve a current one-time code from the same server CLI; old or cross-server codes are rejected.

## UFW is unavailable or inactive

The page distinguishes installation, package, and service state. Installing the tool does not necessarily enable UFW. Allow SSH and the panel before enabling it. If `ufw` is missing, check APT/distro; restarting ARG does not install UFW.

## Nginx or site TLS

```bash
sudo nginx -t
sudo systemctl status nginx
sudo journalctl -u nginx -n 100 --no-pager
sudo ss -lntp | grep -E ':80|:443'
```

A site certificate and the ARG panel certificate are separate configurations. If Nginx was stopped for ACME and was not restored, start it manually after checking `nginx -t`.

## Update is slow or appears stuck

Rust builds can take several minutes. Use spinner/elapsed time/logs and authoritative server state rather than a fixed browser timeout. Do not start a second update. Check from SSH and request cancellation only if the process is actually stuck. If the new binary cannot start, follow rollback and log output.

## MySQL

For an option parsing error, use `--defaults-extra-file=/path/file.cnf`. `mysql` and `mariadb` clients differ by release; capture full output but redact credentials. If the access gate is locked, a current panel password and Admin role are required.

## S3

- Connection test: verify DNS, TLS, exact endpoint, signing region, and key permissions.
- Bucket list is empty: `ListBuckets` may be denied; test a known Bucket directly.
- Quota unknown: the provider may not expose it; enter contractual quota manually if known.
- Usage seems old: scans are cached up to 24 hours; request a manual scan and monitor its state.
- Mount failed: check Rclone, FUSE, credential file, systemd unit, cache space, and safe/empty mount path.
- Transfer stopped: keep the source, inspect job status/logs, check partial destination data, then retry.

## Contact support

Provide version, commit, distro, safe reproduction steps, and the error. Do not publish secrets, setup codes, session cookies, private keys, customer databases, or raw logs. Check existing issues and changelog first.
