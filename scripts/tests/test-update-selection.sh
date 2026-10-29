#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

sed -n '/^select_update_options() {$/,/^}$/p' "$root/install.sh" > "$tmp/functions.sh"
sed -n '/^installer_script_url() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^release_build_cpu_count() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^release_build_memory_capacity_mb() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^configure_release_build_resources() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^release_build_is_root() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^release_build_swap_active() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^prepare_release_build_swap() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^cleanup_release_build_swap() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^release_build_memory_budget_mb() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^release_build_disk_space_ok() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^build_release() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^cleanup_update_build_cache() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
sed -n '/^cleanup_update_reservation() {$/,/^}$/p' "$root/install.sh" >> "$tmp/functions.sh"
source "$tmp/functions.sh"
release_build_cpu_count() { printf '16\n'; }
release_build_memory_capacity_mb() { printf '%s\n' "${BUILD_CAPACITY_MB:-8192}"; }

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

unset CARGO_BUILD_JOBS CARGO_PROFILE_RELEASE_CODEGEN_UNITS CARGO_PROFILE_RELEASE_OPT_LEVEL ARG_CARGO_BUILD_JOBS ARG_CARGO_PROFILE_RELEASE_CODEGEN_UNITS ARG_CARGO_PROFILE_RELEASE_OPT_LEVEL
configure_release_build_resources 2048
[[ "$CARGO_BUILD_JOBS" == 1 && "$CARGO_PROFILE_RELEASE_CODEGEN_UNITS" == 4 && "$CARGO_PROFILE_RELEASE_OPT_LEVEL" == 2 ]]
unset CARGO_BUILD_JOBS CARGO_PROFILE_RELEASE_CODEGEN_UNITS CARGO_PROFILE_RELEASE_OPT_LEVEL ARG_CARGO_BUILD_JOBS ARG_CARGO_PROFILE_RELEASE_CODEGEN_UNITS ARG_CARGO_PROFILE_RELEASE_OPT_LEVEL
configure_release_build_resources 1024
[[ "$CARGO_BUILD_JOBS" == 1 && "$CARGO_PROFILE_RELEASE_CODEGEN_UNITS" == 1 && "$CARGO_PROFILE_RELEASE_OPT_LEVEL" == 0 ]]
unset CARGO_BUILD_JOBS CARGO_PROFILE_RELEASE_CODEGEN_UNITS CARGO_PROFILE_RELEASE_OPT_LEVEL ARG_CARGO_BUILD_JOBS ARG_CARGO_PROFILE_RELEASE_CODEGEN_UNITS ARG_CARGO_PROFILE_RELEASE_OPT_LEVEL
configure_release_build_resources 8192
[[ "$CARGO_BUILD_JOBS" == 2 && "$CARGO_PROFILE_RELEASE_CODEGEN_UNITS" == 8 && -z "${CARGO_PROFILE_RELEASE_OPT_LEVEL:-}" ]]
unset CARGO_BUILD_JOBS CARGO_PROFILE_RELEASE_CODEGEN_UNITS CARGO_PROFILE_RELEASE_OPT_LEVEL ARG_CARGO_BUILD_JOBS ARG_CARGO_PROFILE_RELEASE_CODEGEN_UNITS ARG_CARGO_PROFILE_RELEASE_OPT_LEVEL
configure_release_build_resources 16384
[[ "$CARGO_BUILD_JOBS" == 5 && "$CARGO_PROFILE_RELEASE_CODEGEN_UNITS" == 3 && -z "${CARGO_PROFILE_RELEASE_OPT_LEVEL:-}" ]]
unset CARGO_BUILD_JOBS CARGO_PROFILE_RELEASE_CODEGEN_UNITS CARGO_PROFILE_RELEASE_OPT_LEVEL ARG_CARGO_BUILD_JOBS ARG_CARGO_PROFILE_RELEASE_CODEGEN_UNITS ARG_CARGO_PROFILE_RELEASE_OPT_LEVEL
ARG_CARGO_BUILD_JOBS=20
configure_release_build_resources 8192
[[ "$CARGO_BUILD_JOBS" == 2 ]]
unset CARGO_BUILD_JOBS CARGO_PROFILE_RELEASE_CODEGEN_UNITS CARGO_PROFILE_RELEASE_OPT_LEVEL ARG_CARGO_BUILD_JOBS ARG_CARGO_PROFILE_RELEASE_CODEGEN_UNITS ARG_CARGO_PROFILE_RELEASE_OPT_LEVEL

