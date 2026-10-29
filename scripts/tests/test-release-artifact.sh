#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

sed -n '/^release_version_from_source() {$/,/^}$/p' "$root/install.sh" > "$tmp/functions.sh"
sed -n '/^release_target_for_host() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^download_release_binary() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"

mkdir -p "$tmp/bin" "$tmp/source" "$tmp/rollback" "$tmp/releases"
printf '[package]\nversion = "0.7.7"\n' > "$tmp/source/Cargo.toml"
cat > "$tmp/releases/arg-v0.7.7-x86_64-unknown-linux-musl" <<'MOCK'
#!/usr/bin/env bash
[[ "${1:-}" == --version ]] && printf 'arg 0.7.7\n'
MOCK
chmod 0755 "$tmp/releases/arg-v0.7.7-x86_64-unknown-linux-musl"
(cd "$tmp/releases" && sha256sum arg-v0.7.7-x86_64-unknown-linux-musl > arg-v0.7.7-x86_64-unknown-linux-musl.sha256)

cat > "$tmp/bin/curl" <<'MOCK'
#!/usr/bin/env bash
url="" output=""
while (($#)); do
  case "$1" in
    -o) output="$2"; shift 2 ;;
    http*) url="$1"; shift ;;
    *) shift ;;
  esac
done
name="${url##*/}"
[[ -n "$url" && -n "$output" ]] || exit 2
cp "$ARG_TEST_RELEASE_FIXTURES/$name" "$output"
if [[ "${ARG_TEST_RELEASE_TAMPER:-0}" == 1 && "$name" != *.sha256 ]]; then
  printf 'tampered\n' >> "$output"
fi
MOCK
cat > "$tmp/bin/uname" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "${ARG_TEST_MACHINE:-x86_64}"
MOCK
cat > "$tmp/bin/od" <<'MOCK'
#!/usr/bin/env bash
case "$*" in
  *'-j 4 -N 2'*) printf ' 2 1\n' ;;
  *'-j 18 -N 2'*) printf ' %s 0\n' "${ARG_TEST_ELF_MACHINE:-62}" ;;
  *) exit 2 ;;
esac
MOCK
chmod 0755 "$tmp/bin/curl" "$tmp/bin/uname" "$tmp/bin/od"
export PATH="$tmp/bin:$PATH"
export ARG_TEST_RELEASE_FIXTURES="$tmp/releases"
export SOURCE_DIR="$tmp/source" ROLLBACK_DIR="$tmp/rollback"
source "$tmp/functions.sh"
die() { printf '%s\n' "$1" >&2; exit 2; }
info() { :; }
install() {
  local -a args=()
  while (($#)); do
    case "$1" in
      -o|-g) shift 2 ;;
      *) args+=("$1"); shift ;;
    esac
  done
  command install "${args[@]}"
}

[[ "$(release_version_from_source)" == 0.7.7 ]]
[[ "$(release_target_for_host)" == x86_64-unknown-linux-musl ]]
ARG_TEST_MACHINE=aarch64
export ARG_TEST_MACHINE
[[ "$(release_target_for_host)" == aarch64-unknown-linux-musl ]]
ARG_TEST_MACHINE=armv7l
if release_target_for_host >/dev/null; then
  echo 'unsupported architecture was accepted' >&2
  exit 1
fi
unset ARG_TEST_MACHINE

download_release_binary
[[ -x "$UPDATE_BUILT_BINARY" ]]
[[ "$("$UPDATE_BUILT_BINARY" --version)" == 'arg 0.7.7' ]]
rm -rf "$UPDATE_CLEANUP_RELEASE_DIR"
UPDATE_CLEANUP_RELEASE_DIR=""

ARG_TEST_RELEASE_TAMPER=1
export ARG_TEST_RELEASE_TAMPER
if (download_release_binary) >/dev/null 2>&1; then
  echo 'binary with an invalid checksum was accepted' >&2
  exit 1
fi
unset ARG_TEST_RELEASE_TAMPER

ARG_TEST_ELF_MACHINE=183
export ARG_TEST_ELF_MACHINE
if (download_release_binary) >/dev/null 2>&1; then
  echo 'binary for a different architecture was accepted' >&2
  exit 1
fi

printf 'release artifact checksum, architecture, and version checks passed\n'
