# Pairing servers with scoped access

## Requirements

Both installations should be up to date. The destination panel must be reachable by the controller over HTTPS with a trusted certificate. Check system time, DNS, and network routing between the VPSs. A public IP is not required when using a private network; enable the private-target option only for that trusted destination.

## Create a code on the destination

Open **Nodes → Allow management of this server**. `Read-only` grants no service or data changes. `Full` is for a trusted administrator and also grants workspace and terminal access. Full access does not send a browser token; the destination checks every scope on the server.

Create a one-time code and deliver it to the controller administrator through a secure channel. The code expires after five minutes and is consumed by the first successful pairing. Enter the destination base URL such as `https://panel.example.net`; do not include `/api`, a query string, or credentials in the URL.

## Register the destination on the controller

In **Nodes → Connect this panel to another panel**, enter a local label, endpoint, and code. The label is only for this controller. Enable the private-network option only for private IP addresses or internal DNS names. Loopback, link-local, and metadata targets remain blocked.

After pairing, the destination card shows public name, connection health, compatibility, and granted access. Select the workspace from the top bar to open destination pages. Before a mutating operation, verify the active server title again.

## Troubleshoot pairing

- DNS error: resolve the destination from the controller host and check the returned address.
- HTTPS error: the destination certificate must be trusted and match the endpoint hostname; HTTP redirects are not followed.
- Invalid code: create another code if the current one expired or was consumed.
- Incompatible version: update both panels, review access, then pair again.
- Private destination: enable the private-network option on the controller. Do not enable it for a public endpoint.

Revoking a controller credential does not delete files or services from the destination VPS.
