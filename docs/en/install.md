## Requirements

- Debian or Ubuntu VPS with `systemd` and `root` or `sudo` access.
- Network access to GitHub, distribution package repositories, and build dependencies.
- Enough free space for the Rust toolchain and release compilation. Temporary build size depends on target, dependencies, and cache.
- The panel port must be reachable through both the OS firewall and the provider firewall. HTTP-01 certificate validation also needs inbound TCP port 80 to be reachable temporarily.

Debian/Ubuntu with systemd is the official target. Other distributions may work, but are outside the tested release guarantee.

## Quick install without cloning

Download the official installer and run it as root:

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  https://raw.githubusercontent.com/themrcortex/arg/main/install.sh -o arg.sh
sudo bash arg.sh install
```

To open the installation and management menu instead, omit `install`:

```bash
sudo bash arg.sh
```

The installer installs OS dependencies, prepares stable Rust, fetches the selected source branch, builds the embedded frontend and Rust backend, creates the restricted `arg` service account, and configures `arg.service` and `arg-privileged.service`.

> The installer runs with root privileges. Verify the repository URL and installer contents before running it. Do not execute a remote script on a server you do not trust.

## Installation behavior

The installer detects the panel port and attempts to discover the public IPv4 address, then prints an access summary. Public IP detection can fail because of egress restrictions or unavailable IP lookup services; that alone does not mean the panel's local listener is broken.

### SSL is optional

Enter `skip` at the HTTPS address prompt to continue without a certificate. ARG installation does not depend on certificate issuance. If Let's Encrypt and the optional ZeroSSL fallback fail, installation continues over HTTP and TLS can be configured later. HTTP does not encrypt passwords or session cookies; until valid TLS is active, access the panel only through an SSH tunnel or trusted private network.

ZeroSSL ACME issuance for a domain requires EAB KID and HMAC credentials and does not issue certificates for bare IP addresses. ZeroSSL is optional. If credentials are unavailable or network access fails, decline the fallback and configure a compatible certificate later in panel settings.

## Secure access through an SSH tunnel

Keep this tunnel open on your workstation:

```bash
ssh -N -L 8282:127.0.0.1:8282 <SSH_USER>@<SERVER_IP>
```

Then browse to `http://127.0.0.1:8282` on that workstation. The right-hand port must match the panel's server-side port. Update both ends if the panel port changes.

## Create the first administrator

- **Public HTTPS:** enter the one-time setup code printed by the installer or available from the `arg` menu.
- **SSH tunnel:** open the local forwarded page and enter the server's one-time code.

The setup code is only for creating the first administrator and cannot be reused after one exists. Do not share it in public channels.

## Verify the installation

```bash
sudo arg status
sudo systemctl is-enabled arg
sudo journalctl -u arg -n 100 --no-pager
```

The service should be active and the login page should answer on loopback. To expose it remotely, open the actual panel port in the OS and provider firewalls. For public access, enable HTTPS as a priority.
