## Panel transport

Enable TLS for public use. Installation without SSL is permitted, but public HTTP sends usernames, passwords, and cookies without transport encryption. Use an SSH tunnel or private network until a valid certificate is active. The installer does not force TLS and continues when certificate issuance fails; this preserves installation, but does not remove HTTP risk.

## Accounts and sessions

Use a long, unique password. Do not share setup/reset codes publicly. Sign out on shared devices and revoke unknown sessions. Root Terminal requires administrator re-authentication; use it only for necessary commands.

## S3 and TLS keys

- S3 Secret Access Keys are not returned to the frontend by the API.
- Provider credential files on the server should only be readable by root.
- Never commit a TLS private key to Git or paste it into tickets, logs, query strings, or screenshots.
- Before adding a private S3 endpoint, verify its hostname and network destination; an internal endpoint could create SSRF risk.

## Firewall and network

The provider firewall is separate from UFW. Before changing policy, allow SSH and panel access and keep a second session. Do not blindly enable UFW over SSH. ARG's protected ports do not control external ACLs or datacenter routes.

## High-impact operations

File deletion, database changes, Docker volume removal, bucket actions, unmount, firewall changes, package purge, reboot, and rollback have different effects. Read confirmations and plan recovery for production services. `sudo` and Docker access can grant broad host control.

## Support data

Systemd logs and API errors can include endpoint, username, or paths. Before sharing logs, remove hostnames, public/private IPs, customer domains, credentials, personal paths, and SQL data. Public docs do not require ARG telemetry.

## Assurance limits

ARG is an administration tool; it does not guarantee custom configuration correctness, application compatibility, website-code security, or backup quality. A security review document is a risk assessment, not independent certification or a penetration-test certificate.
