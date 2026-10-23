#!/usr/bin/env bash
set -Eeuo pipefail
fail() { printf 'UFW: %s\n' "$1" >&2; exit "${2:-1}"; }
if [[ $# -eq 1 ]]; then
  if [[ "$1" == install ]]; then
    [[ -r /etc/os-release ]] || fail "cannot identify this Linux distribution" 3
    . /etc/os-release
    [[ "${ID:-}" == debian || "${ID:-}" == ubuntu ]] || fail "automatic installation is supported on Debian and Ubuntu only" 3
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq || fail "apt package index update failed; check DNS and repository access"
    apt-get install -y --no-install-recommends ufw || fail "apt could not install UFW"
    exit 0
  fi
  ufw_bin="$(command -v ufw || true)"
  [[ -n "$ufw_bin" ]] || fail "UFW is not installed; use the Install UFW action first" 3
  case "$1" in
    status) "$ufw_bin" status numbered ;;
    enable)
      ssh_ports="$(sshd -T 2>/dev/null | awk '$1 == "port" {print $2}' || true)"
      [[ -n "$ssh_ports" ]] || ssh_ports=22
      while IFS= read -r ssh_port; do
        [[ "$ssh_port" =~ ^[0-9]+$ ]] || fail "could not determine the SSH port safely; UFW was not enabled" 4
        "$ufw_bin" allow "$ssh_port/tcp" comment 'ARG SSH access' >/dev/null
      done <<< "$ssh_ports"
      panel_bind="$(sed -n 's/^ARG_BIND=//p' /etc/arg/arg.env | tail -1)"
      [[ "${panel_bind##*:}" =~ ^[0-9]+$ ]] || fail "could not read ARG_BIND from /etc/arg/arg.env; UFW was not enabled" 4
      "$ufw_bin" allow "${panel_bind##*:}/tcp" comment 'ARG panel' >/dev/null
      tls_bind="$(sed -n 's/^ARG_TLS_BIND=//p' /etc/arg/arg.env | tail -1)"
      if [[ -n "$tls_bind" ]]; then
        [[ "${tls_bind##*:}" =~ ^[0-9]+$ ]] || fail "could not read ARG_TLS_BIND from /etc/arg/arg.env; UFW was not enabled" 4
        "$ufw_bin" allow "${tls_bind##*:}/tcp" comment 'ARG HTTPS panel' >/dev/null
        "$ufw_bin" allow 80/tcp comment 'ARG HTTPS challenge' >/dev/null
      fi
      "$ufw_bin" --force enable
      ;;
    disable) "$ufw_bin" --force disable ;;
      *) fail "unsupported action '$1'" 2 ;;
  esac
  exit
fi
[[ $# -eq 4 ]] || fail "invalid helper arguments" 2
action="$1"
port="$2"
protocol="$3"
policy="$4"
[[ "$port" =~ ^[0-9]{1,5}$ ]] && (( 10#$port >= 1 && 10#$port <= 65535 )) || fail "port must be between 1 and 65535" 2
[[ "$protocol" == tcp || "$protocol" == udp ]] || fail "protocol must be tcp or udp" 2
[[ "$policy" == allow || "$policy" == deny ]] || fail "policy must be allow or deny" 2
port="$((10#$port))"
# Management of access-critical ports stays with the installer and server administrator.
case "$port" in 22|80|443|8282|8443) fail "port $port is reserved for SSH, HTTP, HTTPS, or the ARG panel" 4 ;; esac
if [[ -f /etc/arg/arg.env ]]; then
  bind="$(sed -n 's/^ARG_BIND=//p' /etc/arg/arg.env | tail -1)"
  [[ "$port" != "${bind##*:}" ]] || fail "port $port is the configured ARG panel port" 4
  tls_bind="$(sed -n 's/^ARG_TLS_BIND=//p' /etc/arg/arg.env | tail -1)"
  [[ -z "$tls_bind" || "$port" != "${tls_bind##*:}" ]] || fail "port $port is the configured ARG HTTPS port" 4
fi
ufw_bin="$(command -v ufw || true)"
[[ -n "$ufw_bin" ]] || fail "UFW is not installed; use the Install UFW action first" 3
if [[ "$action" == add ]]; then
  "$ufw_bin" "$policy" "$port/$protocol" comment 'ARG WEB'
elif [[ "$action" == remove ]]; then
  found=0
  while IFS= read -r line; do
    if [[ "$line" == *'# ARG WEB'* || "$line" == *'# DONATELLO WEB'* ]] && [[ "$line" =~ ^[[:space:]]*\[[[:space:]]*([0-9]+)\] ]]; then
      rule_id="${BASH_REMATCH[1]}"
      if [[ "$line" =~ (^|[[:space:]])${port}/${protocol}([[:space:]]|$) && "$line" == *" ${policy^^} "* ]]; then
        "$ufw_bin" --force delete "$rule_id" >/dev/null
        found=1
      fi
    fi
  done < <("$ufw_bin" status numbered | tac)
  [[ "$found" -eq 1 ]] || fail "no matching ARG-managed UFW rule was found" 5
else
  fail "unsupported action '$action'" 2
fi
