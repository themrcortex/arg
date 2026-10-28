#!/usr/bin/env bash
set -Eeuo pipefail

readonly APP_NAME="ARG"
readonly SERVICE_NAME="arg"
readonly LEGACY_SERVICE_NAME="donatello"
readonly REPOSITORY="https://github.com/themrcortex/arg.git"
requested_branch="${ARG_UPDATE_BRANCH:-${ARG_BRANCH:-main}}"
case "$requested_branch" in
  main|development) ;;
  *) printf 'Invalid ARG update branch. Allowed: main, development.\n' >&2; exit 2 ;;
esac
BRANCH="$requested_branch"
readonly INSTALL_DIR="/opt/arg"
readonly SOURCE_DIR="/opt/arg/source"
readonly BINARY="/usr/local/lib/arg/arg"
readonly CLI="/usr/local/bin/arg"
readonly LEGACY_INSTALLER="/usr/local/sbin/arg-installer"
readonly OLD_CLI="/usr/local/bin/donatello"
readonly OLD_LEGACY_INSTALLER="/usr/local/sbin/donatello-installer"
readonly SERVICE_FILE="/etc/systemd/system/arg.service"
readonly PRIVILEGED_SERVICE_FILE="/etc/systemd/system/arg-privileged.service"
readonly OLD_SERVICE_FILE="/etc/systemd/system/donatello.service"
readonly OLD_PRIVILEGED_SERVICE_FILE="/etc/systemd/system/donatello-privileged.service"
readonly SUDOERS_FILE="/etc/sudoers.d/arg-ufw-status"
readonly UFW_HELPER="/usr/local/lib/arg/ufw-helper"
readonly PORT_HELPER="/usr/local/lib/arg/port-helper"
readonly UPDATE_HELPER="/usr/local/lib/arg/update-helper"
readonly DB_BACKUP_TOOL="/usr/local/lib/arg/arg-db-backup"
readonly UPDATE_LOCK_DIR="/run/arg-update.lock"
readonly UPDATE_MUTEX_FILE="/run/arg-update.lock.mutex"
readonly UPDATE_STATUS_FILE="/run/arg-update-in-progress"
readonly UPDATE_STATE_FILE="/run/arg-update-state.json"
readonly UPDATE_CANCEL_FILE="/run/arg-update-cancel-requested"
readonly ENV_DIR="/etc/arg"
readonly ENV_FILE="/etc/arg/arg.env"
readonly DOMAIN_FILE="/etc/arg/https-domain"
readonly LEGACY_NGINX_SITE="/etc/nginx/sites-available/arg"
readonly LEGACY_NGINX_SITE_LINK="/etc/nginx/sites-enabled/arg"
readonly TLS_DIR="/etc/arg/tls"
readonly TLS_CERT="$TLS_DIR/fullchain.pem"
readonly TLS_KEY="$TLS_DIR/privkey.pem"
readonly CERT_DEPLOY_HOOK="/etc/letsencrypt/renewal-hooks/deploy/arg"
readonly CERT_NAME="arg"
readonly CERTBOT_TIMER_SERVICE="/etc/systemd/system/arg-certbot-renew.service"
readonly CERTBOT_TIMER_UNIT="/etc/systemd/system/arg-certbot-renew.timer"
readonly DATA_DIR="/var/lib/arg"
readonly ROLLBACK_DIR="/var/lib/arg/rollback"
readonly PREVIOUS_BINARY="$ROLLBACK_DIR/arg.previous"
readonly PREVIOUS_DATABASE="$ROLLBACK_DIR/arg.db.previous"
readonly OLD_INSTALL_DIR="/opt/donatello"
readonly OLD_SOURCE_DIR="/opt/donatello/source"
readonly OLD_ENV_DIR="/etc/donatello"
readonly OLD_DATA_DIR="/var/lib/donatello"
readonly OLD_NGINX_SITE="/etc/nginx/sites-available/donatello"
readonly OLD_NGINX_SITE_LINK="/etc/nginx/sites-enabled/donatello"
readonly OLD_CERT_DEPLOY_HOOK="/etc/letsencrypt/renewal-hooks/deploy/donatello"
readonly OLD_CERTBOT_TIMER_SERVICE="/etc/systemd/system/donatello-certbot-renew.service"
readonly OLD_CERTBOT_TIMER_UNIT="/etc/systemd/system/donatello-certbot-renew.timer"
readonly SERVICE_USER="arg"
readonly SERVICE_GROUP="arg"
readonly DEFAULT_BIND="0.0.0.0:8282"
readonly DEFAULT_DB="/var/lib/arg/arg.db"
SERVER_PUBLIC_IP=""
PORT80_STOPPED_UNIT=""
CERTBOT_BIN=""
MENU_ACTIVE=0
UPDATE_STATUS="NOT CHECKED"
UPDATE_LOCAL_REV=""
UPDATE_REMOTE_REV=""
MENU_STATUS_DIR=""
MENU_STATUS_PID=""
UPDATE_CLEANUP_BUILD_STARTED=0
UPDATE_CLEANUP_BINARY_SWAPPED=0
UPDATE_CLEANUP_STAGED=""
UPDATE_CLEANUP_DATABASE_STAGED=""

readonly RUST_HOME="${RUSTUP_HOME:-/root/.rustup}"
readonly CARGO_HOME_DIR="${CARGO_HOME:-/root/.cargo}"
export RUSTUP_HOME="$RUST_HOME"
export CARGO_HOME="$CARGO_HOME_DIR"
export PATH="$CARGO_HOME_DIR/bin:$PATH"

if [[ -t 1 && -z "${NO_COLOR:-}" && "${TERM:-}" != "dumb" ]]; then
  readonly C_RESET=$'\033[0m'
  readonly C_BOLD=$'\033[1m'
  readonly C_COPPER=$'\033[38;5;215m'
  readonly C_CYAN=$'\033[1;36m'
  readonly C_GREEN=$'\033[1;32m'
  readonly C_YELLOW=$'\033[1;33m'
  readonly C_RED=$'\033[1;31m'
  readonly C_DIM=$'\033[2m'
else
  readonly C_RESET=''
  readonly C_BOLD=''
  readonly C_COPPER=''
  readonly C_CYAN=''
  readonly C_GREEN=''
  readonly C_YELLOW=''
  readonly C_RED=''
  readonly C_DIM=''
fi

say() { printf '\n%s%s%s\n' "$C_COPPER" "$*" "$C_RESET"; }
info() { printf '  %s\n' "$*"; }
warn() { printf '%s  WARNING: %s%s\n' "$C_YELLOW" "$*" "$C_RESET" >&2; }
die() { printf '%s  ERROR: %s%s\n' "$C_RED" "$*" "$C_RESET" >&2; exit 1; }
has() { command -v "$1" >/dev/null 2>&1; }

show_wordmark() {
  printf '%s' "$C_COPPER"
  cat <<'BANNER'
   █████╗  ██████╗   ██████╗
  ██╔══██╗ ██╔══██╗ ██╔════╝
  ███████║ ██████╔╝ ██║  ███╗
  ██╔══██║ ██╔══██╗ ██║   ██║
  ██║  ██║ ██║  ██║ ╚██████╔╝
  ╚═╝  ╚═╝ ╚═╝  ╚═╝  ╚═════╝
BANNER
  printf '%s' "$C_RESET"
}

show_banner() {
  show_wordmark
  printf '  %sVPS MANAGEMENT%s  %s·%s  %sINSTALLER & SERVICE CONSOLE%s\n' \
    "$C_BOLD" "$C_RESET" "$C_DIM" "$C_RESET" "$C_CYAN" "$C_RESET"
  printf '  %s\n\n' '────────────────────────────────────────────────────────────────────────────'
}

pause_menu() {
  [[ -r /dev/tty ]] || return 0
  local _answer
  IFS= read -r -p 'Press Enter to continue...' _answer </dev/tty || true
}

read_answer() {
  local prompt="$1" variable="$2" default="${3:-}" _answer_value=""
  if [[ -r /dev/tty && -t 1 ]]; then
    IFS= read -r -p "$prompt" _answer_value </dev/tty || true
  else
    IFS= read -r -p "$prompt" _answer_value || true
  fi
  printf -v "$variable" '%s' "${_answer_value:-$default}"
}

normalize_menu_digits() {
  local value="$1" pair native ascii
  local -a digit_pairs=(
    '۰:0' '۱:1' '۲:2' '۳:3' '۴:4' '۵:5' '۶:6' '۷:7' '۸:8' '۹:9'
    '٠:0' '١:1' '٢:2' '٣:3' '٤:4' '٥:5' '٦:6' '٧:7' '٨:8' '٩:9'
  )
  for pair in "${digit_pairs[@]}"; do
    native="${pair%%:*}"
    ascii="${pair#*:}"
    value="${value//"$native"/$ascii}"
  done
  printf '%s' "$value"
}

valid_menu_choice() {
  case "$1" in
    0|1|2|3|4|5|6|7|8|9|10|11|12|13|14|15) return 0 ;;
    *) return 1 ;;
  esac
}

require_root() {
  [[ "${EUID:-$(id -u)}" -eq 0 ]] && return 0
  if [[ "$1" == "install" ]]; then
    die "Run the installer as root: sudo bash install.sh install"
  fi
  die "Run this command as root: sudo arg $1"
}

require_systemd() {
  has systemctl || die "systemctl was not found on this system."
  [[ -d /run/systemd/system ]] || die "systemd is not running; the service cannot be managed."
}

install_os_dependencies() {
  [[ -r /etc/os-release ]] || die "Only Debian and Ubuntu are supported."
  # shellcheck disable=SC1091
  source /etc/os-release
  case "${ID:-}" in
    debian|ubuntu) ;;
    *) die "${PRETTY_NAME:-This Linux distribution} is not supported. Debian or Ubuntu is required." ;;
  esac
  has apt-get || die "apt-get was not found on this system."
  say "Installing build dependencies"
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends ca-certificates curl git openssh-client sudo build-essential pkg-config iproute2 ripgrep
}

ensure_firewall_dependency() {
  has ufw && return 0
  [[ -r /etc/os-release ]] || die "Cannot install UFW: /etc/os-release is missing."
  source /etc/os-release
  [[ "${ID:-}" == debian || "${ID:-}" == ubuntu ]] || die "UFW installation is supported on Debian and Ubuntu only."
  say "Installing UFW for panel firewall management"
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends ufw
}

ensure_rust() {
  if [[ ! -x "$CARGO_HOME_DIR/bin/rustup" ]]; then
    say "Installing the Rust stable toolchain"
    local installer
    installer="$(mktemp /tmp/arg-rustup.XXXXXX)"
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$installer"
    sh "$installer" -y --profile minimal --default-toolchain stable --no-modify-path
    rm -f "$installer"
  fi
  "$CARGO_HOME_DIR/bin/rustup" toolchain install stable --profile minimal
  "$CARGO_HOME_DIR/bin/rustup" default stable
  info "$(rustc +stable --version)"
}

prepare_source() {
  mkdir -p "$INSTALL_DIR"
  if [[ -d "$SOURCE_DIR/.git" ]]; then
    say "Fetching the latest source from GitHub"
    git -C "$SOURCE_DIR" fetch --depth 1 origin "$BRANCH"
    git -C "$SOURCE_DIR" reset --hard FETCH_HEAD
  elif [[ -e "$SOURCE_DIR" ]]; then
    die "$SOURCE_DIR exists but is not a Git repository. Please inspect it before continuing."
  else
    say "Downloading ARG source from GitHub"
    git clone --depth 1 --branch "$BRANCH" "$REPOSITORY" "$SOURCE_DIR"
  fi
}

configure_release_build_resources() {
  local memory_budget_mb="${1:-0}"
  [[ "$memory_budget_mb" =~ ^[0-9]+$ ]] || memory_budget_mb=0
  if (( memory_budget_mb < 4096 )); then
    export CARGO_BUILD_JOBS="${ARG_CARGO_BUILD_JOBS:-1}"
    export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${ARG_CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-1}"
    if (( memory_budget_mb < 2048 )); then
      export CARGO_PROFILE_RELEASE_OPT_LEVEL="${ARG_CARGO_PROFILE_RELEASE_OPT_LEVEL:-0}"
      warn "Available RAM and swap are below 2 GiB; using the minimum-memory release profile (1 job, opt-level 0, 1 codegen unit)."
    else
      export CARGO_PROFILE_RELEASE_OPT_LEVEL="${ARG_CARGO_PROFILE_RELEASE_OPT_LEVEL:-2}"
      warn "Available RAM and swap are below 4 GiB; using a lower-memory release build (1 job, opt-level 2, 1 codegen unit)."
    fi
  fi
}

ARG_BUILD_SWAP_FILE=""

release_build_is_root() {
  (( EUID == 0 ))
}

release_build_swap_active() {
  local swap_file="${1:-}"
  [[ -n "$swap_file" ]] || return 1
  swapon --show=NAME --noheadings --raw 2>/dev/null | awk -v path="$swap_file" '$0 == path { found=1 } END { exit !found }'
}

