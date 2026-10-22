#!/usr/bin/env bash
set -Eeuo pipefail
[[ $# -eq 3 ]] || exit 2
action="$1"
port="$2"
protocol="$3"
[[ "$port" =~ ^[0-9]{1,5}$ ]] && (( 10#$port >= 1 && 10#$port <= 65535 )) || exit 2
[[ "$protocol" == tcp || "$protocol" == udp ]] || exit 2
port="$((10#$port))"
ufw_bin="$(command -v ufw)"
[[ -n "$ufw_bin" ]] || exit 3
if [[ "$action" == add ]]; then
  "$ufw_bin" allow "$port/$protocol" comment 'DONATELLO WEB'
elif [[ "$action" == remove ]]; then
  [[ "$port" != 22 ]] || exit 4
  if [[ -f /etc/donatello/donatello.env ]]; then
    bind="$(sed -n 's/^DONATELLO_BIND=//p' /etc/donatello/donatello.env | tail -1)"
    [[ "$port" != "${bind##*:}" ]] || exit 4
  fi
  found=0
  while IFS= read -r line; do
    if [[ "$line" == *'# DONATELLO WEB'* && "$line" =~ ^[[:space:]]*\[[[:space:]]*([0-9]+)\] ]]; then
      rule_id="${BASH_REMATCH[1]}"
      if [[ "$line" =~ (^|[[:space:]])${port}/${protocol}([[:space:]]|$) ]]; then
        "$ufw_bin" --force delete "$rule_id" >/dev/null
        found=1
      fi
    fi
  done < <("$ufw_bin" status numbered | tac)
  [[ "$found" -eq 1 ]] || exit 5
else
  exit 2
fi
