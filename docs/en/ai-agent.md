# Connecting an AI Agent to ARG

ARG provides an MCP-compatible endpoint at `/api/agent/mcp`. It uses a dedicated Bearer token and is available only over HTTPS.

## Secure setup

1. Sign in as an administrator and open “AI Agent Connection” in Server and ARG Management.
2. Create a separate token for each agent. Select only the required scopes; start read-only.
3. Store the token in the MCP client's secret manager. ARG shows the raw token once.
4. Configure the MCP endpoint in the client and read the token from its secret environment.
5. Revoke the token in ARG after use or suspected exposure.

Conceptual client configuration:

```json
{
  "mcpServers": {
    "arg": {
      "url": "https://arg.example.com/api/agent/mcp",
      "headers": { "Authorization": "Bearer ${ARG_AGENT_TOKEN}" }
    }
  }
}
```

## Tools and resources

Depending on token scopes, an agent can read application summaries, deployment job summaries, non-sensitive panel status, bundled Persian/English documentation, and the Rust/React TypeScript source snapshot shipped with this panel version. The `arg://repository` resource points to the public [themrcortex/arg](https://github.com/themrcortex/arg) repository. Source and documentation are read from files embedded in the binary; the panel endpoint does not fetch arbitrary URLs.

With `deployments:propose`, an agent can submit a deployment proposal for an application with a saved default source. The source, health check, release retention, and pre-deployment commands are snapshotted when proposed; later setting changes do not alter it. A proposal does not run anything. An administrator must inspect the app, reason, snapshotted source, and execution impact, then approve it in ARG. Approval creates a job through ARG's normal deployment validation and queue.

The current MCP interface does not provide shell/root execution, secret access, an unrestricted API proxy, resource deletion, network changes, or direct service control. Use the normal ARG interface for those operations.

## Security controls

* Tokens contain 256 bits of randomness; only a hash is stored.
* Tokens expire within 30 days; the UI defaults to seven days.
* Each token has independent scopes and a 30 requests/minute limit.
* Up to 10 active tokens per administrator and 20 pending proposals per token are allowed.
* Tokens are revoked when their owner loses administrator status or is disabled.
* Proposals expire after 30 minutes and can only be approved through an administrator session with Origin validation.
* Responses use `no-store`; creation, revocation, proposals, and reviews are written to the audit log.
* Restricting network access to the panel and protecting MCP client credentials remain administrator responsibilities.

## Troubleshooting

* `401`: The token is invalid, expired, or revoked. Create a new one.
* `403`: The token lacks a required scope or the request is not HTTPS.
* `429`: The 30 requests/minute limit was reached; reduce polling.
* `repository:read` is required for source files and is separate from documentation access.
* Deployment proposals require a saved default source.
* Existing token scopes cannot be upgraded; create a new token with the required scopes.