prepare_release_build_swap() {
  local memory_budget_mb="${1:-0}" swap_dir="${ARG_BUILD_SWAP_DIR:-/var/tmp}"
  local swap_size_mb=2048 free_disk_mb swap_path filesystem activation_error="" swap_error_file
  [[ "$memory_budget_mb" =~ ^[0-9]+$ ]] || memory_budget_mb=0
  (( memory_budget_mb < 2048 )) || return 0

  if (( memory_budget_mb >= 1024 )); then
    swap_size_mb=$((3072 - memory_budget_mb))
  fi
  (( swap_size_mb >= 512 )) || swap_size_mb=512
  (( swap_size_mb <= 2048 )) || swap_size_mb=2048

  if ! release_build_is_root || ! command -v swapon >/dev/null 2>&1 || ! command -v swapoff >/dev/null 2>&1 || ! command -v mkswap >/dev/null 2>&1; then
    warn "Temporary build swap could not be prepared (root privileges or util-linux swap tools are unavailable); continuing with opt-level 0."
    return 1
  fi
  [[ -d "$swap_dir" && -w "$swap_dir" ]] || {
    warn "Temporary build swap could not be prepared because $swap_dir is not writable; continuing with opt-level 0."
    return 1
  }
  free_disk_mb="$(df -Pm "$swap_dir" 2>/dev/null | awk 'NR == 2 { print $4 }')"
  [[ "$free_disk_mb" =~ ^[0-9]+$ ]] || free_disk_mb=0
  if (( free_disk_mb < swap_size_mb + 256 )); then
    warn "Temporary build swap needs ${swap_size_mb} MiB plus a safety margin, but $swap_dir has only ${free_disk_mb} MiB free; continuing with opt-level 0."
    return 1
  fi

  swap_path="$swap_dir/arg-build-${BASHPID}.swap"
  if [[ -e "$swap_path" ]]; then
    warn "Temporary build swap path already exists ($swap_path); refusing to overwrite it."
    return 1
  fi
  (umask 077; : > "$swap_path") || {
    warn "Could not create temporary build swap at $swap_path; continuing with opt-level 0."
    return 1
  }
  ARG_BUILD_SWAP_FILE="$swap_path"
  chmod 0600 "$swap_path" || { rm -f "$swap_path"; ARG_BUILD_SWAP_FILE=""; warn "Could not secure temporary build swap permissions."; return 1; }
  filesystem="$(findmnt -n -o FSTYPE --target "$swap_path" 2>/dev/null || true)"
  case "$filesystem" in
    ext2|ext3|ext4|xfs|btrfs) ;;
    *)
      rm -f "$swap_path"
      ARG_BUILD_SWAP_FILE=""
      warn "Temporary swap is disabled on the $filesystem filesystem at $swap_dir; continuing with opt-level 0."
      return 1
      ;;
  esac
  if [[ "$filesystem" == btrfs ]] && ! chattr +C "$swap_path" 2>/dev/null; then
    rm -f "$swap_path"
    ARG_BUILD_SWAP_FILE=""
    warn "Could not mark the temporary swap file as no-copy-on-write on Btrfs; continuing with opt-level 0."
    return 1
  fi

  # fallocate is fast on ext4/xfs. If the filesystem/kernel rejects its extents
  # for swap, retry with a fully written file; never leave an unactivated file.
  info "Preparing ${swap_size_mb} MiB of temporary swap for the Rust build."
  if ! fallocate -l "${swap_size_mb}M" "$swap_path" 2>/dev/null; then
    if ! truncate -s 0 "$swap_path" 2>/dev/null || ! dd if=/dev/zero of="$swap_path" bs=1M count="$swap_size_mb" conv=fsync status=none 2>/dev/null; then
      rm -f "$swap_path"
      ARG_BUILD_SWAP_FILE=""
      warn "Could not allocate ${swap_size_mb} MiB for temporary build swap; continuing with opt-level 0."
      return 1
    fi
  fi
  chmod 0600 "$swap_path"
  if ! mkswap "$swap_path" >/dev/null 2>&1; then
    rm -f "$swap_path"
    ARG_BUILD_SWAP_FILE=""
    warn "The filesystem rejected temporary build swap; continuing with opt-level 0."
    return 1
  fi
  swap_error_file="$(mktemp /tmp/arg-swapon.XXXXXX 2>/dev/null || true)"
  if [[ -z "$swap_error_file" ]]; then
    rm -f "$swap_path"
    ARG_BUILD_SWAP_FILE=""
    warn "Could not create a temporary error log for swap activation; continuing with opt-level 0."
    return 1
  fi
  if ! swapon "$swap_path" >/dev/null 2>"$swap_error_file"; then
    activation_error="$(cat "$swap_error_file" 2>/dev/null || true)"
  fi
  rm -f "$swap_error_file"
  if [[ -n "$activation_error" ]] && ! release_build_swap_active "$swap_path"; then
    if [[ "$activation_error" =~ [Ii]nvalid\ argument|[Hh]oles|[Nn]ot\ supported ]]; then
      if truncate -s 0 "$swap_path" 2>/dev/null && dd if=/dev/zero of="$swap_path" bs=1M count="$swap_size_mb" conv=fsync status=none 2>/dev/null && mkswap "$swap_path" >/dev/null 2>&1 && swapon "$swap_path" >/dev/null 2>&1 && release_build_swap_active "$swap_path"; then
        :
      else
        rm -f "$swap_path"
        ARG_BUILD_SWAP_FILE=""
        warn "The kernel/filesystem rejected the temporary swap layout; continuing with opt-level 0."
        return 1
      fi
    fi
  fi
  if ! release_build_swap_active "$swap_path"; then
    rm -f "$swap_path"
    ARG_BUILD_SWAP_FILE=""
    warn "The kernel did not allow temporary swap activation${activation_error:+: $activation_error}; continuing with opt-level 0."
    return 1
  fi
  ARG_BUILD_SWAP_FILE="$swap_path"
  info "Added ${swap_size_mb} MiB of temporary swap for the Rust build."
}

cleanup_release_build_swap() {
  local swap_path="${ARG_BUILD_SWAP_FILE:-}"
  [[ -n "$swap_path" ]] || return 0
  if release_build_swap_active "$swap_path" && ! swapoff "$swap_path" >/dev/null 2>&1; then
    warn "The build finished, but temporary swap is still in use and could not be disabled. It remains safely enabled at $swap_path."
    ARG_BUILD_SWAP_FILE=""
    return 1
  fi
  rm -f -- "$swap_path" || { warn "Could not remove temporary build swap file $swap_path."; ARG_BUILD_SWAP_FILE=""; return 1; }
  ARG_BUILD_SWAP_FILE=""
  info "Removed temporary Rust build swap."
}

release_build_memory_budget_mb() {
  local available_kb swap_free_kb
  available_kb="$(awk '$1 == "MemAvailable:" { print $2; found=1 } END { if (!found) print 0 }' /proc/meminfo 2>/dev/null || true)"
  swap_free_kb="$(awk '$1 == "SwapFree:" { print $2; found=1 } END { if (!found) print 0 }' /proc/meminfo 2>/dev/null || true)"
  [[ "$available_kb" =~ ^[0-9]+$ ]] || available_kb=0
  [[ "$swap_free_kb" =~ ^[0-9]+$ ]] || swap_free_kb=0
  printf '%s\n' "$(((available_kb + swap_free_kb) / 1024))"
}

release_build_disk_space_ok() {
  local available_mb="${1:-0}" required_mb="${2:-0}"
  [[ "$available_mb" =~ ^[0-9]+$ && "$required_mb" =~ ^[0-9]+$ ]] || return 1
  (( available_mb >= required_mb ))
}

build_release() {
  local available_disk_mb required_disk_mb=2048 memory_budget_mb compile_result=0
  [[ -f "$SOURCE_DIR/frontend/dist/index.html" ]] || die "Built frontend files are missing. Check the selected ARG branch and repository."
  if [[ "${ARG_UPDATE_CONTEXT:-}" == "update" && "${ARG_UPDATE_MODE:-complete}" == "complete" ]]; then
    run_with_progress "Clearing previous Rust build cache" bash -c 'cd "$1" && cargo +stable clean' _ "$SOURCE_DIR"
  fi
  [[ "${ARG_UPDATE_CONTEXT:-}" != "update" || "${ARG_UPDATE_MODE:-complete}" != "fast" ]] || required_disk_mb=1024
  available_disk_mb="$(df -Pm "$SOURCE_DIR" | awk 'NR == 2 { print $4 }')"
  if ! release_build_disk_space_ok "$available_disk_mb" "$required_disk_mb"; then
    die "Not enough free disk space to compile ARG safely (${available_disk_mb:-unknown} MiB available; at least ${required_disk_mb} MiB required). Free disk space, then retry."
  fi
  memory_budget_mb="$(release_build_memory_budget_mb)"
  configure_release_build_resources "$memory_budget_mb"
  trap 'cleanup_release_build_swap; exit 130' INT
  trap 'cleanup_release_build_swap; exit 143' TERM
  prepare_release_build_swap "$memory_budget_mb" || true
  if run_with_progress "Compiling the optimized Rust release" bash -c 'cd "$1" && cargo +stable build --locked --release' _ "$SOURCE_DIR"; then
    compile_result=0
  else
    compile_result=$?
  fi
  trap - INT TERM
  cleanup_release_build_swap || true
  if (( compile_result != 0 )); then
    if [[ "${ARG_UPDATE_CONTEXT:-}" != "update" ]]; then
      bash -c 'cd "$1" && cargo +stable clean' _ "$SOURCE_DIR" >/dev/null 2>&1 || warn "The failed first-install build cache could not be removed; free space manually under $SOURCE_DIR/target."
    fi
    warn "Rust compilation failed before the new ARG binary was installed; the currently active binary, if any, was left untouched. Review available memory and disk space, then retry the update."
    return 1
  fi
  [[ -x "$SOURCE_DIR/target/release/arg" ]] || die "The ARG binary was not created."
}

