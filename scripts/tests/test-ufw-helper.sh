#!/usr/bin/env bash
set -Eeuo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
helper="$root/scripts/ufw-helper.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin"
cat > "$tmp/bin/ufw" <<'MOCK'
#!/usr/bin/env bash
case "$1" in
  status)
    cat "$ARG_UFW_STATUS"
    ;;
  --force)
    [[ "$2" == delete ]] || exit 2
    id="$3"
    awk -v id="$id" '$1 != "[" id "]" { print }' "$ARG_UFW_STATUS" > "$ARG_UFW_STATUS.tmp"
    mv "$ARG_UFW_STATUS.tmp" "$ARG_UFW_STATUS"
    ;;
  *) exit 2 ;;
esac
MOCK
chmod +x "$tmp/bin/ufw"
export PATH="$tmp/bin:$PATH" ARG_UFW_STATUS="$tmp/status"

printf 'Status: active\n\nTo                         Action      From\n80/tcp                     ALLOW IN    Anywhere\n' > "$ARG_UFW_STATUS"
"$helper" remove 80 tcp allow
! grep -q '80/tcp' "$ARG_UFW_STATUS"
# Already removed web access is an idempotent success, not a stale-rule error.
"$helper" remove 80 tcp allow

printf 'Status: active\n\nTo                         Action      From\n443/tcp                    DENY IN     Anywhere\n' > "$ARG_UFW_STATUS"
"$helper" remove 443 tcp deny
! grep -q '443/tcp' "$ARG_UFW_STATUS"

printf 'Status: active\n\nTo                         Action      From\n8080/tcp                   ALLOW IN    Anywhere\n' > "$ARG_UFW_STATUS"
if "$helper" remove 8080 tcp allow >/dev/null 2>&1; then
  echo 'unmanaged non-web rule was unexpectedly removed' >&2
  exit 1
fi
printf 'UFW web-port reconciliation tests passed\n'
