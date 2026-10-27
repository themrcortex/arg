# Cloudflare DNS management

This guide covers ARG's **DNS management** page. It connects a Cloudflare zone with a scoped token and reads or changes records directly in that zone. A bad record can take a site or email offline, so note the current value in Cloudflare before changing it.

## Access and prerequisites

- The page is available only to an ARG **administrator**.
- The server needs outbound HTTPS access to the Cloudflare API.
- The domain zone must belong to an account the token can access. Enter the apex zone such as `example.com`, not a full URL such as `https://example.com`.
- For least privilege, restrict the token to that zone and grant `Zone:Read` and `DNS:Edit`. Do not use a Global API Key or a token covering every zone.

In Cloudflare, create a custom token under **My Profile → API Tokens**. See Cloudflare's [API token guide](https://developers.cloudflare.com/fundamentals/api/get-started/create-token/) for current instructions.

## Connect a zone

1. In ARG's navigation, open **DNS management** and select **Connect Cloudflare**.
2. Enter a local connection label so you can distinguish zones in ARG.
3. Enter the zone name, for example `example.com`, and its API token.
4. Select **Check and connect**. ARG asks Cloudflare to find the zone and verify token access. If the zone is unavailable or the token lacks permission, ARG does not save the connection.

After connection, the token is not shown again in the interface or API response. ARG stores it encrypted in a private local file. At-rest encryption does not replace securing the VPS itself.

## View records

Select a connected zone to view its records. The table shows type, name, content, TTL, and proxy status. The current interface requests only the first API page, with up to 100 records, and has no UI pagination. If the zone has more than 100 records, this list is incomplete; use the Cloudflare dashboard for a full inventory.

## Create a record

Select **New record** and choose `A`, `AAAA`, `CNAME`, `MX`, or `TXT`. The interface checks the input, and the API also restricts the record name to the selected zone.

- Use `@` for the zone apex, or enter a fully qualified name such as `www.example.com` for a subdomain. The record name must belong to the selected zone.
- `A` requires a valid IPv4 address and `AAAA` a valid IPv6 address. A `CNAME` value must be a hostname.
- `MX` requires a target and priority. Coordinate mail-related values with whoever operates your email service.
- A TTL of `1` means **Automatic** in Cloudflare. Manual TTL values must be from 60 to 86,400 seconds.
- The interface offers Cloudflare proxying only for `A`, `AAAA`, and `CNAME`; do not enable it for email or `TXT` records.

Confirming the form sends the change directly to Cloudflare. ARG does not stage the record or verify its propagation through public DNS.

## Delete a record or disconnect

Deleting a record removes it directly from the live Cloudflare zone. `MX` and `TXT` records may be required for email, SPF, DKIM, or DMARC; verify dependencies before deletion. The current interface supports viewing, creating, and deleting records; editing an existing record is not available there.

**Disconnect Cloudflare** removes only the credential stored by ARG. It leaves the zone and its Cloudflare records unchanged. If ARG should no longer use the token, revoke it in Cloudflare as well.

## Verify a change and troubleshoot

After a change, query the result from a device with DNS tools, for example `dig A www.example.com`. Resolvers may keep the previous value until its TTL expires. This page does not perform propagation checks, DNS-01 certificate issuance, registrar nameserver changes, or management for other providers. Do not confuse it with automatic SSL in ARG's Nginx tools.

- Access or zone error: verify the zone name and the token's `Zone:Read` and `DNS:Edit` permissions.
- Invalid value: check the record type, IP or hostname format, name within the zone, TTL, and `MX` priority.
- Timeout or uncertain response: before retrying a create/delete action, inspect the Cloudflare dashboard to see whether the previous request took effect.
- Stale DNS answer: check the record in Cloudflare and more than one resolver; results depend on cache and TTL.

For Nginx `server_name` and TLS setup in ARG, see [Nginx and websites](?page=nginx). That page does not change DNS records itself.
