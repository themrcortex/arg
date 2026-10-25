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
printf 'public IPv4 fallback and validation tests passed\n'