run_with_progress() {
  local label="$1" log_file spinner_index=0 started elapsed minutes seconds pid result
  shift
  log_file="$(mktemp /tmp/arg-build.XXXXXX)"

  if [[ ! -t 1 || ! -t 2 ]]; then
    say "$label"
    set +e
    "$@" 2>&1 | tee "$log_file"
    local -a pipeline_status=("${PIPESTATUS[@]}")
    set -e
    result="${pipeline_status[0]}"
    if (( result == 0 )); then
      info "Build completed successfully."
      rm -f "$log_file"
      return 0
    else
      rm -f "$log_file"
      return "$result"
    fi
  fi

  say "$label"
  "$@" >"$log_file" 2>&1 &
  pid=$!
  started=$SECONDS
  local -a frames=('⠋' '⠙' '⠹' '⠸' '⠼' '⠴' '⠦' '⠧' '⠇' '⠏')
  while kill -0 "$pid" 2>/dev/null; do
    elapsed=$((SECONDS - started))
    minutes=$((elapsed / 60))
    seconds=$((elapsed % 60))
    printf '\r  %s%s%s  Building ARG… %02d:%02d' \
      "$C_CYAN" "${frames[spinner_index]}" "$C_RESET" "$minutes" "$seconds"
    spinner_index=$(((spinner_index + 1) % ${#frames[@]}))
    sleep 0.12
  done

  if wait "$pid"; then
    elapsed=$((SECONDS - started))
    minutes=$((elapsed / 60))
    seconds=$((elapsed % 60))
    printf '\r  %s✓%s  Build completed successfully in %02d:%02d.\n' \
      "$C_GREEN" "$C_RESET" "$minutes" "$seconds"
    rm -f "$log_file"
  else
    result=$?
    printf '\r  %s✗%s  Build failed. Compiler output follows:\n' "$C_RED" "$C_RESET" >&2
    cat "$log_file" >&2
    rm -f "$log_file"
    return "$result"
  fi
}

ensure_service_account() {
  if ! getent group "$SERVICE_GROUP" >/dev/null; then
    groupadd --system "$SERVICE_GROUP"
  fi
  if ! id "$SERVICE_USER" >/dev/null 2>&1; then
    useradd --system --gid "$SERVICE_GROUP" --home-dir "$DATA_DIR" --shell /usr/sbin/nologin "$SERVICE_USER"
  fi
  install -d -o "$SERVICE_USER" -g "$SERVICE_GROUP" -m 0750 "$DATA_DIR"
  install -d -o "$SERVICE_USER" -g "$SERVICE_GROUP" -m 0700 "$DATA_DIR/s3-transfer"
  install -d -o root -g "$SERVICE_GROUP" -m 0750 "$ENV_DIR"
}

migrate_legacy_install() {
  local legacy_found=0
  [[ -e "$OLD_SERVICE_FILE" || -e "$OLD_PRIVILEGED_SERVICE_FILE" || -d "$OLD_ENV_DIR" || -d "$OLD_DATA_DIR" || -d "$OLD_SOURCE_DIR" ]] && legacy_found=1
  (( legacy_found )) || return 0

  say "Migrating the previous installation to ARG"
  systemctl disable --now donatello-certbot-renew.timer 2>/dev/null || true
  systemctl stop "$LEGACY_SERVICE_NAME" "$LEGACY_SERVICE_NAME-privileged" 2>/dev/null || true
  systemctl disable "$LEGACY_SERVICE_NAME" "$LEGACY_SERVICE_NAME-privileged" 2>/dev/null || true

  install -d -o root -g root -m 0755 "$INSTALL_DIR"
  if [[ -d "$OLD_ENV_DIR" && ! -e "$ENV_DIR" ]]; then
    mv "$OLD_ENV_DIR" "$ENV_DIR"
  else
    install -d -o root -g root -m 0755 "$ENV_DIR"
    [[ ! -d "$OLD_ENV_DIR" ]] || cp -a -n "$OLD_ENV_DIR"/. "$ENV_DIR"/
  fi
  if [[ -d "$OLD_DATA_DIR" && ! -e "$DATA_DIR" ]]; then
    mv "$OLD_DATA_DIR" "$DATA_DIR"
  else
    install -d -o root -g root -m 0755 "$DATA_DIR"
    [[ ! -d "$OLD_DATA_DIR" ]] || cp -a -n "$OLD_DATA_DIR"/. "$DATA_DIR"/
  fi
  if [[ -d "$OLD_SOURCE_DIR/.git" && ! -e "$SOURCE_DIR" ]]; then
    mv "$OLD_SOURCE_DIR" "$SOURCE_DIR"
  fi

  if [[ -f "$ENV_DIR/donatello.env" && ! -e "$ENV_FILE" ]]; then
    mv "$ENV_DIR/donatello.env" "$ENV_FILE"
  fi
  if [[ -f "$ENV_FILE" ]]; then
    sed -i \
      -e 's/^DONATELLO_/ARG_/' \
      -e 's#/var/lib/donatello#/var/lib/arg#g' \
      -e 's#/etc/donatello#/etc/arg#g' \
      -e 's#/usr/local/lib/donatello#/usr/local/lib/arg#g' \
      -e 's#/donatello\.db#/arg.db#g' "$ENV_FILE"
  fi
  for suffix in '' -wal -shm; do
    if [[ -f "$DATA_DIR/donatello.db$suffix" && ! -e "$DATA_DIR/arg.db$suffix" ]]; then
      mv "$DATA_DIR/donatello.db$suffix" "$DATA_DIR/arg.db$suffix"
    fi
  done
  if [[ -f "$DATA_DIR/.database-path" ]]; then
    sed -i -e 's#/var/lib/donatello#/var/lib/arg#g' -e 's#/donatello\.db#/arg.db#g' "$DATA_DIR/.database-path"
  fi

  if [[ -d "$SOURCE_DIR/.git" ]]; then
    git -C "$SOURCE_DIR" remote set-url origin "$REPOSITORY" 2>/dev/null || true
  fi
  if [[ -f "$OLD_NGINX_SITE" && ! -e "$LEGACY_NGINX_SITE" ]]; then
    mv "$OLD_NGINX_SITE" "$LEGACY_NGINX_SITE"
    if [[ -L "$OLD_NGINX_SITE_LINK" || -e "$OLD_NGINX_SITE_LINK" ]]; then
      rm -f "$OLD_NGINX_SITE_LINK"
      ln -s "$LEGACY_NGINX_SITE" "$LEGACY_NGINX_SITE_LINK"
    fi
  fi
  if [[ -f "$OLD_CERT_DEPLOY_HOOK" && ! -e "$CERT_DEPLOY_HOOK" ]]; then
    mv "$OLD_CERT_DEPLOY_HOOK" "$CERT_DEPLOY_HOOK"
    sed -i \
      -e 's/DONATELLO/ARG/g' \
      -e 's#/etc/donatello#/etc/arg#g' \
      -e 's/donatello\.service/arg.service/g' "$CERT_DEPLOY_HOOK"
  fi
  rm -f "$OLD_SERVICE_FILE" "$OLD_PRIVILEGED_SERVICE_FILE" \
    "$OLD_CERTBOT_TIMER_SERVICE" "$OLD_CERTBOT_TIMER_UNIT" \
    /etc/sudoers.d/donatello-ufw-status
  systemctl daemon-reload
  chmod 0640 "$ENV_FILE" 2>/dev/null || true
  chmod 0750 "$ENV_DIR" "$DATA_DIR"
}

cleanup_legacy_identity() {
  rm -rf /usr/local/lib/donatello
  rm -f "$OLD_CLI" "$OLD_LEGACY_INSTALLER" /usr/local/sbin/donatello-installer \
    /etc/sudoers.d/donatello-ufw-status
  rmdir "$OLD_INSTALL_DIR" 2>/dev/null || true
  if id donatello >/dev/null 2>&1; then userdel donatello 2>/dev/null || true; fi
  if getent group donatello >/dev/null; then groupdel donatello 2>/dev/null || true; fi
}

configure_ufw_status() {
  local ufw sudoers_tmp
  ufw="$(command -v ufw || true)"
  has visudo || die "visudo is required to configure restricted panel management."
  install -d -o root -g root -m 0755 "$(dirname "$PORT_HELPER")"
  install -o root -g root -m 0755 "$SOURCE_DIR/scripts/port-helper.sh" "$PORT_HELPER"
  install -o root -g root -m 0755 "$SOURCE_DIR/scripts/update-helper.sh" "$UPDATE_HELPER"
  if [[ -n "$ufw" ]]; then
    install -o root -g root -m 0755 "$SOURCE_DIR/scripts/ufw-helper.sh" "$UFW_HELPER"
  else
    # Keep the helper available so an administrator can install UFW from the web panel.
    install -o root -g root -m 0755 "$SOURCE_DIR/scripts/ufw-helper.sh" "$UFW_HELPER"
  fi
  sudoers_tmp="$(mktemp /etc/sudoers.d/.arg-ufw.XXXXXX)"
  printf '%s ALL=(root) NOPASSWD: %s *\n' "$SERVICE_USER" "$PORT_HELPER" > "$sudoers_tmp"
  printf '%s ALL=(root) NOPASSWD: %s check, %s run, %s logs\n' "$SERVICE_USER" "$UPDATE_HELPER" "$UPDATE_HELPER" "$UPDATE_HELPER" >> "$sudoers_tmp"
  chmod 0440 "$sudoers_tmp"
  if ! visudo -cf "$sudoers_tmp" >/dev/null; then
    rm -f "$sudoers_tmp"
    die "The restricted UFW sudoers rule did not pass validation."
  fi
  mv -f "$sudoers_tmp" "$SUDOERS_FILE"
}

write_default_config() {
  local db_path
  db_path="$(configured_db_path)"
  if [[ ! -f "$ENV_FILE" ]]; then
    install -o root -g "$SERVICE_GROUP" -m 0640 /dev/null "$ENV_FILE"
    printf 'ARG_BIND=%s\nARG_DB=%s\n' "$DEFAULT_BIND" "$db_path" > "$ENV_FILE"
  fi
  chown root:"$SERVICE_GROUP" "$ENV_FILE"
  chmod 0640 "$ENV_FILE"
}

configured_db_path() {
  local db_path=""
  if [[ -f "$ENV_FILE" ]]; then
    db_path="$(awk -F= '$1 == "ARG_DB" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE")"
  elif [[ -f "$DATA_DIR/.database-path" ]]; then
    db_path="$(<"$DATA_DIR/.database-path")"
  fi
  [[ -n "$db_path" ]] || db_path="$DEFAULT_DB"
  [[ "$db_path" == /* ]] || db_path="$DATA_DIR/$db_path"
  printf '%s\n' "$db_path"
}

take_database_ownership() {
  local db_path file
  db_path="$(configured_db_path)"
  chown -R "$SERVICE_USER:$SERVICE_GROUP" "$DATA_DIR"
  chown -R root:"$SERVICE_GROUP" "$ENV_DIR"
  find "$ENV_DIR" -type d -exec chmod 0750 {} +
  find "$ENV_DIR" -type f -exec chmod 0640 {} +
  for file in "$db_path" "$db_path-wal" "$db_path-shm"; do
    [[ -f "$file" ]] || continue
    chown "$SERVICE_USER:$SERVICE_GROUP" "$file"
    chmod 0600 "$file"
  done
}

panel_port() {
  local bind
  bind="$(awk -F= '$1 == "ARG_BIND" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
  bind="${bind:-$DEFAULT_BIND}"
  printf '%s\n' "${bind##*:}"
}

detect_public_ipv4() {
  local address endpoint octet valid index
  local -a octets
  # Public-IP services can be unreachable from some providers or filtered
  # networks. Try independent HTTPS endpoints and accept only globally usable
  # IPv4 addresses; never mistake a proxy/private address for the VPS address.
  for endpoint in \
    https://api.ipify.org \
    https://checkip.amazonaws.com \
    https://ipv4.icanhazip.com \
    https://ifconfig.me/ip; do
    address="$(curl -4 --connect-timeout 3 --max-time 6 -fsS "$endpoint" 2>/dev/null | tr -d '[:space:]' || true)"
    [[ "$address" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]] || continue
    IFS=. read -r -a octets <<< "$address"
    valid=1
    for index in 0 1 2 3; do
      octet="${octets[$index]}"
      octets[$index]="$((10#$octet))"
      (( octets[$index] <= 255 )) || { valid=0; break; }
    done
    (( valid == 1 )) || continue
    # Reject non-public ranges, including RFC1918, loopback, link-local,
    # carrier-grade NAT, and multicast/reserved address space.
    (( octets[0] != 0 && octets[0] != 10 && octets[0] != 127 && octets[0] < 224 )) || continue
    (( octets[0] != 169 || octets[1] != 254 )) || continue
    (( octets[0] != 172 || octets[1] < 16 || octets[1] > 31 )) || continue
    (( octets[0] != 192 || octets[1] != 168 )) || continue
    (( octets[0] != 100 || octets[1] < 64 || octets[1] > 127 )) || continue
    printf '%d.%d.%d.%d\n' "${octets[0]}" "${octets[1]}" "${octets[2]}" "${octets[3]}"
    return 0
  done
  return 1
}

check_update_status() {
  local local_rev remote_output remote_rev
  UPDATE_STATUS="CHECK FAILED"
  UPDATE_LOCAL_REV=""
  UPDATE_REMOTE_REV=""
  [[ -f "$SERVICE_FILE" ]] || { UPDATE_STATUS="NOT INSTALLED"; return 0; }
  [[ -d "$SOURCE_DIR/.git" ]] || return 0
  local_rev="$(git -C "$SOURCE_DIR" rev-parse --verify HEAD 2>/dev/null || true)"
  [[ "$local_rev" =~ ^[0-9a-f]{40}$ ]] || return 0
  remote_output="$(GIT_TERMINAL_PROMPT=0 timeout 4s git -c credential.helper= ls-remote "$REPOSITORY" "refs/heads/$BRANCH" 2>/dev/null || true)"
  remote_rev="${remote_output%%[[:space:]]*}"
  [[ "$remote_rev" =~ ^[0-9a-f]{40}$ ]] || return 0
  UPDATE_LOCAL_REV="${local_rev:0:7}"
  UPDATE_REMOTE_REV="${remote_rev:0:7}"
  if [[ "$local_rev" == "$remote_rev" ]]; then
    UPDATE_STATUS="UP TO DATE"
  else
    UPDATE_STATUS="AVAILABLE"
  fi
}

start_menu_status_refresh() {
  [[ -f "$SERVICE_FILE" ]] || return 0
  MENU_STATUS_DIR="$(mktemp -d "${TMPDIR:-/tmp}/arg-menu-status.XXXXXX")"
  chmod 0700 "$MENU_STATUS_DIR"
  (
    check_update_status
    local public_ip="$SERVER_PUBLIC_IP"
    [[ -n "$public_ip" ]] || public_ip="$(detect_public_ipv4 || true)"
    printf '%s|%s|%s|%s\n' "$UPDATE_STATUS" "$UPDATE_LOCAL_REV" "$UPDATE_REMOTE_REV" "$public_ip" > "$MENU_STATUS_DIR/result.tmp"
    mv -f "$MENU_STATUS_DIR/result.tmp" "$MENU_STATUS_DIR/result"
  ) >/dev/null 2>&1 </dev/null &
  MENU_STATUS_PID=$!
}

read_menu_status_refresh() {
  [[ -n "$MENU_STATUS_DIR" && -s "$MENU_STATUS_DIR/result" ]] || return 1
  IFS='|' read -r UPDATE_STATUS UPDATE_LOCAL_REV UPDATE_REMOTE_REV SERVER_PUBLIC_IP < "$MENU_STATUS_DIR/result"
  rm -f "$MENU_STATUS_DIR/result"
  rmdir "$MENU_STATUS_DIR" 2>/dev/null || true
  MENU_STATUS_DIR=""
  MENU_STATUS_PID=""
  return 0
}

stop_menu_status_refresh() {
  [[ -n "$MENU_STATUS_PID" ]] && kill "$MENU_STATUS_PID" 2>/dev/null || true
  [[ -n "$MENU_STATUS_DIR" ]] && rm -rf "$MENU_STATUS_DIR"
  MENU_STATUS_PID=""
  MENU_STATUS_DIR=""
}

write_bind_config() {
  local bind="$1" db_path temp
  db_path="$(configured_db_path)"
  temp="$(mktemp /etc/arg/.arg.env.XXXXXX)"
  if [[ -f "$ENV_FILE" ]]; then
    awk -F= '$1 != "ARG_BIND" && $1 != "ARG_DB" { print }' "$ENV_FILE" > "$temp"
  fi
  printf 'ARG_BIND=%s\nARG_DB=%s\n' "$bind" "$db_path" >> "$temp"
  chown root:"$SERVICE_GROUP" "$temp"
  chmod 0640 "$temp"
  mv -f "$temp" "$ENV_FILE"
}

ensure_public_bind() {
  local current port
  current="$(awk -F= '$1 == "ARG_BIND" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
  [[ -n "$current" ]] || current="$DEFAULT_BIND"
  case "$current" in
    127.0.0.1:*|localhost:*)
      port="${current##*:}"
      write_bind_config "0.0.0.0:$port"
      ;;
  esac
}

install_https_dependencies() {
  say "Installing Let's Encrypt tools"
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends certbot iproute2 ripgrep
  CERTBOT_BIN="$(command -v certbot)"
}

ensure_ip_certbot() {
  local version minimum
  version="$("$CERTBOT_BIN" --version 2>&1 | rg -o '[0-9]+(\.[0-9]+){1,2}' | head -n 1 || true)"
  [[ -n "$version" ]] || version="0.0.0"
  minimum="$(printf '%s\n' 5.4.0 "$version" | sort -V | head -n 1)"
  [[ "$minimum" == "5.4.0" ]] && return 0

  say "Installing Certbot 5.4 or newer for IP address certificates"
  apt-get install -y --no-install-recommends snapd || { warn "Could not install snapd; HTTPS is optional and can be configured later."; return 1; }
  systemctl enable --now snapd.socket
  snap install certbot --classic || { warn "Could not install the IP-certificate Certbot from Snap; HTTPS is optional."; return 1; }
  [[ -x /snap/bin/certbot ]] || { warn "The Snap Certbot executable is unavailable; HTTPS is optional."; return 1; }
  CERTBOT_BIN="/snap/bin/certbot"
  version="$("$CERTBOT_BIN" --version 2>&1 | rg -o '[0-9]+(\.[0-9]+){1,2}' | head -n 1 || true)"
  [[ -n "$version" ]] || version="0.0.0"
  minimum="$(printf '%s\n' 5.4.0 "$version" | sort -V | head -n 1)"
  [[ "$minimum" == "5.4.0" ]] || { warn "Certbot 5.4 or newer is required for IP certificates; HTTPS is optional."; return 1; }
}

ensure_certbot_renewal() {
  # Use the exact Certbot binary that issued the certificate. This matters for
  # IP certificates when the distro package is older than the Snap release.
  systemctl disable --now certbot.timer snap.certbot.renew.timer 2>/dev/null || true
  cat > "$CERTBOT_TIMER_SERVICE" <<UNIT
[Unit]
Description=Renew ARG TLS certificate

[Service]
Type=oneshot
ExecStart=$CERTBOT_BIN renew --quiet
UNIT
  cat > "$CERTBOT_TIMER_UNIT" <<'UNIT'
[Unit]
Description=Twice-daily renewal check for ARG certificates

[Timer]
OnCalendar=*-*-* 00,12:00:00
RandomizedDelaySec=1h
Persistent=true

[Install]
WantedBy=timers.target
UNIT
  chmod 0644 "$CERTBOT_TIMER_SERVICE" "$CERTBOT_TIMER_UNIT"
  systemctl daemon-reload
  systemctl enable --now arg-certbot-renew.timer
}

restore_port80_service() {
  [[ -n "$PORT80_STOPPED_UNIT" ]] || return 0
  if systemctl start "$PORT80_STOPPED_UNIT"; then
    PORT80_STOPPED_UNIT=""
    return 0
  else
    warn "$PORT80_STOPPED_UNIT could not be restarted; start it manually."
    return 1
  fi
}

port80_systemd_service() {
  has ss || return 1
  local listeners pid unit found=""
  listeners="$(ss -H -ltnp 'sport = :80' 2>/dev/null)" || return 1
  [[ -n "$listeners" ]] || return 0
  while IFS= read -r pid; do
    [[ -n "$pid" && -r "/proc/$pid/cgroup" ]] || return 1
    unit="$(sed -nE 's#.*\/([^/]+\.service)$#\1#p' "/proc/$pid/cgroup" | head -n 1)"
    [[ "$unit" =~ ^[A-Za-z0-9_.@:-]+\.service$ ]] || return 1
    systemctl is-active --quiet "$unit" || return 1
    case "$unit" in
      docker.service|containerd.service|podman.service|snap.docker.dockerd.service) return 1 ;;
    esac
    if [[ -n "$found" && "$found" != "$unit" ]]; then
      return 1
    fi
    found="$unit"
  done < <(sed -nE 's/.*pid=([0-9]+).*/\1/p' <<< "$listeners" | sort -u)
  [[ -n "$found" ]] || return 1
  printf '%s\n' "$found"
}

available_tls_port() {
  has ss || return 1
  local port
  for port in 443 8443 9443 10443; do
    if ! ss -H -ltn "sport = :$port" 2>/dev/null | rg -q .; then
      printf '%s\n' "$port"
      return 0
    fi
  done
  return 1
}

set_env_value() {
  local name="$1" value="$2" temp
  temp="$(mktemp /etc/arg/.arg.env.XXXXXX)"
  if [[ -f "$ENV_FILE" ]]; then
    awk -F= -v name="$name" '$1 != name { print }' "$ENV_FILE" > "$temp"
  fi
  printf '%s=%s\n' "$name" "$value" >> "$temp"
  chown root:"$SERVICE_GROUP" "$temp"
  chmod 0640 "$temp"
  mv -f "$temp" "$ENV_FILE"
}

ensure_setup_token() {
  local current token
  current="$(awk -F= '$1 == "ARG_SETUP_TOKEN" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
  [[ -n "$current" ]] && return 0
  token="$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')"
  [[ "$token" =~ ^[0-9a-f]{64}$ ]] || die "Could not generate the one-time setup code."
  set_env_value ARG_SETUP_TOKEN "$token"
}

show_setup_code_if_needed() {
  local state code
  state="$(curl --connect-timeout 1 --max-time 2 -fsS "http://127.0.0.1:$(panel_port)/api/state" 2>/dev/null || true)"
  [[ "$state" == *'"installed":false'* ]] || return 0
  code="$(awk -F= '$1 == "ARG_SETUP_TOKEN" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
  if [[ -z "$code" ]] && has journalctl; then
    code="$(journalctl -u "$SERVICE_NAME" -n 100 --no-pager 2>/dev/null | sed -n 's/.*ARG_SETUP_CODE=//p' | tail -n 1)"
  fi
  [[ -n "$code" ]] || return 0
  say "FIRST ADMINISTRATOR SETUP"
  info "One-time setup code: $code"
  info "Use this code on the HTTPS panel setup screen. Keep it private."
}

get_setup_code() {
  require_root setup-code
  [[ -f "$SERVICE_FILE" ]] || die "ARG is not installed. Install it before requesting the setup code."
  local state code
  state="$(curl --connect-timeout 1 --max-time 2 -fsS "http://127.0.0.1:$(panel_port)/api/state" 2>/dev/null || true)"
  [[ -n "$state" ]] || die "The panel is not responding. Check the service with 'sudo arg status'."
  if [[ "$state" != *'"installed":false'* ]]; then
    say "ADMINISTRATOR SETUP IS COMPLETE"
    info "A one-time setup code is no longer needed."
    return 0
  fi
  code="$(awk -F= '$1 == "ARG_SETUP_TOKEN" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
  if [[ -z "$code" ]] && has journalctl; then
    code="$(journalctl -u "$SERVICE_NAME" -n 100 --no-pager 2>/dev/null | sed -n 's/.*ARG_SETUP_CODE=//p' | tail -n 1)"
  fi
  [[ -n "$code" ]] || die "The setup code is unavailable. Run 'sudo arg update' and request it again."
  say "FIRST ADMINISTRATOR SETUP"
  info "One-time setup code: $code"
  info "Enter this code on the HTTPS panel setup screen. Keep it private."
}

change_account_password() {
  require_root password
  require_systemd
  [[ -f "$SERVICE_FILE" && -x "$BINARY" ]] || die "ARG is not installed."
  local username new_password db_path
  read -r -p 'Account username: ' username </dev/tty
  [[ -n "$username" ]] || die "Username cannot be empty."
  IFS= read -r -s -p 'New password (at least 12 characters): ' new_password </dev/tty
  printf '\n'
  db_path="$(configured_db_path)"
  if printf '%s\n' "$username" "$new_password" | ARG_DB="$db_path" "$BINARY" change-password; then
    unset new_password
  else
    unset new_password
    die "Password reset failed. Verify the username and use at least 12 characters."
  fi
}

sync_ufw_rules() {
  has ufw || return 0
  ufw status 2>/dev/null | rg -q '^Status: active' || return 0
  local port tls_bind tls_port tls_cert tls_key rule_id
  port="$(panel_port)"
  tls_bind="$(awk -F= '$1 == "ARG_TLS_BIND" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
  tls_cert="$(awk -F= '$1 == "ARG_TLS_CERT" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
  tls_key="$(awk -F= '$1 == "ARG_TLS_KEY" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
  tls_port="${tls_bind##*:}"
  while IFS= read -r rule_id; do
    [[ -n "$rule_id" ]] || continue
    ufw --force delete "$rule_id" >/dev/null 2>&1 || return 1
  done < <(ufw status numbered 2>/dev/null | awk '/# ARG panel|# ARG HTTPS panel|# ARG HTTPS challenge|# DONATELLO panel|# DONATELLO HTTPS panel|# DONATELLO HTTPS challenge/ {if(match($0,/^[[:space:]]*\[[[:space:]]*[0-9]+\]/)){id=substr($0,RSTART,RLENGTH); gsub(/[^0-9]/,"",id); print id}}' | sort -rn)
  if [[ -s "$DOMAIN_FILE" && -s "$tls_cert" && -s "$tls_key" && -n "$tls_port" ]]; then
    # HTTPS is the public entry point; leave the HTTP backend port closed in UFW.
    ufw insert 1 allow 80/tcp comment 'ARG HTTPS challenge' >/dev/null || return 1
    ufw insert 1 allow "$tls_port/tcp" comment 'ARG HTTPS panel' >/dev/null || return 1
  else
    ufw insert 1 allow "$port/tcp" comment 'ARG panel' >/dev/null || return 1
  fi
}

write_cert_deploy_hook() {
  install -d -o root -g root -m 0755 "$(dirname "$CERT_DEPLOY_HOOK")"
  cat > "$CERT_DEPLOY_HOOK" <<'HOOK'
#!/bin/sh
set -eu
domain_file=/etc/arg/https-domain
[ -s "$domain_file" ] || exit 0
domain=$(cat "$domain_file")
case " ${RENEWED_DOMAINS:-} " in
  *" $domain "*) ;;
  *) exit 0 ;;
esac
install -o root -g arg -m 0640 "$RENEWED_LINEAGE/fullchain.pem" /etc/arg/tls/fullchain.pem
install -o root -g arg -m 0640 "$RENEWED_LINEAGE/privkey.pem" /etc/arg/tls/privkey.pem
systemctl restart arg.service
HOOK
  chmod 0755 "$CERT_DEPLOY_HOOK"
  chown root:root "$CERT_DEPLOY_HOOK"
}

copy_certificate_files() {
  local cert_dir="$1"
  install -d -o root -g "$SERVICE_GROUP" -m 0750 "$TLS_DIR"
  install -o root -g "$SERVICE_GROUP" -m 0640 "$cert_dir/fullchain.pem" "$TLS_CERT"
  install -o root -g "$SERVICE_GROUP" -m 0640 "$cert_dir/privkey.pem" "$TLS_KEY"
}

issue_certificate() {
  local host="$1" email="$2" port80_unit="$3" is_ip="$4" stopped="n"
  local server="${5:-}" config_file="${6:-}"
  local attempt output_file retryable provider_label="Let's Encrypt"
  local -a hooks=() identifier_args=() provider_args=()
  write_cert_deploy_hook
  if [[ "$is_ip" == "y" ]]; then
    identifier_args=(--ip-address "$host" --preferred-profile shortlived)
  else
    identifier_args=(-d "$host")
  fi
  if [[ -n "$port80_unit" ]]; then
    info "Temporarily stopping $port80_unit to release TCP port 80."
    if ! systemctl stop "$port80_unit"; then
      warn "Could not safely stop $port80_unit; no certificate request was made. HTTPS can be configured later."
      return 1
    fi
    PORT80_STOPPED_UNIT="$port80_unit"
    trap 'restore_port80_service' EXIT INT TERM
    stopped="y"
    hooks+=(--pre-hook "systemctl stop $port80_unit" --post-hook "systemctl start $port80_unit")
  fi
  [[ -z "$server" ]] || provider_args+=(--server "$server")
  [[ -z "$config_file" ]] || provider_args+=(--config "$config_file")
  [[ -z "$server" ]] || provider_label="the alternate ACME provider"

  output_file="$(mktemp /tmp/arg-certbot.XXXXXX)"
  for attempt in 1 2 3; do
    if "$CERTBOT_BIN" certonly --standalone --preferred-challenges http --non-interactive \
      --agree-tos --email "$email" --cert-name "$CERT_NAME" "${identifier_args[@]}" \
      --deploy-hook "$CERT_DEPLOY_HOOK" "${provider_args[@]}" "${hooks[@]}" >"$output_file" 2>&1; then
      cat "$output_file"
      rm -f "$output_file"
      if [[ "$stopped" == "y" ]] && ! restore_port80_service; then
        warn "The certificate was issued, but $port80_unit could not be restarted. Start it manually; HTTPS activation was skipped."
        trap - EXIT INT TERM
        return 1
      fi
      trap - EXIT INT TERM
      return 0
    fi
    cat "$output_file"
    retryable="$(rg -qi 'ReadTimeout|ConnectTimeout|timed out|ConnectionError|Temporary failure in name resolution|Name or service not known|Network is unreachable' "$output_file" && printf y || printf n)"
    if [[ "$retryable" != "y" || "$attempt" == 3 ]]; then
      break
    fi
    warn "The ACME service could not be reached (attempt $attempt/3). Retrying in $((attempt * 5)) seconds."
    sleep "$((attempt * 5))"
  done
  rm -f "$output_file"
  if [[ "$stopped" == "y" ]] && ! restore_port80_service; then
    warn "Certificate issuance failed and $port80_unit could not be restarted. Start it manually; HTTPS activation was skipped."
    trap - EXIT INT TERM
    return 1
  fi
  trap - EXIT INT TERM
  warn "Certificate issuance could not reach $provider_label after up to 3 attempts. Check outbound TCP 443, DNS resolution, server time, and provider egress filtering; then retry with 'sudo arg ssl'."
  return 1
}

try_zerossl_fallback() {
  local host="$1" email="$2" port80_unit="$3" is_ip="$4"
  local answer="" eab_kid="" eab_hmac="" config_file=""
  if [[ "$is_ip" == "y" ]]; then
    warn "ZeroSSL fallback requires a DNS name; it does not issue certificates for bare IP addresses. Use a domain, or install an IP certificate from a CA that supports IP certificates."
    return 1
  fi
  say "ALTERNATE CERTIFICATE AUTHORITY · ZEROSSL"
  info "ZeroSSL uses a separate ACME endpoint. It requires External Account Binding (EAB) credentials from your ZeroSSL account."
  info "Create or retrieve EAB credentials at https://app.zerossl.com/developer"
  read_answer 'Use ZeroSSL as the second certificate source? [Y/n]: ' answer 'y'
  [[ "${answer,,}" == "y" || "${answer,,}" == "yes" ]] || return 1
  IFS= read -r -p 'ZeroSSL EAB KID: ' eab_kid </dev/tty || true
  IFS= read -r -s -p 'ZeroSSL EAB HMAC key: ' eab_hmac </dev/tty || true
  printf '\n'
  [[ "$eab_kid" =~ ^[A-Za-z0-9._-]{8,256}$ && "$eab_hmac" =~ ^[A-Za-z0-9_+/=-]{16,512}$ ]] || {
    unset eab_kid eab_hmac
    warn "The ZeroSSL EAB credentials are missing or malformed; no alternate request was made."
    return 1
  }
  config_file="$(mktemp /run/arg-certbot-zerossl.XXXXXX)"
  chmod 0600 "$config_file"
  printf 'eab-kid = %s\neab-hmac-key = %s\n' "$eab_kid" "$eab_hmac" > "$config_file"
  unset eab_kid eab_hmac
  info "Requesting a domain certificate through ZeroSSL. EAB credentials are kept in a temporary root-only config and removed afterward."
  if issue_certificate "$host" "$email" "$port80_unit" "$is_ip" \
    "https://acme.zerossl.com/v2/DV90" "$config_file"; then
    rm -f "$config_file"
    return 0
  fi
  rm -f "$config_file"
  warn "ZeroSSL could not issue the certificate either. Check outbound HTTPS access to acme.zerossl.com, DNS, inbound TCP 80, and the EAB credentials. You can install a certificate manually in Panel Settings → SSL Certificates."
  return 1
}

configure_https() {
  require_root ssl
  require_systemd
  [[ -f "$SERVICE_FILE" ]] || die "ARG is not installed. Install it before configuring HTTPS."

  local address confirmed host email dns_matches port existing_domain port80_unit env_backup is_ip
  address="$(detect_public_ipv4 || true)"
  if [[ -n "$address" ]]; then
    info "Detected public IPv4 address: $address"
    read_answer "Is this the public IP address of this server? [Y/n]: " confirmed "y"
    if [[ "${confirmed,,}" == "n" || "${confirmed,,}" == "no" ]]; then
      SERVER_PUBLIC_IP=""
      warn "Address confirmation declined. HTTPS setup was skipped."
      return 0
    fi
    SERVER_PUBLIC_IP="$address"
  else
    warn "A public IPv4 address could not be detected."
  fi

  existing_domain="$(cat "$DOMAIN_FILE" 2>/dev/null || true)"
  if [[ -n "$existing_domain" ]]; then
    say "HTTPS is already configured for $existing_domain"
    port="$(awk -F= '$1 == "ARG_TLS_BIND" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
    port="${port##*:}"
    if [[ -n "$port" && "$port" != "443" ]]; then
      info "Panel URL: https://$existing_domain:$port"
    else
      info "Panel URL: https://$existing_domain"
    fi
    info "Certificate renewal is managed by Certbot."
    return 0
  fi

  info "HTTPS is optional. Type 'skip' at the address prompt, or decline the ZeroSSL fallback if certificate issuance is unavailable."
  read_answer "Domain name or server IP for HTTPS [$address] (type skip to skip): " host "$address"
  [[ "${host,,}" != "skip" && -n "$host" ]] || {
    info "HTTPS setup skipped. The panel remains private on localhost."
    return 0
  }
  if [[ "$host" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]]; then
    is_ip="y"
    if [[ -n "$address" && "$host" != "$address" ]]; then
      warn "The requested IP does not match the detected public server IP ($address)."
      read_answer 'Continue with this IP address? [y/N]: ' confirmed 'n'
      [[ "${confirmed,,}" == "y" || "${confirmed,,}" == "yes" ]] || return 0
    fi
  else
    is_ip="n"
    if [[ ! "$host" =~ ^([A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)+[A-Za-z]{2,63}$ ]]; then
      warn "The domain name is invalid. Installation will continue without HTTPS; run 'sudo arg ssl' later."
      return 0
    fi
    dns_matches="$(getent ahostsv4 "$host" | awk '{print $1}' | sort -u || true)"
    if [[ -z "$dns_matches" || ( -n "$address" && " $dns_matches " != *" $address "* ) ]]; then
      warn "The domain does not currently resolve to the detected server IP."
      info "Domain addresses: ${dns_matches:-none}"
      [[ -z "$address" ]] || info "Detected server IP: $address"
      read_answer 'Continue and let certificate validation decide? [y/N]: ' confirmed 'n'
      [[ "${confirmed,,}" == "y" || "${confirmed,,}" == "yes" ]] || return 0
    fi
  fi

  read_answer 'Email address for certificate expiry notices: ' email
  if [[ ! "$email" == *@*.* ]]; then
    warn "The certificate email is invalid. Installation will continue without HTTPS; run 'sudo arg ssl' later."
    return 0
  fi
  if ! install_https_dependencies; then
    warn "Could not prepare certificate tools. Installation will continue without HTTPS; run 'sudo arg ssl' later."
    return 0
  fi
  if [[ "$is_ip" == "y" ]] && ! ensure_ip_certbot; then
    warn "The IP-certificate tools are unavailable. Installation will continue without HTTPS; run 'sudo arg ssl' later."
    return 0
  fi
  port80_unit="$(port80_systemd_service)" || {
    warn "TCP port 80 is occupied by a service ARG cannot safely pause. Installation will continue without HTTPS; configure HTTPS later after resolving the conflict."
    return 0
  }
  if ! issue_certificate "$host" "$email" "$port80_unit" "$is_ip"; then
    if [[ -n "$PORT80_STOPPED_UNIT" ]]; then
      warn "A web service could not be restored after the certificate attempt. ZeroSSL fallback was skipped; restore the service and configure HTTPS later."
      rm -f "$CERT_DEPLOY_HOOK"
      return 0
    fi
    if ! try_zerossl_fallback "$host" "$email" "$port80_unit" "$is_ip"; then
      rm -f "$CERT_DEPLOY_HOOK"
      warn "Certificate issuance failed. The panel remains available on its current address; HTTPS configuration was not changed. Open Panel Settings → SSL Certificates after installing a certificate manually."
      return 0
    fi
  fi

  port="$(available_tls_port)" || {
    warn "No HTTPS port is available (tried 443, 8443, 9443, and 10443). The certificate remains on disk; ARG installation continues without HTTPS. Configure it later in Panel Settings → SSL Certificates."
    return 0
  }
  env_backup="$(mktemp /etc/arg/.arg.env.backup.XXXXXX)"
  if ! cp -p "$ENV_FILE" "$env_backup" || ! copy_certificate_files "/etc/letsencrypt/live/$CERT_NAME"; then
    rm -f "$env_backup"
    warn "The certificate was issued but could not be staged. Installation will continue without HTTPS; configure it later in Panel Settings."
    return 0
  fi
  if ! printf '%s\n' "$host" > "$DOMAIN_FILE" || ! chown root:"$SERVICE_GROUP" "$DOMAIN_FILE" || ! chmod 0640 "$DOMAIN_FILE" \
    || ! set_env_value ARG_TLS_CERT "$TLS_CERT" || ! set_env_value ARG_TLS_KEY "$TLS_KEY" || ! set_env_value ARG_TLS_BIND "0.0.0.0:$port"; then
    mv -f "$env_backup" "$ENV_FILE"
    rm -f "$DOMAIN_FILE" "$TLS_CERT" "$TLS_KEY" "$CERT_DEPLOY_HOOK"
    warn "The certificate was issued but could not be activated. Installation will continue without HTTPS."
    return 0
  fi
  sync_ufw_rules || warn "Could not update UFW rules automatically. Allow inbound TCP 80 and $port before using HTTPS."
  ensure_certbot_renewal || warn "Automatic renewal scheduling failed. The certificate works now, but renew it manually or repair the Certbot timer."
  if ! systemctl restart "$SERVICE_NAME" || ! wait_until_active || ! wait_for_https "$host" "$port"; then
    journalctl -u "$SERVICE_NAME" -n 30 --no-pager || true
    mv -f "$env_backup" "$ENV_FILE"
    rm -f "$DOMAIN_FILE" "$TLS_CERT" "$TLS_KEY" "$CERT_DEPLOY_HOOK"
    "$CERTBOT_BIN" delete --cert-name "$CERT_NAME" --non-interactive >/dev/null 2>&1 || true
    systemctl restart "$SERVICE_NAME" || true
    warn "The certificate was issued, but HTTPS did not start. The previous local panel settings were restored."
    return 0
  fi
  rm -f "$env_backup"
  say "HTTPS is active"
  if [[ "$port" == "443" ]]; then
    info "Panel URL: https://$host"
  else
    info "Panel URL: https://$host:$port"
    warn "Port 443 is occupied. Allow inbound TCP port $port in your firewalls."
  fi
  info "Automatic renewal: twice-daily Certbot check"
  [[ "$is_ip" != "y" ]] || info "IP address certificates are short-lived and renew automatically."
  warn "Make sure inbound TCP port 80 is allowed by your provider firewall for certificate renewal."
  info "For public first-admin setup, enter the one-time code printed by the installer on the HTTPS setup screen."
}

wait_for_https() {
  local domain="$1" port="$2" attempt
  for attempt in {1..20}; do
    if curl --connect-timeout 1 --max-time 2 -fsS --resolve "$domain:$port:127.0.0.1" \
      "https://$domain:$port/api/state" -o /dev/null 2>/dev/null; then
      return 0
    fi
    sleep 1
  done
  return 1
}

show_access_summary() {
  local address domain port tls_bind tls_port panel_url
  address="${SERVER_PUBLIC_IP:-$(detect_public_ipv4 || true)}"
  domain="$(cat "$DOMAIN_FILE" 2>/dev/null || true)"
  port="$(panel_port)"
  say "ARG IS READY"
  if [[ -n "$address" ]]; then
    info "Server IP: $address"
  else
    info "Public IP was not confirmed. Use your server IP in the SSH command below."
  fi
  if [[ -n "$domain" ]]; then
    tls_bind="$(awk -F= '$1 == "ARG_TLS_BIND" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
    tls_port="${tls_bind##*:}"
    [[ -n "$tls_port" && "$tls_port" != "443" ]] && domain="$domain:$tls_port"
    [[ -n "$tls_port" ]] || domain="$domain:443"
    panel_url="https://$domain"
    printf '\n  %sPANEL LOGIN%s\n  ' "$C_CYAN" "$C_RESET"
    printf '\033]8;;%s\033\\%s%s%s\033]8;;\033\\\n\n' "$panel_url" "$C_BOLD" "$panel_url" "$C_RESET"
    info "Create the first administrator on this HTTPS page with the one-time code printed below."
    if [[ -n "$address" ]]; then
      printf '  %sssh -N -L %s:127.0.0.1:%s <SSH_USER>@%s%s\n' "$C_GREEN" "$port" "$port" "$address" "$C_RESET"
    else
      printf '  %sssh -N -L %s:127.0.0.1:%s <SSH_USER>@<SERVER_IP>%s\n' "$C_GREEN" "$port" "$port" "$C_RESET"
    fi
    info "The SSH tunnel is an alternative if you prefer to create the account locally."
  else
    panel_url="http://${address:-<SERVER_IP>}:$(panel_port)"
    printf '\n  %sPUBLIC PANEL%s\n  ' "$C_CYAN" "$C_RESET"
    printf '\033]8;;%s\033\\%s%s%s\033]8;;\033\\\n\n' "$panel_url" "$C_BOLD" "$panel_url" "$C_RESET"
    info "Create the first administrator locally through SSH, or enable HTTPS before using the one-time setup code."
    if [[ -n "$address" ]]; then
      printf '  From your computer, run:\n  %sssh -N -L %s:127.0.0.1:%s <SSH_USER>@%s%s\n' \
        "$C_GREEN" "$port" "$port" "$address" "$C_RESET"
    else
      printf '  From your computer, run:\n  %sssh -N -L %s:127.0.0.1:%s <SSH_USER>@<SERVER_IP>%s\n' \
        "$C_GREEN" "$port" "$port" "$C_RESET"
    fi
    info "Replace <SSH_USER> with your server login. For secure public sign-in, enable HTTPS: sudo arg ssl"
  fi
  printf '\n  %sMANAGEMENT MENU%s\n  Run: %sarg%s\n' "$C_CYAN" "$C_RESET" "$C_BOLD" "$C_RESET"
  info "From a non-root account, use: sudo arg"
}

write_service_unit() {
  cat > "$SERVICE_FILE" <<'UNIT'
[Unit]
Description=ARG VPS Management Panel
Documentation=https://github.com/themrcortex/arg
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=arg
Group=arg
WorkingDirectory=/var/lib/arg
EnvironmentFile=/etc/arg/arg.env
ExecStart=/usr/local/lib/arg/arg
Restart=on-failure
RestartSec=3
UMask=0027
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
ReadWritePaths=/var/lib/arg
RestrictSUIDSGID=true
LockPersonality=true
AmbientCapabilities=CAP_NET_BIND_SERVICE CAP_SETUID CAP_SETGID
CapabilityBoundingSet=CAP_NET_BIND_SERVICE CAP_SETUID CAP_SETGID
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
UNIT
  chmod 0644 "$SERVICE_FILE"
  systemctl daemon-reload
}

write_privileged_service_unit() {
  cat > "$PRIVILEGED_SERVICE_FILE" <<'UNIT'
[Unit]
Description=ARG restricted privileged operations
After=local-fs.target

[Service]
Type=simple
User=root
Group=arg
RuntimeDirectory=arg-privileged
RuntimeDirectoryMode=0750
UMask=0007
ExecStart=/usr/local/lib/arg/arg privileged-helper
Restart=on-failure
RestartSec=2
PrivateTmp=true
LockPersonality=true
LimitNOFILE=1024

[Install]
WantedBy=multi-user.target
UNIT
  chmod 0644 "$PRIVILEGED_SERVICE_FILE"
  systemctl daemon-reload
}

activate_binary() {
  local built="$SOURCE_DIR/target/release/arg"
  local staged
  install -d -o root -g root -m 0755 "$(dirname "$BINARY")"
  staged="$(mktemp /usr/local/lib/arg/.arg.XXXXXX)"
  install -o root -g root -m 0755 "$built" "$staged"
  mv -f "$staged" "$BINARY"
}

installer_script_url() {
  local branch="${1:-$BRANCH}"
  [[ "$branch" == main || "$branch" == development ]] || return 2
  printf 'https://raw.githubusercontent.com/themrcortex/arg/%s/install.sh' "$branch"
}

install_cli() {
  local staged installer_url
  installer_url="$(installer_script_url "$BRANCH")" || die "Cannot install the CLI for unsupported branch '$BRANCH'."
  staged="$(mktemp /usr/local/bin/.arg-cli.XXXXXX)"
  curl --proto '=https' --tlsv1.2 -fsSL "$installer_url" -o "$staged"
  chmod 0755 "$staged"
  chown root:root "$staged"
  mv -f "$staged" "$CLI"
  rm -f "$LEGACY_INSTALLER"
}

install_app() {
  require_root install
  require_systemd
  migrate_legacy_install
  install_os_dependencies
  ensure_rust
  ensure_firewall_dependency
  prepare_source
  build_release
  ensure_service_account
  configure_ufw_status
  write_default_config
  ensure_setup_token
  ensure_public_bind
  sync_ufw_rules
  take_database_ownership
  activate_binary
  write_privileged_service_unit
  systemctl enable --now arg-privileged.service
  write_service_unit
  install_cli
  systemctl enable "$SERVICE_NAME"
  systemctl restart "$SERVICE_NAME"
  if ! systemctl is-active --quiet "$SERVICE_NAME"; then
    systemctl --no-pager --full status "$SERVICE_NAME" || true
    journalctl -u "$SERVICE_NAME" -n 50 --no-pager || true
    die "The service did not start. Review the service status and logs above."
  fi
  if ! wait_for_panel; then
    journalctl -u "$SERVICE_NAME" -n 50 --no-pager || true
    die "The service is running but the web panel did not answer on 127.0.0.1:$(panel_port)."
  fi
  cleanup_legacy_identity
  say "ARG is installed and running"
  info "Service status: sudo arg status"
  configure_https
  show_access_summary
  show_setup_code_if_needed
}

wait_until_active() {
  local attempt
  for attempt in {1..15}; do
    systemctl is-active --quiet "$SERVICE_NAME" && return 0
    sleep 1
  done
  return 1
}

wait_for_panel() {
  local attempt port
  port="$(panel_port)"
  for attempt in {1..20}; do
    if curl --connect-timeout 1 --max-time 2 -fsS "http://127.0.0.1:$port/api/state" -o /dev/null; then
      return 0
    fi
    sleep 1
  done
  return 1
}

select_update_options() {
  local requested_branch="${1:-}" requested_mode="${2:-}" answer default_mode
  if [[ -z "$requested_branch" ]]; then
    if [[ -r /dev/tty && -t 1 && "${ARG_UPDATE_RESERVED:-0}" != "1" ]]; then
      info "Choose the source branch. main is stable; development contains prerelease changes."
      read_answer "Update branch [main/development] [$BRANCH]: " answer "$BRANCH"
      requested_branch="$answer"
    else
      requested_branch="$BRANCH"
    fi
  fi
  [[ "$requested_branch" == main || "$requested_branch" == development ]] || die "Invalid update branch. Choose main or development."

  default_mode="${ARG_UPDATE_MODE:-complete}"
  [[ "$default_mode" == fast || "$default_mode" == complete ]] || default_mode="complete"
  if [[ -z "$requested_mode" ]]; then
    if [[ -r /dev/tty && -t 1 && "${ARG_UPDATE_RESERVED:-0}" != "1" ]]; then
      info "Fast keeps Rust build cache on disk for quicker future updates. Complete clears it before and after compilation; builds take longer."
      local default_choice=2
      [[ "$default_mode" == fast ]] && default_choice=1
      read_answer "Build mode [1=fast / 2=complete] [$default_choice]: " answer "$default_choice"
      case "${answer,,}" in
        1|fast) requested_mode="fast" ;;
        2|complete) requested_mode="complete" ;;
        *) die "Invalid build mode. Choose 1/fast or 2/complete." ;;
      esac
    else
      requested_mode="$default_mode"
    fi
  fi
  [[ "$requested_mode" == fast || "$requested_mode" == complete ]] || die "Invalid build mode. Choose fast or complete."

  BRANCH="$requested_branch"
  ARG_UPDATE_MODE="$requested_mode"
  export ARG_UPDATE_MODE ARG_UPDATE_BRANCH="$BRANCH" ARG_UPDATE_OPERATION=update ARG_UPDATE_CONTEXT=update
  check_update_status
  info "Update source: $BRANCH ($UPDATE_STATUS) · build mode: $ARG_UPDATE_MODE"
}

