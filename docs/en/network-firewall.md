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