# Fast mode holds Cargo's profile steady as free RAM changes, while scheduling
# more independent crates when the host has enough memory and CPU capacity.
ARG_UPDATE_CONTEXT=update ARG_UPDATE_MODE=fast
BUILD_CAPACITY_MB=8192
configure_release_build_resources 8192
[[ "$CARGO_BUILD_JOBS" == 4 && "$CARGO_PROFILE_RELEASE_CODEGEN_UNITS" == 4 && "$CARGO_PROFILE_RELEASE_OPT_LEVEL" == 2 ]]
fast_profile="$CARGO_PROFILE_RELEASE_OPT_LEVEL/$CARGO_PROFILE_RELEASE_CODEGEN_UNITS"
configure_release_build_resources 2048
[[ "$CARGO_BUILD_JOBS" == 1 && "$fast_profile" == "$CARGO_PROFILE_RELEASE_OPT_LEVEL/$CARGO_PROFILE_RELEASE_CODEGEN_UNITS" ]]
ARG_CARGO_BUILD_JOBS=20
configure_release_build_resources 8192
[[ "$CARGO_BUILD_JOBS" == 4 ]]
unset ARG_UPDATE_CONTEXT ARG_UPDATE_MODE BUILD_CAPACITY_MB ARG_CARGO_BUILD_JOBS CARGO_BUILD_JOBS CARGO_PROFILE_RELEASE_CODEGEN_UNITS CARGO_PROFILE_RELEASE_OPT_LEVEL ARG_CARGO_PROFILE_RELEASE_CODEGEN_UNITS ARG_CARGO_PROFILE_RELEASE_OPT_LEVEL

# Exercise temporary swap lifecycle with shell mocks. No host swap or disk is changed.
swap_test_dir="$tmp/swap"
mkdir -p "$swap_test_dir"
ARG_BUILD_SWAP_DIR="$swap_test_dir"
ARG_BUILD_SWAP_FILE=""
SWAP_TEST_ACTIVE=""
SWAP_TEST_FAIL_ONCE=0
SWAP_TEST_FREE_MB=9999
SWAP_TEST_FS=ext4
release_build_is_root() { return 0; }
df() { printf 'Filesystem 1M-blocks Used Available Use%% Mounted on\nmock 10000 1 %s 1%% /tmp\n' "$SWAP_TEST_FREE_MB"; }
findmnt() { printf '%s\n' "$SWAP_TEST_FS"; }
fallocate() { truncate -s "$2" "$3"; }
dd() {
  local output count
  for arg in "$@"; do
    case "$arg" in of=*) output="${arg#of=}" ;; count=*) count="${arg#count=}" ;; esac
  done
  truncate -s "${count}M" "$output"
}
mkswap() { [[ -f "$1" ]]; }
swapon() {
  if [[ "${1:-}" == "--show=NAME" ]]; then
    [[ -z "$SWAP_TEST_ACTIVE" ]] || printf '%s\n' "$SWAP_TEST_ACTIVE"
    return 0
  fi
  if (( SWAP_TEST_FAIL_ONCE )); then
    SWAP_TEST_FAIL_ONCE=0
    printf 'swapon: Invalid argument\n' >&2
    return 1
  fi
  SWAP_TEST_ACTIVE="$1"
}
swapoff() { [[ "$SWAP_TEST_ACTIVE" == "$1" ]] || return 1; SWAP_TEST_ACTIVE=""; }
prepare_release_build_swap 512
[[ -n "$ARG_BUILD_SWAP_FILE" && "$SWAP_TEST_ACTIVE" == "$ARG_BUILD_SWAP_FILE" && -s "$ARG_BUILD_SWAP_FILE" ]]
cleanup_release_build_swap
[[ -z "$ARG_BUILD_SWAP_FILE" && -z "$SWAP_TEST_ACTIVE" ]]
[[ -z "$(find "$swap_test_dir" -mindepth 1 -maxdepth 1 -type f -print -quit)" ]]
SWAP_TEST_FAIL_ONCE=1
prepare_release_build_swap 512
[[ -n "$ARG_BUILD_SWAP_FILE" && "$SWAP_TEST_ACTIVE" == "$ARG_BUILD_SWAP_FILE" ]]
cleanup_release_build_swap
SWAP_TEST_FREE_MB=256
if prepare_release_build_swap 512; then
  echo 'temporary swap ignored the free-disk guard' >&2
  exit 1