cleanup_update_build_cache() {
  local exit_code="${1:-0}"
  (( UPDATE_CLEANUP_BUILD_STARTED )) || return 0
  [[ "$ARG_UPDATE_MODE" == "complete" || "$exit_code" != 0 ]] || return 0
  if bash -c 'cd "$1" && cargo +stable clean' _ "$SOURCE_DIR" >/dev/null 2>&1; then
    info "Removed the Rust build cache to reclaim disk space."
  elif (( exit_code == 0 )); then
    warn "The update succeeded, but the Rust build cache could not be removed. Run 'cargo clean' in $SOURCE_DIR later."
  else
    warn "The update failed and the Rust build cache could not be removed. Free disk space manually under $SOURCE_DIR/target before retrying."
  fi
}
cleanup_update_reservation() {
  local exit_code="${1:-$?}"
  if (( exit_code != 0 )); then
    if [[ -e "$UPDATE_CANCEL_FILE" ]]; then
      local cancellation_recovered=1
      if (( UPDATE_CLEANUP_BINARY_SWAPPED )) && [[ -x "$PREVIOUS_BINARY" ]] && ! cmp -s "$BINARY" "$PREVIOUS_BINARY"; then
        warn "Cancellation arrived during activation. Restoring the previous release before finishing cancellation."
        systemctl stop "$SERVICE_NAME" 2>/dev/null || true
        restore_previous_database || { warn "The previous database snapshot could not be restored."; cancellation_recovered=0; }
        if [[ -x "$PREVIOUS_BINARY" ]]; then
          install -o root -g root -m 0755 "$PREVIOUS_BINARY" "$BINARY" || cancellation_recovered=0
          systemctl start "$SERVICE_NAME" 2>/dev/null || cancellation_recovered=0
          wait_for_panel || cancellation_recovered=0
          UPDATE_CLEANUP_BINARY_SWAPPED=0
        else
          cancellation_recovered=0
        fi
      fi
      if (( cancellation_recovered )); then
        "$UPDATE_HELPER" state cancelled cancelled "The update was cancelled; the previous release was restored and checked." >/dev/null 2>&1 || true
      else
        "$UPDATE_HELPER" state failed failed "Cancellation recovery needs attention. Check ARG service logs and the saved rollback files." >/dev/null 2>&1 || true
      fi
    else
      if (( UPDATE_CLEANUP_BINARY_SWAPPED )) && [[ -x "$PREVIOUS_BINARY" ]] && ! cmp -s "$BINARY" "$PREVIOUS_BINARY"; then
        warn "The update stopped unexpectedly after replacing the ARG binary. Restoring the previous release."
        systemctl stop "$SERVICE_NAME" 2>/dev/null || true
        restore_previous_database || warn "The previous database snapshot could not be restored."
        if [[ -x "$PREVIOUS_BINARY" ]]; then
          install -o root -g root -m 0755 "$PREVIOUS_BINARY" "$BINARY" || true
          systemctl start "$SERVICE_NAME" 2>/dev/null || true
          wait_for_panel || warn "The previous binary was restored, but the panel health check still fails. Review service logs."
          UPDATE_CLEANUP_BINARY_SWAPPED=0
        fi
      fi
      "$UPDATE_HELPER" state failed failed "The update stopped before successful verification. Review the update log and retry." >/dev/null 2>&1 || true
    fi
  fi
  cleanup_update_build_cache "$exit_code"
  [[ -z "${UPDATE_CLEANUP_STAGED:-}" ]] || rm -f "$UPDATE_CLEANUP_STAGED"
  [[ -z "${UPDATE_CLEANUP_DATABASE_STAGED:-}" ]] || rm -f "$UPDATE_CLEANUP_DATABASE_STAGED"
  rm -f "$UPDATE_STATUS_FILE"
  rm -f "$UPDATE_CANCEL_FILE"
  rmdir "$UPDATE_LOCK_DIR" 2>/dev/null || true
  return "$exit_code"
}

