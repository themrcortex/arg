# Read-only public API

The first public API release exposes application and deployment summaries to external dashboards and reporting tools. It does not provide a general-purpose administration API.

## Security boundary

- API keys use the `arg_live_` prefix and contain 256 bits of random material.
- ARG stores only a SHA-256 token hash. The raw token is shown once when created and cannot be retrieved again.
- Key management is available to administrators in **Server & ARG management → Public API keys**.
- Each administrator can have up to 10 active keys. Expiry is required and limited to 1–365 days.
- Both token issuance and API use require HTTPS. Do not put a key in a URL, query string, source repository, browser local storage, or logs.
- Every key currently has the fixed scopes `apps:read` and `deployments:read`. Keys are not restricted to a particular project, workspace, or server yet; issue them only to a trusted integration that may read all application/deployment summaries on this ARG instance.
- The service enforces 60 requests per minute per key. The in-memory API responses are not cached by the ARG service worker.

## Create a key

1. Sign in as an administrator over HTTPS.
2. Open **مدیریت سرور و ARG** from the sidebar and find **کلیدهای API عمومی**.
3. Enter a descriptive label and an expiry between 1 and 365 days, then create the key.
4. Copy the token from the one-time dialog into a trusted secret manager. Closing the dialog discards the raw value from the browser state.
5. Send the token only in an `Authorization: Bearer` header.

## List applications

```sh
curl --fail-with-body --silent --show-error \
  -H "Authorization: Bearer ${ARG_TOKEN}" \
  "https://arg.example/api/v1/apps?limit=50&offset=0"
```

Response fields include the application ID/name, framework, declared runtime version, configured domain, desired and observed states, lifecycle state, and update timestamp. The endpoint does not return application environment variables, source configuration, secret references, or credentials.

## List deployments

```sh
curl --fail-with-body --silent --show-error \
  -H "Authorization: Bearer ${ARG_TOKEN}" \
  "https://arg.example/api/v1/deployments?limit=50&offset=0"
```

The response contains job ID, application ID/name, state, source commit, and lifecycle timestamps. Source JSON and detailed error output are deliberately omitted because they may include private repository or environment details.

## Pagination and errors

`limit` defaults to 50 and is clamped to 1–100. `offset` defaults to 0 and is clamped to 0–1,000,000. The response includes `data` and a `pagination` object with the effective limit, offset, and returned count.

| Status | Meaning |
| --- | --- |
| `400` | Invalid request or pagination syntax |
| `401` | Missing, unknown, expired, or revoked key |
| `403` | HTTPS is required or the key lacks the endpoint scope |
| `429` | Per-key limit of 60 requests per minute was exceeded |
| `500` | Internal database or service error; do not retry in a tight loop |

## Revoke and rotate

In the API key table, choose the revoke button for the old key. Revocation takes effect on the next request and cannot be undone. Create a replacement key and update the integration's secret before removing the old credential from that integration. The raw key is not recoverable from ARG.

## OpenAPI document

The machine-readable contract is available without a token at `GET /api/v1/openapi.json`. The document describes the current read endpoints. It does not promise write APIs, webhooks, tenant-specific scopes, or future endpoints.

## Current limits

This API is read-only and applies to the ARG instance that issued the token. There are no write scopes, webhook delivery, key-level project selection, per-tenant limits, audit export, or generated SDK yet. For stronger isolation between customer teams, wait for the dedicated tenancy and role model instead of sharing a broad key.
