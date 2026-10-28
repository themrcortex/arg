#!/usr/bin/env bash
set -Eeuo pipefail

if [[ $# -eq 1 ]]; then
  mode=http
  port="$1"
elif [[ $# -eq 2 ]]; then
  mode="$1"
  port="$2"
else
  exit 2
fi
[[ "$mode" == http || "$mode" == https ]] || exit 2
[[ "$port" =~ ^[0-9]{4,5}$ ]] || exit 2
port=$((10#$port))
(( port >= 1024 && port <= 65535 )) || exit 2
[[ -f /etc/arg/arg.env ]] || exit 3
http_bind="$(sed -n 's/^ARG_BIND=//p' /etc/arg/arg.env | tail -1)"
tls_bind="$(sed -n 's/^ARG_TLS_BIND=//p' /etc/arg/arg.env | tail -1)"
if [[ "$mode" == http ]]; then
  [[ "$port" != "${tls_bind##*:}" ]] || exit 4
  action=port
  confirmation_args=()
else
  [[ -n "${tls_bind:-}" ]] || exit 5
  [[ -s /etc/arg/https-domain ]] || exit 5
  cert="$(sed -n 's/^ARG_TLS_CERT=//p' /etc/arg/arg.env | tail -1)"
  key="$(sed -n 's/^ARG_TLS_KEY=//p' /etc/arg/arg.env | tail -1)"
  [[ -s "$cert" && -s "$key" ]] || exit 5
  [[ "$port" != "${http_bind##*:}" ]] || exit 4
  action=tls-port
  confirmation_args=(web-confirm)
fi
[[ -x /usr/local/bin/arg ]] || exit 3

# Run independently of arg.service so restarting it cannot cancel this task.
exec systemd-run --quiet --collect --unit="arg-${mode}-port-change-${port}-$$" \
  --on-active=2s /usr/local/bin/arg manage "$action" "$port" "${confirmation_args[@]}"