update_app() {
  require_root update
  require_systemd
  local was_installed=0 previous database_path update_mutex_fd
  UPDATE_CLEANUP_BUILD_STARTED=0
  UPDATE_CLEANUP_BINARY_SWAPPED=0
  UPDATE_CLEANUP_STAGED=""
  UPDATE_CLEANUP_DATABASE_STAGED=""
  select_update_options "${1:-}" "${2:-}"
  [[ -f "$SERVICE_FILE" || -f "$OLD_SERVICE_FILE" ]] && was_installed=1
  (( was_installed )) || die "ARG is not installed. Run 'arg install' first."
  if [[ "${ARG_UPDATE_RESERVED:-0}" == "1" ]]; then
    [[ -d "$UPDATE_LOCK_DIR" && -r "$UPDATE_STATUS_FILE" ]] || die "The scheduled update reservation is missing. Please retry from the panel."
    local reservation_started
    reservation_started="$(awk 'NR==1 {print $1}' "$UPDATE_STATUS_FILE" 2>/dev/null || true)"
    [[ "$reservation_started" =~ ^[0-9]+$ ]] || reservation_started="$(date +%s)"
    printf '%s %s %s\n' "$reservation_started" "$$" "$(awk '{print $22}' "/proc/$$/stat")" > "$UPDATE_STATUS_FILE"
  else
    exec {update_mutex_fd}>"$UPDATE_MUTEX_FILE"
    chmod 0600 "$UPDATE_MUTEX_FILE"
    flock -x "$update_mutex_fd" || die "Could not lock ARG update state."
    if [[ -d "$UPDATE_LOCK_DIR" ]]; then
      local owner owner_start owner_actual unit owner_command
      owner="$(awk 'NR==1 {print $2}' "$UPDATE_STATUS_FILE" 2>/dev/null || true)"
      owner_start="$(awk 'NR==1 {print $3}' "$UPDATE_STATUS_FILE" 2>/dev/null || true)"
      unit="$(cat /run/arg-update-last-unit 2>/dev/null || true)"
      if [[ "$unit" =~ ^arg-web-update-[0-9]+$ ]] && { systemctl is-active --quiet "$unit.service" || systemctl is-active --quiet "$unit.timer"; }; then
        die "Another ARG update is genuinely running. Cancel it with 'sudo arg cancel-update' or wait for it to finish."
      fi
      owner_command=""
      [[ "$owner" =~ ^[0-9]+$ && -r "/proc/$owner/cmdline" ]] && owner_command="$(tr '\0' ' ' <"/proc/$owner/cmdline" 2>/dev/null || true)"
      owner_actual=""
      [[ "$owner" =~ ^[0-9]+$ && -r "/proc/$owner/stat" ]] && owner_actual="$(awk '{print $22}' "/proc/$owner/stat" 2>/dev/null || true)"
      if { [[ "$owner_start" =~ ^[0-9]+$ && "$owner_actual" == "$owner_start" ]] || [[ "$owner_command" == *"/usr/local/bin/arg"* && "$owner_command" == *"update"* ]]; } && kill -0 "$owner" 2>/dev/null; then
        die "Another ARG update is genuinely running. Cancel it with 'sudo arg cancel-update' or wait for it to finish."
      fi
      "$UPDATE_HELPER" state failed failed "A stale update lock was cleared because no updater process or systemd job is active." >/dev/null 2>&1 || true
      rm -f "$UPDATE_STATUS_FILE" "$UPDATE_CANCEL_FILE"
      rmdir "$UPDATE_LOCK_DIR" 2>/dev/null || true
    fi
    mkdir "$UPDATE_LOCK_DIR" 2>/dev/null || die "Another ARG update is already running. Please wait for it to finish."
    printf '%s %s\n' "$(date +%s)" "$$" > "$UPDATE_STATUS_FILE"
    export ARG_UPDATE_JOB_ID="arg-$(cat /proc/sys/kernel/random/uuid 2>/dev/null || date +%s)-$$"
    flock -u "$update_mutex_fd" || true
    exec {update_mutex_fd}>&-
  fi
  chmod 0644 "$UPDATE_STATUS_FILE"
  [[ -n "${ARG_UPDATE_JOB_ID:-}" ]] || export ARG_UPDATE_JOB_ID="arg-$(cat /proc/sys/kernel/random/uuid 2>/dev/null || date +%s)-$$"
  update_phase() {
    [[ ! -e "$UPDATE_CANCEL_FILE" ]] || return 1
    "$UPDATE_HELPER" state running "$1" "${2:-}" >/dev/null 2>&1 || true
    [[ ! -e "$UPDATE_CANCEL_FILE" ]]
  }
  trap cleanup_update_reservation EXIT
  update_phase preparing "Preparing the ARG update."
  migrate_legacy_install
  ensure_setup_token
  ensure_public_bind
  ensure_firewall_dependency
  sync_ufw_rules
  prepare_source
  # Keep the privileged updater helper in sync before the replacement service starts polling it.
  configure_ufw_status
  update_phase building "Building the new ARG release."
  UPDATE_CLEANUP_BUILD_STARTED=1
  build_release
  ensure_service_account
  configure_ufw_status
  take_database_ownership
  install -d -o root -g root -m 0755 "$(dirname "$BINARY")"
  UPDATE_CLEANUP_STAGED="$(mktemp /usr/local/lib/arg/.arg.XXXXXX)"
  install -d -o root -g root -m 0700 "$ROLLBACK_DIR"
  install -o root -g root -m 0755 "$SOURCE_DIR/target/release/arg" "$DB_BACKUP_TOOL"
  database_path="$(configured_db_path)"
  rm -f "$PREVIOUS_DATABASE"
  if [[ -s "$database_path" ]]; then
    UPDATE_CLEANUP_DATABASE_STAGED="$(mktemp "$ROLLBACK_DIR/.arg.db.previous.XXXXXX")"
    rm -f "$UPDATE_CLEANUP_DATABASE_STAGED"
    if ! "$SOURCE_DIR/target/release/arg" database-backup "$database_path" "$UPDATE_CLEANUP_DATABASE_STAGED"; then
      rm -f "$UPDATE_CLEANUP_DATABASE_STAGED"
      die "Could not create a consistent database snapshot. The running ARG binary was left untouched."
    fi
    chown root:root "$UPDATE_CLEANUP_DATABASE_STAGED"
    chmod 0600 "$UPDATE_CLEANUP_DATABASE_STAGED"
    mv -f "$UPDATE_CLEANUP_DATABASE_STAGED" "$PREVIOUS_DATABASE"
  fi
  if [[ -x "$BINARY" ]]; then
    previous="$(mktemp "$ROLLBACK_DIR/.arg.previous.XXXXXX")"
    cp -p "$BINARY" "$previous"
    chmod 0755 "$previous"
    mv -f "$previous" "$PREVIOUS_BINARY"
  fi
  install -o root -g root -m 0755 "$SOURCE_DIR/target/release/arg" "$UPDATE_CLEANUP_STAGED"
  update_phase installing "Installing the verified build."
  # Arm the EXIT recovery before the atomic rename. If a signal arrives between
  # rename and the next shell instruction, cleanup still knows a swap may have happened.
  UPDATE_CLEANUP_BINARY_SWAPPED=1
  mv -f "$UPDATE_CLEANUP_STAGED" "$BINARY"
  write_privileged_service_unit
  systemctl restart arg-privileged.service
  write_service_unit
  install_cli
  update_phase restarting "Restarting the ARG service."
  systemctl restart "$SERVICE_NAME"
  update_phase verifying "Checking that the updated service is active."
  if wait_for_panel; then
    cleanup_legacy_identity
    say "ARG was updated successfully"
    info "Source revision: $(git -C "$SOURCE_DIR" rev-parse --short HEAD)"
    "$UPDATE_HELPER" state succeeded complete "ARG is updated and the service is active." >/dev/null 2>&1 || true
    UPDATE_CLEANUP_BINARY_SWAPPED=0
    show_setup_code_if_needed || true
    cleanup_update_reservation 0
    trap - EXIT
    if (( MENU_ACTIVE )); then
      exec "$CLI" menu
    fi
    return
  fi
  if [[ -x "$PREVIOUS_BINARY" ]]; then
    warn "The new release did not start. Restoring the previous binary."
    systemctl stop "$SERVICE_NAME" 2>/dev/null || true
    restore_previous_database || warn "The previous database snapshot could not be restored; the database was left unchanged."
    install -o root -g root -m 0755 "$PREVIOUS_BINARY" "$BINARY"
    systemctl start "$SERVICE_NAME" || true
    wait_for_panel || warn "The previous binary was restored, but the panel health check still fails. Review service logs."
    UPDATE_CLEANUP_BINARY_SWAPPED=0
  fi
  journalctl -u "$SERVICE_NAME" -n 50 --no-pager || true
  die "The update could not be activated."
}