fi
[[ -z "$ARG_BUILD_SWAP_FILE" && -z "$(find "$swap_test_dir" -mindepth 1 -maxdepth 1 -type f -print -quit)" ]]
SWAP_TEST_FREE_MB=9999
SWAP_TEST_FS=tmpfs
if prepare_release_build_swap 512; then
  echo 'temporary swap was enabled on tmpfs' >&2
  exit 1
fi
[[ -z "$ARG_BUILD_SWAP_FILE" && -z "$(find "$swap_test_dir" -mindepth 1 -maxdepth 1 -type f -print -quit)" ]]
SWAP_TEST_FS=ext4
prepare_release_build_swap 2048
[[ -z "$ARG_BUILD_SWAP_FILE" && -z "$SWAP_TEST_ACTIVE" ]]
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
commands=()
run_with_progress() {
  calls+=("$1")
  commands+=("$*")
  if [[ "$1" == "Compiling the optimized Rust release" || "$1" == "Fast update: incrementally compiling the ARG application" ]]; then
    [[ "${ARG_TEST_BUILD_FAIL:-0}" != 1 ]] || return 137
    mkdir -p "$SOURCE_DIR/target/release"
    touch "$SOURCE_DIR/target/release/arg"
    chmod +x "$SOURCE_DIR/target/release/arg"
  fi
}
release_build_memory_budget_mb() { printf '512\n'; }

ARG_UPDATE_CONTEXT=update ARG_UPDATE_MODE=complete
build_release
[[ -z "$ARG_BUILD_SWAP_FILE" && -z "$SWAP_TEST_ACTIVE" ]]
[[ "${calls[*]}" == "Clearing previous Rust build cache Compiling the optimized Rust release" ]]
calls=()
commands=()
ARG_UPDATE_CONTEXT=update ARG_UPDATE_MODE=fast
build_release
[[ "${calls[*]}" == "Fast update: incrementally compiling the ARG application" ]]
[[ "${commands[*]}" == *'cargo +stable rustc --locked --release --bin arg -- -C opt-level="$3" -C incremental="$2"'* ]]
[[ "${commands[*]}" == *"$SOURCE_DIR/target/release/incremental/arg-fast 1"* ]]
[[ "${commands[*]}" == *"CARGO_INCREMENTAL=0"* ]]
calls=()
commands=()
ARG_UPDATE_CONTEXT= ARG_UPDATE_MODE=complete
build_release
[[ "${calls[*]}" == "Compiling the optimized Rust release" ]]
ARG_TEST_BUILD_FAIL=1
if build_release; then
  echo 'failed release compilation was reported as successful' >&2
  exit 1
fi
[[ -z "$ARG_BUILD_SWAP_FILE" && -z "$SWAP_TEST_ACTIVE" ]]
grep -q 'known-good-active-binary' "$BINARY"
unset ARG_TEST_BUILD_FAIL
ARG_UPDATE_MODE=fast
ARG_TEST_BUILD_FAIL=1
if build_release; then
  echo 'failed fast compilation was reported as successful' >&2
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
[[ "$(grep -c '+stable clean' "$ARG_TEST_CARGO_LOG")" == 1 ]]

(
  export PATH="$cleanup_dir/bin:$PATH"
  ARG_UPDATE_MODE=complete
  UPDATE_CLEANUP_BUILD_STARTED=1
  SOURCE_DIR="$tmp/source"
  cleanup_update_build_cache 0
)
[[ "$(grep -c '+stable clean' "$ARG_TEST_CARGO_LOG")" == 2 ]]

printf 'SSH update selection, low-memory build settings, and EXIT cleanup tests passed\n'
