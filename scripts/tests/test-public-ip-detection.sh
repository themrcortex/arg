#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin"
sed -n '/^detect_public_ipv4() {$/,/^}$/p' "$root/install.sh" > "$tmp/detect.sh"
cat > "$tmp/bin/curl" <<'MOCK'
#!/usr/bin/env bash
endpoint="${!#}"
case "$endpoint" in
  https://api.ipify.org) value="${ARG_TEST_IPIFY:-}" ;;
  https://checkip.amazonaws.com) value="${ARG_TEST_AMAZON:-}" ;;
  https://ipv4.icanhazip.com) value="${ARG_TEST_ICANHAZ:-}" ;;
  https://ifconfig.me/ip) value="${ARG_TEST_IFCONFIG:-}" ;;
  *) exit 2 ;;
esac
[[ -n "$value" ]] || exit 7
printf '%s\n' "$value"
MOCK
chmod +x "$tmp/bin/curl"
export PATH="$tmp/bin:$PATH"
source "$tmp/detect.sh"

export ARG_TEST_IPIFY=''
export ARG_TEST_AMAZON='94.184.32.235'
test "$(detect_public_ipv4)" = '94.184.32.235'

export ARG_TEST_AMAZON='192.168.1.10'
export ARG_TEST_IFCONFIG='999.1.1.1'
if detect_public_ipv4 >/dev/null; then
  echo 'private or malformed IPv4 was accepted' >&2
  exit 1
fi

export ARG_TEST_IFCONFIG='094.184.032.235'
test "$(detect_public_ipv4)" = '94.184.32.235'

# The alternate CA request must retain its configured directory and retry a
# transient ACME read timeout without adding a shell or exposing EAB values.
sed -n '/^issue_certificate() {$/,/^}$/p' "$root/install.sh" > "$tmp/issue.sh"
source "$tmp/issue.sh"
write_cert_deploy_hook() { :; }
restore_port80_service() { PORT80_STOPPED_UNIT=""; }
info() { :; }
warn() { :; }
sleep() { :; }
export ARG_TEST_CERTBOT_COUNT="$tmp/certbot-count"
export ARG_TEST_CERTBOT_ARGS="$tmp/certbot-args"
cat > "$tmp/bin/certbot" <<'MOCK'
#!/usr/bin/env bash
count=0
[[ ! -f "$ARG_TEST_CERTBOT_COUNT" ]] || count="$(cat "$ARG_TEST_CERTBOT_COUNT")"
count=$((count + 1))
printf '%s' "$count" > "$ARG_TEST_CERTBOT_COUNT"
printf '%s\n' "$*" >> "$ARG_TEST_CERTBOT_ARGS"
if (( count == 1 )); then
  echo 'requests.exceptions.ReadTimeout'
  exit 1
fi
echo 'certificate issued'
MOCK
chmod +x "$tmp/bin/certbot"
CERTBOT_BIN="$tmp/bin/certbot"
CERT_NAME=arg
CERT_DEPLOY_HOOK="$tmp/deploy-hook"
PORT80_STOPPED_UNIT=""
issue_certificate 'panel.example.com' 'admin@example.com' '' n \
  'https://acme.zerossl.com/v2/DV90' "$tmp/zerossl.ini" >/dev/null
trap 'rm -rf "$tmp"' EXIT
test "$(cat "$ARG_TEST_CERTBOT_COUNT")" = 2
rg -q -- '--server https://acme.zerossl.com/v2/DV90' "$ARG_TEST_CERTBOT_ARGS"
rg -F -q -- "--config $tmp/zerossl.ini" "$ARG_TEST_CERTBOT_ARGS"
printf 'public IPv4 validation and alternate ACME retry tests passed\n'