restore_previous_database() {
  [[ -s "$PREVIOUS_DATABASE" ]] || return 0
  local db_path staged
  db_path="$(configured_db_path)"
  staged="$(mktemp "${db_path}.rollback.XXXXXX")"
  cp -p "$PREVIOUS_DATABASE" "$staged"
  chown "$SERVICE_USER:$SERVICE_GROUP" "$staged"
  chmod 0600 "$staged"
  mv -f "$staged" "$db_path"
  rm -f "$db_path-wal" "$db_path-shm"
}

show_status() {
  require_systemd
  if [[ -f "$SERVICE_FILE" ]]; then
    systemctl --no-pager --full status "$SERVICE_NAME" || true
  else
    info "ARG is not installed."
  fi
}

change_port() {
  require_root manage
  require_systemd
  [[ -f "$ENV_FILE" ]] || die "Configuration was not found. ARG is not installed."
  local port current_port tls_bind tls_port env_backup domain nginx_backup
  port="${1:-}"
  if [[ -z "$port" ]]; then read -r -p "New panel port (1024-65535): " port; fi
  [[ "$port" =~ ^[0-9]{1,5}$ ]] || die "Invalid port. Enter a number from 1024 to 65535."
  port=$((10#$port))
  (( port >= 1024 && port <= 65535 )) || die "Invalid port. Enter a number from 1024 to 65535."
  current_port="$(panel_port)"
  tls_bind="$(awk -F= '$1 == "ARG_TLS_BIND" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
  tls_port="${tls_bind##*:}"
  [[ -z "$tls_bind" || "$port" != "$tls_port" ]] || die "HTTP and HTTPS cannot use the same port. Choose another port."
  if [[ "$port" == "$current_port" ]]; then
    say "HTTP is already using port $port."
    return 0
  fi
  if ss -H -ltn "sport = :$port" 2>/dev/null | rg -q .; then
    die "Port $port is already in use. Choose a free port."
  fi
  env_backup="$(mktemp /etc/arg/.arg.env.http-port-backup.XXXXXX)"
  cp -p "$ENV_FILE" "$env_backup"
  write_bind_config "0.0.0.0:$port"
  if ! sync_ufw_rules || ! systemctl restart "$SERVICE_NAME" || ! wait_until_active || ! wait_for_panel; then
    mv -f "$env_backup" "$ENV_FILE"
    sync_ufw_rules || warn "The old firewall rules could not be restored automatically."
    systemctl restart "$SERVICE_NAME" || true
    wait_until_active || true
    wait_for_panel || true
    rm -f "$env_backup"
    die "The panel did not respond on port $port. The previous port $current_port was restored."
  fi
  domain="$(cat "$DOMAIN_FILE" 2>/dev/null || true)"
  if [[ -n "$domain" && -f "$LEGACY_NGINX_SITE" ]]; then
    nginx_backup="$(mktemp /etc/nginx/.arg-panel-proxy-backup.XXXXXX)"
    if ! cp -p "$LEGACY_NGINX_SITE" "$nginx_backup"; then
      rm -f "$nginx_backup"
      mv -f "$env_backup" "$ENV_FILE"
      sync_ufw_rules || true
      systemctl restart "$SERVICE_NAME" || true
      wait_until_active || true
      wait_for_panel || true
      die "Could not back up the HTTPS proxy configuration. The previous HTTP port $current_port was restored."
    fi
    sed -E -i "s#proxy_pass http://127\\.0\\.0\\.1:[0-9]+;#proxy_pass http://127.0.0.1:$port;#" "$LEGACY_NGINX_SITE"
    if ! nginx -t || ! systemctl reload nginx; then
      mv -f "$nginx_backup" "$LEGACY_NGINX_SITE"
      nginx -t && systemctl reload nginx || true
      mv -f "$env_backup" "$ENV_FILE"
      sync_ufw_rules || true
      systemctl restart "$SERVICE_NAME" || true
      wait_until_active || true
      wait_for_panel || true
      rm -f "$env_backup"
      die "The HTTPS proxy could not be updated. The previous HTTP port $current_port was restored."
    fi
    rm -f "$nginx_backup"
  fi
  rm -f "$env_backup"
  say "The panel is publicly listening on port $port"
}

change_tls_port() {
  require_root manage
  require_systemd
  [[ -f "$ENV_FILE" ]] || die "Configuration was not found. ARG is not installed."
  local port current_bind current_port cert key domain env_backup confirmation_mode pending_file pending_tmp
  port="${1:-}"
  confirmation_mode="${2:-}"
  [[ -z "$confirmation_mode" || "$confirmation_mode" == web-confirm ]] || die "Invalid HTTPS port confirmation mode."
  exec 9>/run/arg-tls-port-change.lock
  flock -n 9 || die "Another ARG port change is already running. Wait for it to finish."
  if [[ -z "$port" ]]; then read -r -p "New panel HTTPS port (1024-65535): " port; fi
  [[ "$port" =~ ^[0-9]{1,5}$ ]] || die "Invalid port. Enter a number from 1024 to 65535."
  port=$((10#$port))
  (( port >= 1024 && port <= 65535 )) || die "Invalid port. Enter a number from 1024 to 65535."

  current_bind="$(awk -F= '$1 == "ARG_TLS_BIND" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
  [[ -n "$current_bind" ]] || die "HTTPS is not active. Configure a certificate before changing its port."
  current_port="${current_bind##*:}"
  cert="$(awk -F= '$1 == "ARG_TLS_CERT" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
  key="$(awk -F= '$1 == "ARG_TLS_KEY" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
  domain="$(cat "$DOMAIN_FILE" 2>/dev/null || true)"
  [[ -s "$cert" && -s "$key" && -n "$domain" ]] || die "The active HTTPS certificate or domain is unavailable. Reapply the certificate in Panel Settings first."
  [[ "$port" != "$(panel_port)" ]] || die "HTTP and HTTPS cannot use the same port. Choose a different port."
  if [[ "$port" == "$current_port" ]]; then
    say "HTTPS is already using port $port."
    return 0
  fi
  if ss -H -ltn "sport = :$port" 2>/dev/null | rg -q .; then
    die "Port $port is already in use. Choose a free port; 443 may be occupied by a website's Nginx listener."
  fi
  pending_file=/run/arg-tls-port-change.pending
  if [[ "$confirmation_mode" == web-confirm && -e "$pending_file" ]]; then
    die "An earlier HTTPS port change is still awaiting confirmation or rollback."
  fi

  # Open the new firewall port first so the panel remains reachable during the switch.
  if has ufw && ufw status 2>/dev/null | rg -q '^Status: active'; then
    ufw insert 1 allow "$port/tcp" comment 'ARG HTTPS panel' >/dev/null \
      || die "Could not open HTTPS port $port in UFW; the current port was left unchanged."
  fi

  env_backup="$(mktemp /etc/arg/.arg.env.tls-port-backup.XXXXXX)"
  if ! cp -p "$ENV_FILE" "$env_backup"; then
    rm -f "$env_backup"
    sync_ufw_rules || true
    die "Could not back up ARG settings; the current HTTPS port was left unchanged."
  fi
  if [[ "$confirmation_mode" == web-confirm ]]; then
    pending_tmp="$(mktemp /run/.arg-tls-port-change.XXXXXX)"
    if ! printf '%s %s\n' "$current_port" "$port" > "$pending_tmp" \
      || ! chown root:"$SERVICE_GROUP" "$pending_tmp" \
      || ! chmod 0640 "$pending_tmp" \
      || ! mv -f "$pending_tmp" "$pending_file"; then
      rm -f "$pending_tmp" "$pending_file" "$env_backup"
      sync_ufw_rules || true
      die "Could not prepare automatic HTTPS port rollback; the current port was left unchanged."
    fi
    if ! systemd-run --quiet --collect --unit=arg-tls-port-rollback --on-active=90s \
      /usr/local/bin/arg manage tls-port-rollback; then
      rm -f "$pending_file" "$env_backup"
      sync_ufw_rules || true
      die "Could not schedule automatic HTTPS rollback; the current port was left unchanged."
    fi
  fi
  if ! set_env_value ARG_TLS_BIND "0.0.0.0:$port"; then
    mv -f "$env_backup" "$ENV_FILE"
    if [[ "$confirmation_mode" == web-confirm ]]; then
      systemctl stop arg-tls-port-rollback.timer >/dev/null 2>&1 || true
      rm -f "$pending_file"
    fi
    sync_ufw_rules || true
    die "Could not save the HTTPS port; the previous configuration was restored."
  fi

  if ! systemctl restart "$SERVICE_NAME" || ! wait_until_active || ! wait_for_https "$domain" "$port"; then
    mv -f "$env_backup" "$ENV_FILE"
    if [[ "$confirmation_mode" == web-confirm ]]; then
      systemctl stop arg-tls-port-rollback.timer >/dev/null 2>&1 || true
      rm -f "$pending_file"
    fi
    sync_ufw_rules || warn "The old firewall rules could not be restored automatically."
    systemctl restart "$SERVICE_NAME" || true
    wait_until_active || true
    wait_for_https "$domain" "$current_port" || true
    rm -f "$env_backup"
    die "HTTPS did not respond on port $port. The previous port $current_port was restored."
  fi

  rm -f "$env_backup"
  sync_ufw_rules || warn "HTTPS is working on port $port, but UFW still needs a rule review."
  say "The panel is reachable over HTTPS on port $port."
  info "New address: https://$domain:$port"
  [[ "$confirmation_mode" != web-confirm ]] || info "Keep the panel open and confirm this new address within 90 seconds. If the browser cannot reach it, ARG restores port $current_port automatically."
}

confirm_tls_port_change() {
  require_root manage
  local port old_port new_port
  port="${1:-}"
  [[ "$port" =~ ^[0-9]{1,5}$ ]] || die "Invalid HTTPS confirmation port."
  port=$((10#$port))
  exec 9>/run/arg-tls-port-change.lock
  flock -x 9 || die "Could not lock the HTTPS port change."
  [[ -s /run/arg-tls-port-change.pending ]] || die "There is no HTTPS port change waiting for confirmation."
  read -r old_port new_port < /run/arg-tls-port-change.pending
  [[ "$new_port" == "$port" ]] || die "This browser is not connected to the pending HTTPS port."
  systemctl stop arg-tls-port-rollback.timer >/dev/null 2>&1 || die "Could not cancel the automatic rollback timer."
  rm -f /run/arg-tls-port-change.pending
  say "HTTPS port $port is confirmed. Automatic rollback is cancelled."
}

rollback_tls_port_change() {
  require_root manage
  require_systemd
  local old_port new_port current_bind current_port domain
  exec 9>/run/arg-tls-port-change.lock
  flock -x 9 || die "Could not lock the HTTPS rollback operation."
  [[ -s /run/arg-tls-port-change.pending ]] || return 0
  read -r old_port new_port < /run/arg-tls-port-change.pending
  current_bind="$(awk -F= '$1 == "ARG_TLS_BIND" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
  current_port="${current_bind##*:}"
  if [[ "$current_port" != "$new_port" ]]; then
    rm -f /run/arg-tls-port-change.pending
    return 0
  fi
  domain="$(cat "$DOMAIN_FILE" 2>/dev/null || true)"
  [[ -n "$domain" ]] || die "HTTPS rollback cannot find the panel domain; keep the pending state for manual recovery."
  set_env_value ARG_TLS_BIND "0.0.0.0:$old_port"
  sync_ufw_rules || die "HTTPS port $new_port is no longer current, but UFW could not be restored to $old_port."
  systemctl restart "$SERVICE_NAME" || die "Could not restart ARG on the previous HTTPS port $old_port."
  wait_until_active || die "ARG did not become active on the previous HTTPS port $old_port."
  wait_for_https "$domain" "$old_port" || die "The previous HTTPS port $old_port did not pass its local health check."
  rm -f /run/arg-tls-port-change.pending
  say "The unconfirmed HTTPS change was rolled back to port $old_port."
}

manage_app() {
  require_root manage
  if [[ "${2:-}" != "" ]]; then
    require_systemd
    case "$2" in
      status) show_status ;;
      start|stop|restart)
        [[ -f "$SERVICE_FILE" ]] || die "ARG is not installed."
        systemctl "$2" "$SERVICE_NAME"
        ;;
      logs) journalctl -u "$SERVICE_NAME" -n "${3:-100}" --no-pager ;;
      port) change_port "${3:-}" ;;
      tls-port) change_tls_port "${3:-}" "${4:-}" ;;
      tls-port-confirm) confirm_tls_port_change "${3:-}" ;;
      tls-port-rollback) rollback_tls_port_change ;;
      ssl) configure_https ;;
      update) update_app "${3:-}" "${4:-}" ;;
      delete) delete_app "${3:-}" ;;
      *) die "Unknown management action: $2" ;;
    esac
    return
  fi
  main_menu
}

