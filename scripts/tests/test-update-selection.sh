#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

sed -n '/^select_update_options() {$/,/^}$/p' "$root/install.sh" > "$tmp/functions.sh"
sed -n '/^build_release() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
source "$tmp/functions.sh"

BRANCH=main
ARG_UPDATE_MODE=complete
check_update_status() { UPDATE_STATUS="AVAILABLE"; }
info() { :; }
die() { printf '%s\n' "$1" >&2; exit 2; }
select_update_options development fast
[[ "$BRANCH" == development && "$ARG_UPDATE_MODE" == fast && "$ARG_UPDATE_CONTEXT" == update ]]
select_update_options main complete
[[ "$BRANCH" == main && "$ARG_UPDATE_MODE" == complete ]]

SOURCE_DIR="$tmp/source"
mkdir -p "$SOURCE_DIR/frontend/dist"
: > "$SOURCE_DIR/frontend/dist/index.html"
calls=()
run_with_progress() {
  calls+=("$1")
  if [[ "$1" == "Compiling the optimized Rust release" ]]; then
    mkdir -p "$SOURCE_DIR/target/release"
    touch "$SOURCE_DIR/target/release/arg"
    chmod +x "$SOURCE_DIR/target/release/arg"
  fi
}

ARG_UPDATE_CONTEXT=update ARG_UPDATE_MODE=complete
build_release
[[ "${calls[*]}" == "Clearing previous Rust build cache Compiling the optimized Rust release" ]]
calls=()
ARG_UPDATE_CONTEXT=update ARG_UPDATE_MODE=fast
build_release
[[ "${calls[*]}" == "Compiling the optimized Rust release" ]]
calls=()
ARG_UPDATE_CONTEXT= ARG_UPDATE_MODE=complete
build_release
[[ "${calls[*]}" == "Compiling the optimized Rust release" ]]

if (select_update_options staging fast) >/dev/null 2>&1; then
  echo 'unsupported branch was accepted' >&2
  exit 1
fi
if (select_update_options main clean) >/dev/null 2>&1; then
  echo 'unsupported build mode was accepted' >&2
  exit 1
fi

printf 'SSH update branch and build-cache selection tests passed\n'
