# 04 — Federation and multi-server connections

## Scope and revisions

- Starting revision: audit branch after phase 3, `0f412f0a5211c4a2108f878af6d202cf098314a3`.
- Source areas reviewed: `src/federation.rs`, Federation routes in `src/routes.rs`, schema/migration setup in `src/main.rs`, and the persisted-token helpers used by Federation.
- No production panel, remote Node, database, or user credential was contacted. Address-validation tests use literal `SocketAddr` values only.
- This phase covers connection pairing, endpoint validation, credential storage and use, controller authorization, workspace proxy and terminal forwarding, status identity checks, unpair/revoke behavior, credential rotation, and update forwarding.

## Architecture and trust boundaries

1. A local administrator supplies an HTTPS panel URL, a five-minute pairing code, and an explicit private-network choice.
2. The controller resolves the peer and validates every resolved address before the first handshake. The request is pinned to the validated address set while TLS continues to authenticate the URL hostname.
3. Pairing exchanges scoped controller and workspace tokens. Raw controller/workspace tokens are written to per-node secret files under the protected node-secret directory; the database holds node metadata, expiry, scopes, and token-file references rather than raw tokens.
4. Later Federation requests revalidate the endpoint and DNS result, pin the current request, require HTTPS/TLS 1.2 or newer, disable redirects, and send a Bearer token only after scope checks.
5. The destination hashes the controller credential, checks enabled state, expiry, negotiated scope, and updates last-seen. Workspace proxy requests have separate read/write scopes. Root-terminal requests require the distinct root-terminal scope and existing local root-ticket flow.
6. Revoke deletes the controller grant. Credential rotation keeps a protected pending credential to recover from a lost response. Remote status data cannot replace an already-bound stable Node ID/server UUID.

## Federation authorization matrix (implementation summary)

| Operation family | Local caller gate | Remote gate / scope |
|---|---|---|
| Pair a Node, rename/remove a Node, change public name, rotate or revoke credentials | Authenticated local administrator; write operations check request Origin | Pair code is one-use, hashed, and expires after five minutes; subsequent requests use scoped controller token |
| Read Node status and services | Authenticated local administrator, then negotiated per-Node scope | `node.status.read` / `node.services.read` |
| Change remote service state or run remote updater | Authenticated local administrator plus negotiated write/update capability | `node.services.manage` or `node.update.manage`; legacy peers do not gain write permission |
| Proxy remote workspace HTTP | Local administrator; safe-path allowlist; scope/capability check | Workspace cookie is checked against `federation_proxy_sessions`, expiry, and method-specific `workspace.read` / `workspace.write` |
| Proxy remote terminal | Local administrator, WebSocket Origin/Host check, requested mode scope | `workspace.terminal` or `workspace.root_terminal`; a normal terminal ticket does not grant root mode |
| Accept a pairing / controller call | HTTPS transport required | Hashed controller credential, enabled grant, expiry and operation-specific scope |

This summary is not the project-wide API permission matrix; see [02-auth-security.md](02-auth-security.md) for the shared-route inventory.

## Findings

### FED-001 — Cloud metadata addresses were reachable through private endpoint opt-in

- **ID:** FED-001
- **Title:** Explicit private-endpoint mode could resolve a saved Node URL to cloud metadata services.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High; address classification and the credential-bearing request path were inspected, and pure validation tests exercise both affected address families.
- **Module:** Federation endpoint validation and outbound Node client
- **File:** `src/federation.rs`
- **Function:** `validate_endpoint`, `address_is_private`, `resolve_endpoint_addresses`, `remote_node_request_with_timeout`, `proxy_workspace_api`
- **Commit SHA:** Baseline `0f412f0a5211c4a2108f878af6d202cf098314a3`
- **Affected behavior:** When a trusted private endpoint was explicitly allowed, Alibaba's `100.100.100.200` was not classified as private or link-local. AWS's `fd00:ec2::254` was private but accepted by that opt-in. Subsequent Node calls resolve the configured URL again and attach the stored controller/workspace credential, so the address policy must reject metadata endpoints regardless of the private-network preference.
- **Technical evidence:** Before the change, `validate_endpoint` rejected loopback/link-local and selected metadata hostnames but not these literal addresses. `address_is_private` covered RFC1918 and selected special ranges, not the IPv4 shared-address range `100.64.0.0/10`. `resolve_endpoint_addresses` allowed private results when the explicit option was true. Outbound operations then attached the stored Bearer token or workspace cookie.
- **Safe reproduction:** `federation_blocks_cloud_metadata_even_for_explicit_private_networks` checks IPv4 metadata, AWS IPv6 metadata, and IPv4-mapped metadata and verifies that private mode does not allow them. `federation_private_endpoint_opt_in_does_not_allow_cgnat_by_default` checks shared-address-space behavior. `node_endpoint_requires_a_clean_https_origin` checks literal metadata URL rejection. These tests open no socket.
- **Root cause:** The private-network opt-in was treated as permission to reach every non-loopback private address, without a provider-metadata denylist; CGNAT was also absent from the private classification.
- **Security/functional impact:** A saved peer hostname resolving to a metadata address could receive credential-bearing requests when private destinations were enabled. The same omission also made private-address classification incomplete.
- **Proposed fix:** Hard-block known cloud metadata addresses independent of the private-network option, and classify IPv4 shared address space as private so it requires explicit opt-in.
- **Implemented change:** Added a metadata-address predicate for `100.100.100.200`, `fd00:ec2::254`, and IPv4-mapped forms; apply it during initial endpoint validation and every resolved-address validation. Added `100.64.0.0/10` to private classification. Existing HTTPS-only, TLS verification, redirect denial, and per-request DNS pinning remain active.
- **Tests added:** `federation_blocks_cloud_metadata_even_for_explicit_private_networks`; `federation_private_endpoint_opt_in_does_not_allow_cgnat_by_default`; metadata URL assertions in `node_endpoint_requires_a_clean_https_origin`.
- **Regression risks:** Federation to a service intentionally hosted at a blocked metadata address is no longer possible. Such an endpoint is not a supported Node topology. CGNAT peers now require the existing explicit private-network option.
- **Commit:** `6852cdd fix(federation): block cloud metadata endpoints`.
- **Remaining risk:** Cloud providers may introduce additional metadata addresses. This guard covers the documented addresses currently in scope; deployment testing against real cloud networks was intentionally not performed.

