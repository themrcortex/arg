#!/usr/bin/env bash
set -Eeuo pipefail

[[ $# -eq 1 && "$1" =~ ^[0-9]{4,5}$ ]] || exit 2
port="$1"
(( port >= 1024 && port <= 65535 )) || exit 2
[[ -f /etc/donatello/donatello.env ]] || exit 3
tls_bind="$(sed -n 's/^DONATELLO_TLS_BIND=//p' /etc/donatello/donatello.env | tail -1)"
[[ "$port" != "${tls_bind##*:}" ]] || exit 4
[[ -x /usr/local/bin/donatello ]] || exit 3

# Run independently of donatello.service so restarting it cannot cancel this task.
exec systemd-run --quiet --collect --unit="donatello-port-change-${port}-$$" \
  --on-active=2s /usr/local/bin/donatello manage port "$port"
