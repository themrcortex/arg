#!/usr/bin/env bash
set -Eeuo pipefail
fail() { printf 'UFW: %s\n' "$1" >&2; exit "${2:-1}"; }
ensure_allow_first() {
  local port="$1" protocol="$2" label="$3" status line rule_id i
  local -a rule_ids=()
  status="$("$ufw_bin" status numbered)" || fail "could not inspect existing UFW rules before opening $port/$protocol" 4
  while IFS= read -r line; do
    if [[ "$line" == *"# $label"* && "$line" == *"ALLOW IN"* && "$line" =~ ^[[:space:]]*\[[[:space:]]*([0-9]+)\] ]]; then
      rule_id="${BASH_REMATCH[1]}"
      if [[ " $line " =~ [[:space:]]${port}/${protocol}[[:space:]] ]]; then
        rule_ids+=("$rule_id")
      fi
    fi
  done <<< "$status"
  # Delete matching ARG rules from bottom to top so numbered IDs remain valid.
  for ((i=${#rule_ids[@]}-1; i>=0; i--)); do
    "$ufw_bin" --force delete "${rule_ids[i]}" >/dev/null || fail "could not refresh the $label firewall rule" 4
  done
  # Put essential access rules ahead of any pre-existing deny rules.
  "$ufw_bin" insert 1 allow "$port/$protocol" comment "$label" >/dev/null || fail "could not allow $port/$protocol before enabling UFW" 4
}
ensure_web_allow_unless_denied() {
  local port="$1" status line rule_id first_id=999999 first_action="" label
  if [[ "$port" == 80 ]]; then label='ARG HTTP websites'; else label='ARG HTTPS websites'; fi
  status="$("$ufw_bin" status numbered)" || fail "could not inspect existing rules for web port $port" 4
  while IFS= read -r line; do
    [[ "$line" =~ ^[[:space:]]*\[[[:space:]]*([0-9]+)\] ]] || continue
    rule_id="${BASH_REMATCH[1]}"
    [[ " $line " =~ [[:space:]]${port}/tcp[[:space:]] ]] || continue
    if [[ "$line" == *"DENY IN"* || "$line" == *"ALLOW IN"* ]] && (( 10#$rule_id < first_id )); then
      first_id=$((10#$rule_id))
      if [[ "$line" == *"DENY IN"* ]]; then first_action=deny; else first_action=allow; fi
    fi
  done <<< "$status"
  # Respect an explicit inbound deny; the panel can change it to allow as a managed rule.
  [[ "$first_action" == deny ]] && return 0
  ensure_allow_first "$port" tcp "$label"
}
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
        ensure_allow_first "$ssh_port" tcp 'ARG SSH access'
      done <<< "$ssh_ports"
      panel_bind="$(sed -n 's/^ARG_BIND=//p' /etc/arg/arg.env | tail -1)"
      [[ "${panel_bind##*:}" =~ ^[0-9]+$ ]] || fail "could not read ARG_BIND from /etc/arg/arg.env; UFW was not enabled" 4
      ensure_allow_first "${panel_bind##*:}" tcp 'ARG panel'
      tls_bind="$(sed -n 's/^ARG_TLS_BIND=//p' /etc/arg/arg.env | tail -1)"
      if [[ -n "$tls_bind" ]]; then
        [[ "${tls_bind##*:}" =~ ^[0-9]+$ ]] || fail "could not read ARG_TLS_BIND from /etc/arg/arg.env; UFW was not enabled" 4
        ensure_allow_first "${tls_bind##*:}" tcp 'ARG HTTPS panel'
      fi
      # Keep hosted websites reachable regardless of whether panel TLS is configured.
      ensure_web_allow_unless_denied 80
      ensure_web_allow_unless_denied 443
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
# SSH and the ARG panel stay protected. HTTP/HTTPS are intentionally editable in the panel.
case "$port" in 22|8282|8443) fail "port $port is reserved for SSH or the ARG panel" 4 ;; esac
if [[ -f /etc/arg/arg.env ]]; then
  bind="$(sed -n 's/^ARG_BIND=//p' /etc/arg/arg.env | tail -1)"
  [[ "$port" != "${bind##*:}" ]] || fail "port $port is the configured ARG panel port" 4
  tls_bind="$(sed -n 's/^ARG_TLS_BIND=//p' /etc/arg/arg.env | tail -1)"
  [[ -z "$tls_bind" || "$port" != "${tls_bind##*:}" ]] || fail "port $port is the configured ARG HTTPS port" 4
fi
ufw_bin="$(command -v ufw || true)"
[[ -n "$ufw_bin" ]] || fail "UFW is not installed; use the Install UFW action first" 3
if [[ "$action" == add ]]; then
  "$ufw_bin" insert 1 "$policy" "$port/$protocol" comment 'ARG WEB'
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