### FED-002 — Workspace session renewal has no issuance rate limit

- **ID:** FED-002
- **Title:** A valid controller can request repeated workspace sessions without a per-controller issuance limit.
- **Severity:** P3 / Low
- **Status:** AUDIT_REQUIRED
- **Confidence:** Medium; the handler inserts a fresh `sessions` row and `federation_proxy_sessions` row for every authorized request. Abuse impact and a compatible idempotency/rate-limit policy need a focused follow-up.
- **Module:** Federation workspace-session renewal
- **File:** `src/federation.rs`
- **Function:** `renew_workspace_session`
- **Commit SHA:** `0f412f0a5211c4a2108f878af6d202cf098314a3`
- **Affected behavior:** A controller with the granted `workspace.session.renew` scope can issue multiple still-valid session records by repeating the renewal request. Normal controller behavior renews near expiry, but the endpoint itself does not enforce that cadence.
- **Technical evidence:** Each authorized handler invocation calls `token()`, inserts into `sessions`, and inserts a second scoped record. The startup cleanup removes expired `sessions` rows, but no corresponding routine for expired `federation_proxy_sessions` rows was found in the reviewed paths.
- **Safe reproduction:** Static call-path evidence only; no database mutation test was run because this item is not yet treated as a confirmed service-impacting defect.
- **Root cause:** Renewal is issuance-oriented and lacks an idempotency key, per-controller cooldown, or scoped-session retention policy.
- **Security/functional impact:** A compromised but still-valid controller credential could grow session tables through repeated authorized requests. The likely impact is database growth; no privilege escalation beyond the controller's granted scopes was identified.
- **Proposed fix:** Design a retry-safe renewal contract (for example, request IDs plus bounded issuance and cleanup) before enforcing a cooldown. A blind one-session-only rule could break recovery if a renewal response is lost while the previous token is still cached by the caller.
- **Implemented change:** None. Deliberately left for a focused design and concurrency test rather than adding a non-idempotent rate limit in this security patch.
- **Tests added:** None; remains audit-required.
- **Regression risks:** A rate limit without retry semantics can strand a workspace client after a lost renewal response.
- **Commit:** —
- **Remaining risk:** Table growth under repeated authorized renewal calls is not bounded by this phase.

## Controls verified

- Pairing codes are hashed, short-lived, one-use, and tied to an enabled administrator.
- Controller credentials are stored as hashes on the accepting side; the connecting side stores raw credentials in protected files, not SQLite API output.
- New outbound calls resolve DNS each time, validate the full address set, pin the current request to those addresses, require HTTPS, keep TLS verification enabled, and do not follow redirects.
- Workspace proxy paths reject encoded separators, traversal, empty components, and control-plane routes. Request bodies are streamed with a 512 MiB cap.
- Remote status responses are checked against the stable Node/server identity before metadata is updated.
- Revoke and rotation keep local and peer-side state distinct; failed revocation does not silently claim the remote credential was revoked.
- Root terminal uses a separate scope from normal terminal and has a separate ticket check at the destination.
- The private-network option is an explicit exception for trusted private Node endpoints, not a claim that a private overlay is provisioned. The private-network feature itself remains a non-enforcing draft as documented in phase 1.

## Verification

- `cargo test --locked --all-targets`: **222 passed, 0 failed** after the FED-001 regression tests.
- `cargo check --locked --all-targets`: **passed**.
- Tests were run in the isolated Rust Docker environment with no outbound Node connection.
- No migration was required for FED-001.
- Formatting remains subject to the `rustfmt` component download problem recorded in [00-baseline.md](00-baseline.md); `git diff --check` is still required before commit.

## Review limits and follow-up

- No real DNS rebinding, Node outage, certificate rotation, or cross-cloud private-network scenario was exercised; deterministic address validation is covered without network traffic.
- FED-002 needs a retry-safe session renewal design and bounded-cleanup test.
- Full multi-Node update/rollback and file-transfer chaos testing remains part of deployment and integration phases.
