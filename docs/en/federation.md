# ARG Federation and Remote Workspaces

This article describes the current Federation behavior. Each VPS runs an independent ARG installation. Two installations establish trust with a one-time pairing code. The panel that adds a node is the controller; the destination remains the owner of its resources and the final authority for permissions. Pairing does not install ARG on the destination.

## Before pairing

Keep both VPS installations up to date and use a valid HTTPS certificate on the destination panel. In **Nodes** on the destination, set a public server name. It is shared with paired controllers, while a local label is visible only on the controller. The operating-system hostname is not changed. Check the destination role: `member` and `both` can issue inbound pairing codes; `controller-capable` and `both` can add outbound nodes.

## Roles

| Role | Connect to an outbound node | Accept an inbound controller |
| --- | --- | --- |
| `standalone` | No | No |
| `member` | No | Yes |
| `controller-capable` | Yes | No |
| `both` | Yes | Yes |

Changing the Federation role does not disable the independent ARG installation. A role change requires administrator confirmation. If the new role conflicts with existing connections, ARG reports the number of dependent nodes or controllers and refuses to orphan them. Disconnect those pairings from Nodes first, then retry.

## Pairing workflow

1. On the destination, open **Nodes → Allow management of this server** and choose an access mode. Read-only grants observation; Full also grants the allowed service, workspace, and terminal operations. The destination still validates each request independently.
2. Create a pairing code. It expires after five minutes or its first use. Do not post it publicly or include it in screenshots.
3. On the controller, open **Nodes → Connect this panel to another panel**. Enter a local label, the destination's HTTPS base URL, and the code.
4. Treat the destination as public by default. If it uses a private IP or internal DNS on a trusted private network, enable the private-network option. Loopback, link-local, and metadata endpoints remain blocked either way.
5. Inspect the node card after pairing. It reports connection state, compatibility, granted scopes, and workspace-session expiry. Selecting a node in the top bar changes the workspace context; it does not grant authorization.

## Scopes and older peers

The controller token is stored in a private server-side file and is never sent to browser code or localStorage. Scopes separate status reads, service reads, service management, workspace reads/writes, terminal, and root-terminal authorization. The destination checks the required scope for each request. Remote mutations are audited on the destination under the paired controller name.

If either panel lacks a negotiated workspace-proxy capability, ARG marks it `legacy-read-only` or `incompatible`. It does not add permissions to make an older version appear compatible. Unsupported operations should return an upgrade and re-pair message. **All servers** is an aggregated read view; a write requires an explicitly selected destination.

## ARG Mesh dashboard map

In multi-server mode, use the dashboard's **Layout** control to show or hide the **ARG Mesh** widget and change its order. The diagram shows this panel's registered connections to its Nodes. It is a hub-and-spoke view, not a claim that every VPS has a direct connection to every other VPS. Every 30 seconds, the widget issues a read-only status probe to reachable Nodes. Outbound and return dots represent that real probe and its real response. Response time is measured; the displayed byte count is only the JSON response size, not HTTPS overhead, file transfers, or total interface traffic. A disconnected state is the last probe result.

Federation and workspace-proxy HTTP requests use HTTPS with certificate verification, TLS 1.2 or later, and the paired credential. Each HTTP request creates a fresh connection. TLS negotiates that session's traffic keys during the handshake, and the connection is not reused for the next request. A terminal WebSocket keeps its TLS session for the lifetime of that terminal and is not disconnected to force a timed rotation. This uses standard transport encryption: ARG does not implement a custom cipher or a separately scheduled key that changes every few minutes. Requests are rejected when destination HTTPS or certificate validation fails.

## Credentials, sessions, and troubleshooting

Controller credentials expire. The rotate action first saves a random credential in a restricted pending file, sends it to the destination over HTTPS, then replaces the active local file after a successful response. If the network fails, the pending file supports retrying the same rotation. Do not copy or remove it manually. If retries fail, check destination network access and certificate, then use re-pairing.

Workspace sessions last 12 hours. The controller renews them through the authenticated destination before expiry. Connection states are `healthy`, `degraded`, `offline`, `untrusted`, and `incompatible`. The node card shows sanitized errors and the last check time. `offline` means the secure remote connection is unavailable; it does not mean the local ARG service has stopped.

To revoke access, remove the node on the controller or revoke that controller on the destination. The destination invalidates the credential and deletes the controller's proxy sessions. Removing a node never deletes files or applications from its VPS.

## Current limits

Version and scope negotiation, workspace proxy, and terminal proxy are implemented. A full network integration test on two real VPSs is still needed before claiming operational end-to-end coverage. If public-name synchronization or a health check fails for some nodes, ARG reports the pending count and never redirects the operation to a different server.
# Checking and updating Nodes

The Nodes list checks the `main` branch when it loads and every 15 minutes while the page is open. It checks at most two eligible Nodes at a time. Eligible Nodes must be compatible and grant update-management access. When a newer revision is found, ARG shows an alert on that Node and a summary above the list; the alert's action opens the update dialog for that exact Node.

The controller reads the update phase, branch, build mode, message, and job ID from the destination Node's Update Helper. The destination may be briefly unavailable while its service restarts. The controller reconnects and then verifies the destination's stable server identity and API. The dialog can also retrieve the real update log, bounded to its last 64 KiB.

The update runs on the destination Node. Federation identity, Node ID, and saved pairing keys are retained. Remote update is unavailable for outdated, read-only, or incompatible pairings; update that Node locally or over SSH first.
