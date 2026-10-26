## Network interfaces

For each active interface, ARG may show the device name, UP/DOWN state, IPv4/IPv6 addresses, MAC, MTU, and RX/TX counters. Names such as `ens3`, `eth0`, or `enp1s0` vary by Linux distribution and hypervisor.

RX/TX rates are calculated from counter differences between samples. Aggregates sum eligible interfaces. The first sample after reboot, counter reset, or device replacement may not have a rate. Inspect the timestamp for each chart point.

## Traffic volume

The Network page derives hourly/daily byte totals from stored interface counters. These figures may differ from provider billing: providers can count ingress/egress, internal traffic, IPv6, billing periods, or units differently. ARG does not perform general exact per-TCP/UDP-port accounting by default.

## Ports

The Ports page lists listening sockets: which process listens on which address and port. It does not guarantee internet reachability. External access also depends on routing, OS firewall, provider security group, bind address, and service health.

The TCP probe checks a TCP connection. It is not an external vulnerability scan or application-level health test. UDP is connectionless and cannot be verified by a simple TCP probe.

## UFW

The Firewall page can inspect UFW state, display ARG-managed rules, and create or modify TCP/UDP rules. Installation, enable/disable, and rule actions depend on tool availability and system permissions.

### Before enabling UFW

1. Confirm the actual SSH port using `sshd -T` or provider settings.
2. Add and verify an allow rule for SSH.
3. Allow the panel and any required HTTP/HTTPS or service ports.
4. Keep a second SSH session and test external access.
5. Enable UFW only after those checks.

ARG protects detected SSH and panel ports, but does not control provider firewalls or prevent mistakes in external network policy. Installing UFW must not silently enable it. Closing ports 80/443 affects websites and certificate issuance.

## Read-only SSH commands

```bash
ip -br address
ip -s link
ss -lntup
sudo ufw status verbose
```

## Working with the page controls

### Interfaces and traffic

Use **Active interfaces** to hide disconnected or less useful virtual devices while investigating. Expand a card for interface details, addresses, and receive/transmit counters. Rx/Tx rate is not the same as total transfer: rate is derived from counter changes between samples; a range total is based on samples and can have gaps after reboot, counter reset, or interface replacement. For example, 450 MiB received by `eth0` from 10:00–11:00 is interface traffic, not necessarily one website's traffic.

Time filters select preset or custom ranges. To inspect hourly usage for one day, select that date and choose the hourly view. Chart points retain their own dates/times; a compact axis does not mean values were merged. For ranges over 24 hours, read each tooltip or the data table instead of inferring all points from the first and last hour labels.

### Ports

A port row reports the listener's port, protocol/bind address, and system state. A firewall rule and a listener are separate: allowing an unused port only exposes no service; a running listener may still be blocked by UFW. Critical ports such as SSH and ARG's panel remain visible so their protection is clear. If their toggle is disabled, ARG is preventing an accidental loss of management access. Ports 80 and 443 are manageable; before closing them, check websites, reverse proxy, certificate renewal, and HTTPS access.

### UFW: install, prepare, enable

If UFW is absent, **Install UFW** installs the package and should not immediately enable it. First inspect Ports and note the actual SSH port and panel port. Add the required rules (commonly SSH, 80, 443, and panel), confirm remote access, then enable UFW. When UFW is off, you can prepare rules without interrupting the current connection. Enable/Disable, Reload, Status, and rule management are separate actions; inspect each operation's result.

When adding a rule, specify direction, port or range, TCP/UDP/both, and optionally the allowed source. If SSH uses `22/tcp`, an allow for `22/udp` does not replace it. Web access generally needs `80/tcp` and `443/tcp`; UDP is unnecessary unless HTTP/3/QUIC is configured. Choosing both should create two corresponding rules. Removing a rule affects that managed rule only; do not remove unrelated manual or tool-created rules without identifying them.

### Example: enable UFW without losing SSH

1. Find the actual SSH port on Ports; it may not be 22.
2. Add an Allow rule for that port and TCP. Restrict the source to trusted IPs if practical.
3. Add 80/443 rules needed by websites and keep the panel reachable.
4. Review rules and compare them with active listeners.
5. Enable UFW while keeping a second SSH session or test connection available.
6. Once confirmed, remove access that is no longer needed.

`No matching ARG-managed UFW rule` usually means the rule was created outside ARG or its properties do not match the managed record. Inspect full `ufw status numbered`; do not delete a manual rule from the panel unless you have positively identified it. Installing UFW does not change the VPS provider's separate network firewall.
