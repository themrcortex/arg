#!/usr/bin/env bash
set -Eeuo pipefail
if [[ $# -eq 1 ]]; then
  ufw_bin="$(command -v ufw)"
  [[ -n "$ufw_bin" ]] || exit 3
  case "$1" in
    enable)
      ssh_ports="$(sshd -T 2>/dev/null | awk '$1 == "port" {print $2}' || true)"
      [[ -n "$ssh_ports" ]] || ssh_ports=22
      while IFS= read -r ssh_port; do
        [[ "$ssh_port" =~ ^[0-9]+$ ]] || exit 4
        "$ufw_bin" allow "$ssh_port/tcp" comment 'DONATELLO SSH access' >/dev/null
      done <<< "$ssh_ports"
      panel_bind="$(sed -n 's/^DONATELLO_BIND=//p' /etc/donatello/donatello.env | tail -1)"
      [[ "${panel_bind##*:}" =~ ^[0-9]+$ ]] || exit 4
      "$ufw_bin" allow "${panel_bind##*:}/tcp" comment 'DONATELLO panel' >/dev/null
      tls_bind="$(sed -n 's/^DONATELLO_TLS_BIND=//p' /etc/donatello/donatello.env | tail -1)"
      if [[ -n "$tls_bind" ]]; then
        [[ "${tls_bind##*:}" =~ ^[0-9]+$ ]] || exit 4
        "$ufw_bin" allow "${tls_bind##*:}/tcp" comment 'DONATELLO HTTPS panel' >/dev/null
        "$ufw_bin" allow 80/tcp comment 'DONATELLO HTTPS challenge' >/dev/null
      fi
      "$ufw_bin" --force enable
      ;;
    disable) "$ufw_bin" --force disable ;;
    *) exit 2 ;;
  esac
  exit
fi
[[ $# -eq 4 ]] || exit 2
action="$1"
port="$2"
protocol="$3"
policy="$4"
[[ "$port" =~ ^[0-9]{1,5}$ ]] && (( 10#$port >= 1 && 10#$port <= 65535 )) || exit 2
[[ "$protocol" == tcp || "$protocol" == udp ]] || exit 2
[[ "$policy" == allow || "$policy" == deny ]] || exit 2
port="$((10#$port))"
# Management of access-critical ports stays with the installer and server administrator.
case "$port" in 22|80|443|8282|8443) exit 4 ;; esac
if [[ -f /etc/donatello/donatello.env ]]; then
  bind="$(sed -n 's/^DONATELLO_BIND=//p' /etc/donatello/donatello.env | tail -1)"
  [[ "$port" != "${bind##*:}" ]] || exit 4
  tls_bind="$(sed -n 's/^DONATELLO_TLS_BIND=//p' /etc/donatello/donatello.env | tail -1)"
  [[ -z "$tls_bind" || "$port" != "${tls_bind##*:}" ]] || exit 4
fi
ufw_bin="$(command -v ufw)"
[[ -n "$ufw_bin" ]] || exit 3
if [[ "$action" == add ]]; then
  "$ufw_bin" "$policy" "$port/$protocol" comment 'DONATELLO WEB'
elif [[ "$action" == remove ]]; then
  found=0
  while IFS= read -r line; do
    if [[ "$line" == *'# DONATELLO WEB'* && "$line" =~ ^[[:space:]]*\[[[:space:]]*([0-9]+)\] ]]; then
      rule_id="${BASH_REMATCH[1]}"
      if [[ "$line" =~ (^|[[:space:]])${port}/${protocol}([[:space:]]|$) && "$line" == *" ${policy^^} "* ]]; then
        "$ufw_bin" --force delete "$rule_id" >/dev/null
        found=1
      fi
    fi
  done < <("$ufw_bin" status numbered | tac)
  [[ "$found" -eq 1 ]] || exit 5
else
  exit 2
fi
