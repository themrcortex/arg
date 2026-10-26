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

## Users and roles

Review username, role, and enabled state in the user list. Admin can perform sensitive management operations; Viewer has only permitted read capabilities. Keep at least one reachable administrator before changing roles. Disabling or removing a user may invalidate that user's session.

## Audit log and its limits

Audit records actor, action type, time, result, and safe details. Use a time filter and expand an event. Audit is not a substitute for Nginx access/error logs, APT output, or `journalctl`; each answers a different question. Credentials must never be placed in events. Before sharing output, check for IPs, filenames, usernames, and accidental tokens.

Example firewall investigation: filter around the change time, inspect the actor and rule event, then compare the current UFW rule in Firewall. A successful event means the request ran; validate actual external access with a probe and real connection.

## TLS and root access

If the panel is served over public HTTP, passwords and session cookies are not encrypted between browser and VPS. After obtaining a certificate, enable panel TLS and close or securely redirect the panel's HTTP port according to configuration. A certificate on an Nginx website does not imply the ARG panel itself uses HTTPS. Restrict Root Terminal and helper actions to administrators; session authorization is not the same as root authorization. Hide TLS private keys, S3 secrets, DB passwords, and tokens in logs/screenshots.