run_service_action() {
  local action="$1"
  require_root "$action"
  require_systemd
  [[ -f "$SERVICE_FILE" ]] || die "ARG is not installed."
  systemctl "$action" "$SERVICE_NAME"
}

view_logs() {
  local lines="${1:-100}"
  [[ "$lines" =~ ^[0-9]+$ ]] || die "Log line count must be a positive number."
  require_root logs
  require_systemd
  journalctl -u "$SERVICE_NAME" -n "$lines" --no-pager
}

draw_menu_item() {
  local selected="$1" number="$2" title="$3" hint="$4"
  if [[ "$selected" == "$number" ]]; then
    printf '  %s❯%s %s%-2s%s  %-30s %s%s%s\n' \
      "$C_COPPER" "$C_RESET" "$C_BOLD" "$number" "$C_RESET" "$title" "$C_DIM" "$hint" "$C_RESET"
  else
    printf '    %s%-2s%s  %-30s %s%s%s\n' "$C_DIM" "$number" "$C_RESET" "$title" "$C_DIM" "$hint" "$C_RESET"
  fi
}

draw_control_center() {
  local selected="$1" digit_buffer="$2" state="NOT INSTALLED" state_color="$C_DIM" endpoint="Not installed" update_color="$C_DIM" tls_bind tls_port scheme
  if valid_menu_choice "$digit_buffer" && [[ "$digit_buffer" != 0 ]]; then selected="$digit_buffer"; fi
  if [[ -f "$SERVICE_FILE" ]]; then
    state="STOPPED"
    state_color="$C_YELLOW"
    if has systemctl && systemctl is-active --quiet "$SERVICE_NAME"; then
      state="RUNNING"
      state_color="$C_GREEN"
    fi
  fi
  if [[ -s "$DOMAIN_FILE" ]]; then
    endpoint="https://$(<"$DOMAIN_FILE")"
    tls_bind="$(awk -F= '$1 == "ARG_TLS_BIND" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
    tls_port="${tls_bind##*:}"
    [[ -n "$tls_port" && "$tls_port" != "443" ]] && endpoint="$endpoint:$tls_port"
  elif [[ -f "$ENV_FILE" ]]; then
    if [[ -n "$SERVER_PUBLIC_IP" ]]; then
      endpoint="http://$SERVER_PUBLIC_IP:$(panel_port)"
    else
      endpoint="http://<SERVER_IP>:$(panel_port)"
    fi
  fi

  printf '\033[2J\033[H'
  show_wordmark
  printf '  %sVPS CONTROL CENTER%s\n' "$C_DIM" "$C_RESET"
  printf '  %s────────────────────────────────────────────────────────────────────%s\n' "$C_COPPER" "$C_RESET"
  printf '  Service  %s● %s%s\n' "$state_color" "$state" "$C_RESET"
  printf '  Panel    '
  if [[ "$endpoint" == http* ]]; then
    printf '\033]8;;%s\033\\%s%s%s\033]8;;\033\\\n' "$endpoint" "$C_BOLD" "$endpoint" "$C_RESET"
  else
    printf '%s%s%s\n' "$C_BOLD" "$endpoint" "$C_RESET"
  fi
  case "$UPDATE_STATUS" in
    AVAILABLE) update_color="$C_YELLOW"; printf '  Update   %s● AVAILABLE  %s → %s  (9 to update)%s\n' "$update_color" "$UPDATE_LOCAL_REV" "$UPDATE_REMOTE_REV" "$C_RESET" ;;
    'UP TO DATE') update_color="$C_GREEN"; printf '  Update   %s● UP TO DATE  %s%s\n' "$update_color" "$UPDATE_LOCAL_REV" "$C_RESET" ;;
    'NOT INSTALLED') printf '  Update   %s● NOT INSTALLED%s\n' "$C_DIM" "$C_RESET" ;;
    CHECKING) printf '  Update   %s◌ CHECKING IN BACKGROUND%s\n' "$C_DIM" "$C_RESET" ;;
    *) printf '  Update   %s● CHECK FAILED%s\n' "$C_YELLOW" "$C_RESET" ;;
  esac
  printf '  %s────────────────────────────────────────────────────────────────────%s\n\n' "$C_COPPER" "$C_RESET"

  printf '  %sSETUP%s\n' "$C_CYAN" "$C_RESET"
  draw_menu_item "$selected" 1 'Install / repair' 'Build and configure the service'
  printf '\n  %sSERVICE%s\n' "$C_CYAN" "$C_RESET"
  draw_menu_item "$selected" 2 'Service status' 'View systemd health and details'
  draw_menu_item "$selected" 3 'Start service' 'Bring the panel online'
  draw_menu_item "$selected" 4 'Stop service' 'Stop the background service'
  draw_menu_item "$selected" 5 'Restart service' 'Restart after configuration changes'
  draw_menu_item "$selected" 6 'View service logs' 'Inspect recent ARG logs'
  draw_menu_item "$selected" 7 'Change panel port' 'Update the public listening port'
  printf '\n  %sSECURITY & MAINTENANCE%s\n' "$C_CYAN" "$C_RESET"
  draw_menu_item "$selected" 8 'Configure HTTPS / SSL' 'Get a certificate and enable Rust TLS'
  draw_menu_item "$selected" 9 'Update ARG' 'Fetch and install the latest version'
  draw_menu_item "$selected" 10 'Uninstall, keep database' 'Remove ARG and retain its data'
  draw_menu_item "$selected" 11 'Uninstall and purge data' 'Remove ARG and all its data'
  draw_menu_item "$selected" 12 'Get one-time setup code' 'Display the first-admin code'
  draw_menu_item "$selected" 13 'Reset account password' 'Root reset; no old password needed'
  draw_menu_item "$selected" 14 'Cancel running update' 'Stop a stuck build and release its lock'
  draw_menu_item "$selected" 15 'Rollback previous version' 'Restore the saved binary and restart ARG'
  printf '\n  Type option%s%s%s then %sEnter%s   %s↑%s / %s↓%s move   %sq%s quit' \
    "$([[ -n "$digit_buffer" ]] && printf ' [' || true)" "$digit_buffer" "$([[ -n "$digit_buffer" ]] && printf ']' || true)" \
    "$C_BOLD" "$C_RESET" "$C_BOLD" "$C_RESET" "$C_BOLD" "$C_RESET" "$C_BOLD" "$C_RESET"
  [[ -n "$digit_buffer" ]] || printf '  (0 to exit)'
  printf '\n\n'
}

