#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

sed -n '/^select_update_options() {$/,/^}$/p' "$root/install.sh" > "$tmp/functions.sh"
sed -n '/^installer_script_url() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^configure_release_build_resources() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^release_build_memory_budget_mb() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^release_build_disk_space_ok() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^build_release() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^cleanup_update_build_cache() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^cleanup_update_reservation() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
source "$tmp/functions.sh"

BRANCH=main
ARG_UPDATE_MODE=complete
check_update_status() { UPDATE_STATUS="AVAILABLE"; }
info() { :; }
warn() { :; }
die() { printf '%s\n' "$1" >&2; exit 2; }
select_update_options development fast
[[ "$BRANCH" == development && "$ARG_UPDATE_MODE" == fast && "$ARG_UPDATE_CONTEXT" == update ]]
[[ "$(installer_script_url development)" == https://raw.githubusercontent.com/themrcortex/arg/development/install.sh ]]
select_update_options main complete
[[ "$BRANCH" == main && "$ARG_UPDATE_MODE" == complete ]]
[[ "$(installer_script_url main)" == https://raw.githubusercontent.com/themrcortex/arg/main/install.sh ]]

unset CARGO_BUILD_JOBS CARGO_PROFILE_RELEASE_CODEGEN_UNITS CARGO_PROFILE_RELEASE_OPT_LEVEL
configure_release_build_resources 2048
[[ "$CARGO_BUILD_JOBS" == 1 && "$CARGO_PROFILE_RELEASE_CODEGEN_UNITS" == 1 && "$CARGO_PROFILE_RELEASE_OPT_LEVEL" == 2 ]]
unset CARGO_BUILD_JOBS CARGO_PROFILE_RELEASE_CODEGEN_UNITS CARGO_PROFILE_RELEASE_OPT_LEVEL
configure_release_build_resources 1024
[[ "$CARGO_BUILD_JOBS" == 1 && "$CARGO_PROFILE_RELEASE_CODEGEN_UNITS" == 1 && "$CARGO_PROFILE_RELEASE_OPT_LEVEL" == 1 ]]
unset CARGO_BUILD_JOBS CARGO_PROFILE_RELEASE_CODEGEN_UNITS CARGO_PROFILE_RELEASE_OPT_LEVEL
configure_release_build_resources 8192
[[ -z "${CARGO_BUILD_JOBS:-}" && -z "${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-}" && -z "${CARGO_PROFILE_RELEASE_OPT_LEVEL:-}" ]]
release_build_disk_space_ok 2048 2048
if release_build_disk_space_ok 1023 1024; then
  echo 'low disk space was accepted for a complete build' >&2
  exit 1
fi
release_build_disk_space_ok 1024 1024

SOURCE_DIR="$tmp/source"
BINARY="$tmp/active-arg"
mkdir -p "$SOURCE_DIR/frontend/dist"
: > "$SOURCE_DIR/frontend/dist/index.html"
printf 'known-good-active-binary\n' > "$BINARY"
df() { printf 'Filesystem 1M-blocks Used Available Use%% Mounted on\nmock 10000 1 9999 1%% /tmp\n'; }
calls=()
run_with_progress() {
  calls+=("$1")
  if [[ "$1" == "Compiling the optimized Rust release" ]]; then
    [[ "${ARG_TEST_BUILD_FAIL:-0}" != 1 ]] || return 137
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
ARG_TEST_BUILD_FAIL=1
if build_release; then
  echo 'failed release compilation was reported as successful' >&2
  exit 1
fi
grep -q 'known-good-active-binary' "$BINARY"
unset ARG_TEST_BUILD_FAIL

if (select_update_options staging fast) >/dev/null 2>&1; then
  echo 'unsupported branch was accepted' >&2
  exit 1
fi
if (select_update_options main clean) >/dev/null 2>&1; then
  echo 'unsupported build mode was accepted' >&2
  exit 1
fi

# EXIT recovery state must outlive update_app's local variables. Simulate a failed
# compile, return from that function scope, then let the process EXIT trap clean up.
cleanup_dir="$tmp/update-cleanup"
mkdir -p "$cleanup_dir/run/arg-update.lock"
printf 'started pid start\n' > "$cleanup_dir/run/arg-update-in-progress"
touch "$cleanup_dir/staged-binary" "$cleanup_dir/staged-database"
cat > "$cleanup_dir/update-helper" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$ARG_TEST_CLEANUP_HELPER_LOG"
MOCK
chmod +x "$cleanup_dir/update-helper"
mkdir -p "$cleanup_dir/bin"
cat > "$cleanup_dir/bin/cargo" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$ARG_TEST_CARGO_LOG"
MOCK
chmod +x "$cleanup_dir/bin/cargo"
export ARG_TEST_CLEANUP_HELPER_LOG="$cleanup_dir/helper.log"
export ARG_TEST_CARGO_LOG="$cleanup_dir/cargo.log"
cleanup_status=0
(
  UPDATE_HELPER="$cleanup_dir/update-helper"
  UPDATE_CANCEL_FILE="$cleanup_dir/run/arg-update-cancel-requested"
  UPDATE_STATUS_FILE="$cleanup_dir/run/arg-update-in-progress"
  UPDATE_LOCK_DIR="$cleanup_dir/run/arg-update.lock"
  UPDATE_CLEANUP_BUILD_STARTED=1
  UPDATE_CLEANUP_BINARY_SWAPPED=0
  UPDATE_CLEANUP_STAGED="$cleanup_dir/staged-binary"
  UPDATE_CLEANUP_DATABASE_STAGED="$cleanup_dir/staged-database"
  ARG_UPDATE_MODE=complete
  export PATH="$cleanup_dir/bin:$PATH"
  failed_update() {
    local build_started=1 binary_swapped=0 staged=local-only database_staged=local-only
    trap cleanup_update_reservation EXIT
    return 19
  }
  failed_update || exit "$?"
) 2>"$cleanup_dir/stderr" || cleanup_status=$?
[[ "$cleanup_status" == 19 ]]
! grep -q 'unbound variable' "$cleanup_dir/stderr"
test ! -e "$cleanup_dir/run/arg-update-in-progress"
test ! -e "$cleanup_dir/run/arg-update-cancel-requested"
test ! -d "$cleanup_dir/run/arg-update.lock"
test ! -e "$cleanup_dir/staged-binary"
test ! -e "$cleanup_dir/staged-database"
grep -q 'state failed failed' "$ARG_TEST_CLEANUP_HELPER_LOG"
grep -q '+stable clean' "$ARG_TEST_CARGO_LOG"
(
  export PATH="$cleanup_dir/bin:$PATH"
  ARG_UPDATE_MODE=fast
  UPDATE_CLEANUP_BUILD_STARTED=1
  SOURCE_DIR="$tmp/source"
  cleanup_update_build_cache 1
)
[[ "$(grep -c '+stable clean' "$ARG_TEST_CARGO_LOG")" == 2 ]]

printf 'SSH update selection, low-memory build settings, and EXIT cleanup tests passed\n'
