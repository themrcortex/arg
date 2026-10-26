# Choosing and safely changing a server role

## What this setting controls

The Federation role decides how this ARG installation participates in panel-to-panel connections. It is different from the account role: an `admin` account may change local settings, while the Federation role describes which connection directions this server accepts.

## Role meanings

- `standalone`: manage this installation locally; do not create inbound or outbound pairings.
- `member`: issue pairing codes so a trusted controller can manage this installation.
- `controller-capable`: connect this panel to other nodes using their pairing codes.
- `both`: accept inbound controllers and manage outbound nodes.

## Change a role

1. Open **Nodes** and find **Federation role for this server**.
2. Choose a role and select **Save role**.
3. Confirm the dialog. If the new role conflicts with current pairings, the API returns a conflict and reports how many connections block the change.
4. Disconnect those pairings from Nodes or the controller list, then save the role again.

ARG never silently deletes an active connection. A `standalone` role still permits local Nginx, S3, Docker, and other host management when their dependencies are available. Refresh the node list after changing the role and verify the new role appears.

## Common messages

If the message says pairing is not allowed, the current role does not support the requested connection direction. If it reports dependent connections, disconnect them from Nodes; removing a pairing can revoke the credential on the other panel. A role change does not change the hostname, ports, or system services.