read_menu_key() {
  local saved key sequence
  saved="$(stty -g </dev/tty 2>/dev/null)" || return 1
  stty -echo -icanon min 1 time 0 </dev/tty 2>/dev/null || return 1
  if IFS= read -r -s -n 1 -t 1 key </dev/tty; then
    [[ -n "$key" ]] || key='enter'
  else
    key='tick'
  fi
  if [[ "$key" == $'\033' ]]; then
    IFS= read -r -s -n 2 -t 1 sequence </dev/tty || sequence=""
    case "$sequence" in
      '[A') key='up' ;;
      '[B') key='down' ;;
      *) key='escape' ;;
    esac
  elif [[ "$key" == $'\r' || "$key" == $'\n' ]]; then
    key='enter'
  fi
  stty "$saved" </dev/tty 2>/dev/null || true
  printf '%s' "$key"
}

run_menu_action() {
  case "$1" in
    1) install_app ;;
    2) show_status ;;
    3) run_service_action start ;;
    4) run_service_action stop ;;
    5) run_service_action restart ;;
    6) view_logs 100 ;;
    7) change_port ;;
    8) configure_https ;;
    9) update_app ;;
    10) delete_app "" ;;
    11) delete_app "--purge" ;;
    12) get_setup_code ;;
    13) change_account_password ;;
    14) require_root cancel-update; "$UPDATE_HELPER" cancel ;;
    15) require_root rollback; confirm_rollback; "$UPDATE_HELPER" rollback ;;
  esac
}

confirm_rollback() {
  local answer
  warn "Rollback restores the previous ARG binary and its database snapshot. Database changes made since that snapshot will be lost."
  read_answer 'Continue with rollback? [y/N]: ' answer 'n'
  [[ "${answer,,}" == y || "${answer,,}" == yes ]] || die "Rollback cancelled."
}

main_menu() {
  local selection="" key interactive=0 action_ready=0 draw_needed=1
  local digit_buffer=""
  local selected=1
  MENU_ACTIVE=1
  UPDATE_STATUS="NOT INSTALLED"
  UPDATE_LOCAL_REV=""
  UPDATE_REMOTE_REV=""
  if [[ -f "$SERVICE_FILE" ]]; then
    UPDATE_STATUS="CHECKING"
    start_menu_status_refresh
    selected=2
  fi
  if [[ -r /dev/tty && -t 1 && -t 0 && "${TERM:-dumb}" != "dumb" ]] && has stty; then
    interactive=1
  fi

  while true; do
    read_menu_status_refresh && draw_needed=1
    if (( interactive )); then
      if (( draw_needed )); then
        draw_control_center "$selected" "$digit_buffer"
        draw_needed=0
      fi
      key="$(read_menu_key)" || interactive=0
      if (( interactive )); then
        [[ "$key" == tick ]] && continue
        draw_needed=1
        case "$key" in
          up|k) (( selected > 1 )) && ((selected--)) || selected=15 ;;
          down|j) (( selected < 15 )) && ((selected++)) || selected=1 ;;
          enter)
            if [[ -n "$digit_buffer" ]]; then
              [[ "$digit_buffer" != "0" ]] || { stop_menu_status_refresh; printf '\033[0m\n'; return 0; }
              selection="$digit_buffer"
              [[ "$selection" =~ ^([1-9]|1[0-5])$ ]] || { warn 'Choose a menu option from 1 to 15.'; selection=""; digit_buffer=""; continue; }
            else
              selection="$selected"
            fi
            digit_buffer=""
            action_ready=1
            ;;
          q|Q|escape) stop_menu_status_refresh; printf '\033[0m\n'; return 0 ;;
          h|H) usage; pause_menu; continue ;;
          [0-9۰۱۲۳۴۵۶۷۸۹٠١٢٣٤٥٦٧٨٩])
            digit_buffer="$(normalize_menu_digits "$digit_buffer$key")"
            if ((${#digit_buffer} > 2)); then digit_buffer="${digit_buffer:0:2}"; fi
            ;;
          backspace|$'\177') digit_buffer="${digit_buffer%?}" ;;
          *) continue ;;
        esac
      fi
    else
      show_wordmark
      printf '  %sVPS CONTROL CENTER%s\n' "$C_DIM" "$C_RESET"
      printf '  Service: %s\n  Panel: %s\n\n' \
        "$( [[ -f "$SERVICE_FILE" ]] && systemctl is-active "$SERVICE_NAME" 2>/dev/null || printf 'not installed' )" \
      "$( [[ -s "$DOMAIN_FILE" ]] && printf 'https://%s' "$(<"$DOMAIN_FILE")" || printf 'http://%s:%s' "${SERVER_PUBLIC_IP:-<SERVER_IP>}" "$(panel_port 2>/dev/null || printf '8282')" )"
      case "$UPDATE_STATUS" in
        AVAILABLE) printf '  Update: AVAILABLE  %s -> %s (option 9)\n\n' "$UPDATE_LOCAL_REV" "$UPDATE_REMOTE_REV" ;;
        'UP TO DATE') printf '  Update: UP TO DATE  %s\n\n' "$UPDATE_LOCAL_REV" ;;
        'NOT INSTALLED') printf '  Update: NOT INSTALLED\n\n' ;;
        CHECKING) printf '  Update: CHECKING IN BACKGROUND\n\n' ;;
        *) printf '  Update: CHECK FAILED\n\n' ;;
      esac
      printf '  1 Install / repair     2 Status       3 Start          4 Stop\n'
      printf '  5 Restart              6 Logs         7 Change port    8 HTTPS / SSL\n'
      printf '  9 Update              10 Uninstall    11 Purge data\n'
      printf ' 12 Get setup code      13 Change password\n'
      printf ' 14 Cancel update       15 Rollback previous version\n'
      printf '  0 Exit\n\n'
      read -r -p 'Select an option: ' selection || { stop_menu_status_refresh; return 0; }
      selection="$(normalize_menu_digits "${selection//$'\r'/}")"
      [[ "$selection" == "0" ]] && { stop_menu_status_refresh; return 0; }
      valid_menu_choice "$selection" || { warn 'Choose a menu option from 1 to 15.'; continue; }
      action_ready=1
    fi

    if (( action_ready )) && [[ -n "$selection" ]]; then
      run_menu_action "$selection"
      selection=""
      action_ready=0
      pause_menu
    fi
  done
}

delete_app() {
  require_root delete
  require_systemd
  local purge="${1:-}"
  local keep_database="n" db_path entry answer
  local cert_name
  if [[ "$purge" == "--purge-confirmed" ]]; then
    purge="--purge"
  elif [[ "$purge" == "--purge" ]]; then
    warn "This removes the service, installer, source, configuration, and database."
    read_answer 'Type PURGE ARG to confirm: ' confirmation
    [[ "$confirmation" == "PURGE ARG" ]] || die "Purge cancelled."
  else
    warn "ARG services, binaries, source, system account, and configuration will be removed."
    read_answer 'Keep the database for the next installation? [Y/n]: ' answer 'y'
    case "${answer,,}" in
      n|no) keep_database="n" ;;
      y|yes) keep_database="y" ;;
      *) die "Choose Y to keep the database or N to remove it." ;;
    esac
    read_answer 'Type DELETE ARG to confirm: ' confirmation
    [[ "$confirmation" == "DELETE ARG" ]] || die "Uninstall cancelled."
  fi
  # Normalize earlier DONATELLO paths first, so uninstall removes the old
  # service/config/source too and the keep-database option preserves its DB.
  migrate_legacy_install
  db_path="$(configured_db_path)"
  if [[ "$keep_database" == "y" ]]; then
    install -d -o root -g root -m 0750 "$DATA_DIR"
    printf '%s\n' "$db_path" > "$DATA_DIR/.database-path"
    chmod 0600 "$DATA_DIR/.database-path"
  fi
  systemctl disable --now "$SERVICE_NAME" "$LEGACY_SERVICE_NAME" 2>/dev/null || true
  systemctl disable --now arg-privileged.service donatello-privileged.service 2>/dev/null || true
  if has ufw && ufw status 2>/dev/null | rg -q '^Status: active'; then
    while IFS= read -r rule_id; do
      [[ -n "$rule_id" ]] || continue
      ufw --force delete "$rule_id" >/dev/null 2>&1 || true
    done < <(ufw status numbered 2>/dev/null | awk '/# ARG / || /# DONATELLO / {if(match($0,/^[[:space:]]*\[[[:space:]]*[0-9]+\]/)){id=substr($0,RSTART,RLENGTH); gsub(/[^0-9]/,"",id); print id}}' | sort -rn)
  fi
  systemctl disable --now arg-certbot-renew.timer donatello-certbot-renew.timer 2>/dev/null || true
  rm -f "$CERTBOT_TIMER_SERVICE" "$CERTBOT_TIMER_UNIT" \
    "$OLD_CERTBOT_TIMER_SERVICE" "$OLD_CERTBOT_TIMER_UNIT"
  rm -f "$SERVICE_FILE" "$PRIVILEGED_SERVICE_FILE" "$OLD_SERVICE_FILE" "$OLD_PRIVILEGED_SERVICE_FILE" \
    "$BINARY" "$CLI" "$LEGACY_INSTALLER" "$OLD_CLI" "$OLD_LEGACY_INSTALLER" \
    "$SUDOERS_FILE" /etc/sudoers.d/donatello-ufw-status \
    "$UFW_HELPER" "$PORT_HELPER" "$UPDATE_HELPER" "$DB_BACKUP_TOOL" /usr/local/lib/donatello/ufw-helper
  rm -f "$LEGACY_NGINX_SITE_LINK" "$LEGACY_NGINX_SITE" "$OLD_NGINX_SITE_LINK" "$OLD_NGINX_SITE" \
    "$CERT_DEPLOY_HOOK" "$OLD_CERT_DEPLOY_HOOK"
  if [[ -x /snap/bin/certbot ]]; then
    for cert_name in "$CERT_NAME" "$LEGACY_SERVICE_NAME"; do
      [[ -d "/etc/letsencrypt/live/$cert_name" ]] || continue
      /snap/bin/certbot delete --cert-name "$cert_name" --non-interactive >/dev/null 2>&1 || true
    done
  elif has certbot; then
    for cert_name in "$CERT_NAME" "$LEGACY_SERVICE_NAME"; do
      [[ -d "/etc/letsencrypt/live/$cert_name" ]] || continue
      certbot delete --cert-name "$cert_name" --non-interactive >/dev/null 2>&1 || true
    done
  fi
  if has nginx; then
    nginx -t >/dev/null 2>&1 && systemctl reload nginx 2>/dev/null || true
  fi
  rm -rf /usr/local/lib/donatello
  rmdir /usr/local/lib/arg 2>/dev/null || true
  rm -rf "$SOURCE_DIR" "$OLD_SOURCE_DIR"
  rmdir "$INSTALL_DIR" "$OLD_INSTALL_DIR" 2>/dev/null || true
  rm -rf "$ENV_DIR" "$OLD_ENV_DIR" "$OLD_DATA_DIR"
  systemctl daemon-reload
  systemctl reset-failed "$SERVICE_NAME" 2>/dev/null || true
  userdel "$SERVICE_USER" 2>/dev/null || true
  userdel "$LEGACY_SERVICE_NAME" 2>/dev/null || true
  groupdel "$SERVICE_GROUP" 2>/dev/null || true
  groupdel "$LEGACY_SERVICE_NAME" 2>/dev/null || true
  if [[ "$purge" == "--purge" || "$keep_database" != "y" ]]; then
    rm -f "$db_path" "$db_path-wal" "$db_path-shm"
    rm -rf "$DATA_DIR"
    if [[ "$purge" == "--purge" ]]; then
      say "ARG and all application data have been removed."
    else
      say "ARG has been uninstalled and its database was removed."
    fi
  else
    for entry in "$DATA_DIR"/* "$DATA_DIR"/.[!.]* "$DATA_DIR"/..?*; do
      [[ -e "$entry" || -L "$entry" ]] || continue
      [[ "$entry" == "$DATA_DIR/.database-path" || "$entry" == "$db_path" ||
        "$db_path" == "$entry/"* || "$entry" == "$db_path-wal" || "$entry" == "$db_path-shm" ]] && continue
      rm -rf -- "$entry"
    done
    for entry in "$db_path" "$db_path-wal" "$db_path-shm"; do
      [[ -f "$entry" ]] || continue
      chown root:root "$entry"
      chmod 0600 "$entry"
    done
    chown root:root "$DATA_DIR"
    chmod 0750 "$DATA_DIR"
    say "ARG has been uninstalled. The database is kept for the next installation."
    info "Retained database: $db_path"
  fi
}

usage() {
  cat <<'HELP'
ARG · Installer and System Service Manager

Usage:
  sudo arg                  Open the interactive control center
  sudo arg menu             Open the interactive control center
  sudo bash install.sh             Open the interactive installer menu
  sudo bash install.sh install     Install ARG
  sudo arg manage [status|start|stop|restart|logs|port|ssl|update|delete]
  sudo arg ssl
  sudo arg update
  sudo arg cancel-update
  sudo arg rollback
  sudo arg password
  sudo arg delete [--purge]

Commands:
  install          Build ARG and install its systemd service
  manage           Open the interactive control center
  status           Show service status
  start|stop       Start or stop the systemd service
  restart          Restart the systemd service
  logs [count]     Show recent service logs
  update [main|development] [fast|complete]
                   Select the source branch and cache policy, then build/install with automatic rollback
  cancel-update    Cancel an active update or clear a stale update lock
  rollback         Restore the last saved ARG binary and restart the service
  password         Reset an account password as root
  ssl              Obtain a Let's Encrypt certificate and enable Rust TLS
  delete           Uninstall the service while preserving application data
  delete --purge   Uninstall and remove all configuration and application data

Supported systems: Debian and Ubuntu with systemd.
HELP
}

if [[ "${1:-menu}" != "menu" && !( "${1:-menu}" == "manage" && -z "${2:-}" ) ]]; then
  show_banner
fi

case "${1:-menu}" in
  install) install_app ;;
  manage) manage_app "$@" ;;
  menu) main_menu ;;
  status) show_status ;;
  start|stop|restart)
    require_root "$1"
    require_systemd
    [[ -f "$SERVICE_FILE" ]] || die "ARG is not installed."
    systemctl "$1" "$SERVICE_NAME"
    ;;
  logs)
    view_logs "${2:-100}"
    ;;
  update) update_app "${2:-}" "${3:-}" ;;
  cancel-update) require_root cancel-update; "$UPDATE_HELPER" cancel ;;
  rollback) require_root rollback; confirm_rollback; "$UPDATE_HELPER" rollback ;;
  ssl) configure_https ;;
  password) change_account_password ;;
  delete) delete_app "${2:-}" ;;
  help|-h|--help) usage ;;
  *) usage >&2; exit 2 ;;
esac
