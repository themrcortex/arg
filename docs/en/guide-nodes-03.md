# Health checks, credential rotation, and compatibility

## Node health

The node card reports connection and compatibility separately. `healthy` means the last secure request succeeded; `degraded` means the request reached the node but part of the operation or response failed; `offline` means it cannot be reached; `untrusted` means a credential or permission was rejected; and `incompatible` means the negotiated version or capability is insufficient. The sanitized error and last-check time appear on the card. Older nodes remain limited to supported read operations; writes are not attempted.

## Rotate a credential

Select the rotate-key button beside a node. The controller creates a random credential in the private `node-secrets` directory, sends it to the destination over HTTPS, and replaces the active credential only after success. If the network response is lost, a `.pending` file keeps the same key for a safe retry. Do not open, copy, or delete that file manually, and never paste the raw token into a log.

## Workspace session renewal

A remote workspace session expires after 12 hours. When a controller request is close to expiry, ARG obtains a new scoped session and replaces the private local token file. If renewal fails, that node request fails; it is never rerouted to another server. `401` usually means an expired session or credential. `403` usually means the granted scope does not permit the operation.

## Resolve incompatibility

For `Upgrade Required`, update both panels from a trusted source and refresh the node status. If required capabilities are still missing, revoke the pairing with confirmation and pair again at the desired access level. Re-pairing issues new scopes; changing a role or selected workspace does not broaden old permissions.

For `untrusted`, check the endpoint, TLS certificate, credential expiry, and granted scopes. Do not confuse `offline` with `untrusted`: the former is usually a network or service availability problem, while the latter means a credential or permission was rejected.
