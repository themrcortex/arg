#!/usr/bin/env bash
set -Eeuo pipefail

readonly APP_NAME="ARG"
readonly SERVICE_NAME="arg"
readonly LEGACY_SERVICE_NAME="donatello"
readonly REPOSITORY="https://github.com/themrcortex/arg.git"
readonly INSTALL_SCRIPT_URL="https://raw.githubusercontent.com/themrcortex/arg/main/install.sh"
requested_branch="${ARG_UPDATE_BRANCH:-${ARG_BRANCH:-main}}"
case "$requested_branch" in
  main|development) ;;
  *) printf 'Invalid ARG update branch. Allowed: main, development.\n' >&2; exit 2 ;;
esac
readonly BRANCH="$requested_branch"
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
readonly UPDATE_LOCK_DIR="/run/arg-update.lock"
readonly UPDATE_STATUS_FILE="/run/arg-update-in-progress"
readonly UPDATE_STATE_FILE="/run/arg-update-state.json"
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
    0|1|2|3|4|5|6|7|8|9|10|11|12|13) return 0 ;;
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
  apt-get install -y --no-install-recommends ca-certificates curl git sudo build-essential pkg-config iproute2 ripgrep
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

build_release() {
  [[ -f "$SOURCE_DIR/frontend/dist/index.html" ]] || die "Built frontend files are missing. Check the selected ARG branch and repository."
  if [[ "${ARG_UPDATE_RESERVED:-0}" == "1" && "${ARG_UPDATE_MODE:-complete}" == "complete" ]]; then
    run_with_progress "Clearing previous Rust build cache" bash -c 'cd "$1" && cargo +stable clean' _ "$SOURCE_DIR"
  fi
  run_with_progress "Compiling the optimized Rust release" bash -c 'cd "$1" && cargo +stable build --locked --release' _ "$SOURCE_DIR"
  [[ -x "$SOURCE_DIR/target/release/arg" ]] || die "The ARG binary was not created."
}

run_with_progress() {
  local label="$1" log_file spinner_index=0 started elapsed minutes seconds pid result
  shift
  log_file="$(mktemp /tmp/arg-build.XXXXXX)"

  if [[ ! -t 1 || ! -t 2 ]]; then
    say "$label"
    if "$@" >"$log_file" 2>&1; then
      info "Build completed successfully."
      rm -f "$log_file"
      return 0
    else
      result=$?
      cat "$log_file" >&2
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
  local address
  address="$(curl -4 --connect-timeout 3 --max-time 6 -fsS https://api.ipify.org 2>/dev/null || true)"
  [[ "$address" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]] || return 1
  printf '%s\n' "$address"
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
  apt-get install -y --no-install-recommends snapd
  systemctl enable --now snapd.socket
  snap install certbot --classic
  [[ -x /snap/bin/certbot ]] || die "The current Certbot could not be installed from Snap."
  CERTBOT_BIN="/snap/bin/certbot"
  version="$("$CERTBOT_BIN" --version 2>&1 | rg -o '[0-9]+(\.[0-9]+){1,2}' | head -n 1 || true)"
  [[ -n "$version" ]] || version="0.0.0"
  minimum="$(printf '%s\n' 5.4.0 "$version" | sort -V | head -n 1)"
  [[ "$minimum" == "5.4.0" ]] || die "Certbot 5.4 or newer is required for IP certificates."
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
    ufw --force delete "$rule_id" >/dev/null 2>&1 || true
  done < <(ufw status numbered 2>/dev/null | awk '/# ARG panel|# ARG HTTPS challenge|# DONATELLO panel|# DONATELLO HTTPS challenge/ {if(match($0,/^[[:space:]]*\[[[:space:]]*[0-9]+\]/)){id=substr($0,RSTART,RLENGTH); gsub(/[^0-9]/,"",id); print id}}' | sort -rn)
  if [[ -s "$DOMAIN_FILE" && -s "$tls_cert" && -s "$tls_key" && -n "$tls_port" ]]; then
    # HTTPS is the public entry point; leave the HTTP backend port closed in UFW.
    ufw allow 80/tcp comment 'ARG HTTPS challenge' >/dev/null
    ufw allow "$tls_port/tcp" comment 'ARG HTTPS panel' >/dev/null
  else
    ufw allow "$port/tcp" comment 'ARG panel' >/dev/null
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
  local -a hooks=() identifier_args=()
  write_cert_deploy_hook
  if [[ "$is_ip" == "y" ]]; then
    identifier_args=(--ip-address "$host" --preferred-profile shortlived)
  else
    identifier_args=(-d "$host")
  fi
  if [[ -n "$port80_unit" ]]; then
    info "Temporarily stopping $port80_unit to release TCP port 80."
    systemctl stop "$port80_unit" || die "Could not stop $port80_unit; no certificate request was made."
    PORT80_STOPPED_UNIT="$port80_unit"
    trap 'restore_port80_service' EXIT INT TERM
    stopped="y"
    hooks+=(--pre-hook "systemctl stop $port80_unit" --post-hook "systemctl start $port80_unit")
  fi

  if "$CERTBOT_BIN" certonly --standalone --preferred-challenges http --non-interactive \
    --agree-tos --email "$email" --cert-name "$CERT_NAME" "${identifier_args[@]}" \
    --deploy-hook "$CERT_DEPLOY_HOOK" "${hooks[@]}"; then
    if [[ "$stopped" == "y" ]] && ! restore_port80_service; then
      die "The certificate was issued, but $port80_unit could not be restarted."
    fi
    trap - EXIT INT TERM
    return 0
  fi
  if [[ "$stopped" == "y" ]] && ! restore_port80_service; then
    die "Certificate issuance failed and $port80_unit could not be restarted."
  fi
  trap - EXIT INT TERM
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
    [[ "$host" =~ ^([A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)+[A-Za-z]{2,63}$ ]] || die "Enter a valid fully qualified domain name or IPv4 address."
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
  [[ "$email" == *@*.* ]] || die "Enter a valid email address for the certificate."
  install_https_dependencies
  [[ "$is_ip" != "y" ]] || ensure_ip_certbot
  port80_unit="$(port80_systemd_service)" || die "TCP port 80 is occupied by a process that cannot be safely paused as a systemd service. Stop it manually, then run 'sudo arg ssl' again."
  if ! issue_certificate "$host" "$email" "$port80_unit" "$is_ip"; then
    rm -f "$CERT_DEPLOY_HOOK"
    warn "Certificate issuance failed. The panel remains available through localhost or an SSH tunnel."
    return 0
  fi

  port="$(available_tls_port)" || {
    certbot delete --cert-name "$CERT_NAME" --non-interactive >/dev/null 2>&1 || true
    rm -f "$CERT_DEPLOY_HOOK"
    die "No available HTTPS port was found (tried 443, 8443, 9443, and 10443)."
  }
  env_backup="$(mktemp /etc/arg/.arg.env.backup.XXXXXX)"
  cp -p "$ENV_FILE" "$env_backup"
  copy_certificate_files "/etc/letsencrypt/live/$CERT_NAME"
  printf '%s\n' "$host" > "$DOMAIN_FILE"
  chown root:"$SERVICE_GROUP" "$DOMAIN_FILE"
  chmod 0640 "$DOMAIN_FILE"
  set_env_value ARG_TLS_CERT "$TLS_CERT"
  set_env_value ARG_TLS_KEY "$TLS_KEY"
  set_env_value ARG_TLS_BIND "0.0.0.0:$port"
  sync_ufw_rules
  ensure_certbot_renewal
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

install_cli() {
  local staged
  staged="$(mktemp /usr/local/bin/.arg-cli.XXXXXX)"
  curl --proto '=https' --tlsv1.2 -fsSL "$INSTALL_SCRIPT_URL" -o "$staged"
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

update_app() {
  require_root update
  require_systemd
  local was_installed=0 build_started=0
  export ARG_UPDATE_MODE="${ARG_UPDATE_MODE:-complete}"
  case "$ARG_UPDATE_MODE" in fast|complete) ;; *) die "Invalid update build mode." ;; esac
  [[ -f "$SERVICE_FILE" || -f "$OLD_SERVICE_FILE" ]] && was_installed=1
  (( was_installed )) || die "ARG is not installed. Run 'arg install' first."
  if [[ "${ARG_UPDATE_RESERVED:-0}" == "1" ]]; then
    [[ -d "$UPDATE_LOCK_DIR" && -r "$UPDATE_STATUS_FILE" ]] || die "The scheduled update reservation is missing. Please retry from the panel."
  else
    if [[ -d "$UPDATE_LOCK_DIR" ]]; then
      local reserved_at now_at lock_time stale_after=60
      reserved_at="$(awk 'NR==1 {print $1}' "$UPDATE_STATUS_FILE" 2>/dev/null || true)"
      now_at="$(date +%s)"
      if [[ "$reserved_at" =~ ^[0-9]+$ ]]; then
        stale_after=21600
        lock_time="$reserved_at"
      else
        lock_time="$(stat -c %Y "$UPDATE_LOCK_DIR" 2>/dev/null || echo "$now_at")"
      fi
      if (( now_at - lock_time > stale_after )); then
        rm -f "$UPDATE_STATUS_FILE"
        rmdir "$UPDATE_LOCK_DIR" 2>/dev/null || true
      fi
    fi
    mkdir "$UPDATE_LOCK_DIR" 2>/dev/null || die "Another ARG update is already running. Please wait for it to finish."
    printf '%s %s\n' "$(date +%s)" "$$" > "$UPDATE_STATUS_FILE"
    export ARG_UPDATE_JOB_ID="arg-$(cat /proc/sys/kernel/random/uuid 2>/dev/null || date +%s)-$$"
  fi
  chmod 0644 "$UPDATE_STATUS_FILE"
  [[ -n "${ARG_UPDATE_JOB_ID:-}" ]] || export ARG_UPDATE_JOB_ID="arg-$(cat /proc/sys/kernel/random/uuid 2>/dev/null || date +%s)-$$"
  update_phase() { "$UPDATE_HELPER" state running "$1" "${2:-}" >/dev/null 2>&1 || true; }
  cleanup_update_build_cache() {
    (( build_started )) || return 0
    [[ "$ARG_UPDATE_MODE" == "complete" ]] || return 0
    if bash -c 'cd "$1" && cargo +stable clean' _ "$SOURCE_DIR" >/dev/null 2>&1; then
      info "Removed the Rust build cache to reclaim disk space."
    else
      warn "The update succeeded, but the Rust build cache could not be removed. Run 'cargo clean' in $SOURCE_DIR later."
    fi
  }
  cleanup_update_reservation() {
    local exit_code=$?
    if (( exit_code != 0 )); then
      "$UPDATE_HELPER" state failed failed "The update stopped before successful verification. Review the update log and retry." >/dev/null 2>&1 || true
    fi
    cleanup_update_build_cache
    rm -f "$UPDATE_STATUS_FILE"
    rmdir "$UPDATE_LOCK_DIR" 2>/dev/null || true
    return "$exit_code"
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
  build_started=1
  build_release
  update_phase installing "Installing the verified build."
  ensure_service_account
  configure_ufw_status
  take_database_ownership
  local previous staged
  previous="$(mktemp /var/tmp/arg.previous.XXXXXX)"
  install -d -o root -g root -m 0755 "$(dirname "$BINARY")"
  staged="$(mktemp /usr/local/lib/arg/.arg.XXXXXX)"
  if [[ -x "$BINARY" ]]; then
    cp -p "$BINARY" "$previous"
  else
    rm -f "$previous"
  fi
  install -o root -g root -m 0755 "$SOURCE_DIR/target/release/arg" "$staged"
  mv -f "$staged" "$BINARY"
  write_privileged_service_unit
  systemctl restart arg-privileged.service
  write_service_unit
  install_cli
  update_phase restarting "Restarting the ARG service."
  systemctl restart "$SERVICE_NAME"
  update_phase verifying "Checking that the updated service is active."
  if wait_until_active; then
    rm -f "$previous"
    cleanup_legacy_identity
    say "ARG was updated successfully"
    info "Source revision: $(git -C "$SOURCE_DIR" rev-parse --short HEAD)"
    "$UPDATE_HELPER" state succeeded complete "ARG is updated and the service is active." >/dev/null 2>&1 || true
    show_setup_code_if_needed
    if (( MENU_ACTIVE )); then
      cleanup_update_reservation
      trap - EXIT
      exec "$CLI" menu
    fi
    return
  fi
  if [[ -s "$previous" ]]; then
    warn "The new release did not start. Restoring the previous binary."
    install -o root -g root -m 0755 "$previous" "$BINARY"
    systemctl restart "$SERVICE_NAME" || true
  fi
  rm -f "$previous"
  journalctl -u "$SERVICE_NAME" -n 50 --no-pager || true
  die "The update could not be activated."
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
  [[ -f "$ENV_FILE" ]] || die "Configuration was not found. ARG is not installed."
  local port
  port="${1:-}"
  if [[ -z "$port" ]]; then read -r -p "New panel port (1024-65535): " port; fi
  [[ "$port" =~ ^[0-9]{1,5}$ ]] && (( port >= 1024 && port <= 65535 )) || die "Invalid port. Enter a number from 1024 to 65535."
  write_bind_config "0.0.0.0:$port"
  sync_ufw_rules
  systemctl restart "$SERVICE_NAME"
  wait_until_active || die "The service did not start after changing the port."
  wait_for_panel || die "The service is running but the web panel did not answer on the new port."
  local domain
  domain="$(cat "$DOMAIN_FILE" 2>/dev/null || true)"
  if [[ -n "$domain" && -f "$LEGACY_NGINX_SITE" ]]; then
    sed -E -i "s#proxy_pass http://127\\.0\\.0\\.1:[0-9]+;#proxy_pass http://127.0.0.1:$port;#" "$LEGACY_NGINX_SITE"
    nginx -t && systemctl reload nginx || die "The panel port changed, but the legacy HTTPS proxy could not be updated."
  fi
  say "The panel is publicly listening on port $port"
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
      ssl) configure_https ;;
      update) update_app ;;
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
  if IFS= read -r -s -n 1 key </dev/tty; then
    [[ -n "$key" ]] || key='enter'
  else
    key=""
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
  esac
}

main_menu() {
  local selection="" key interactive=0 action_ready=0
  local digit_buffer=""
  local selected=1
  MENU_ACTIVE=1
  check_update_status
  [[ -f "$SERVICE_FILE" ]] && selected=2
  if [[ -z "$SERVER_PUBLIC_IP" && -f "$SERVICE_FILE" ]]; then SERVER_PUBLIC_IP="$(detect_public_ipv4 || true)"; fi
  if [[ -r /dev/tty && -t 1 && -t 0 && "${TERM:-dumb}" != "dumb" ]] && has stty; then
    interactive=1
  fi

  while true; do
    if (( interactive )); then
      draw_control_center "$selected" "$digit_buffer"
      key="$(read_menu_key)" || interactive=0
      if (( interactive )); then
        case "$key" in
          up|k) (( selected > 1 )) && ((selected--)) || selected=13 ;;
          down|j) (( selected < 13 )) && ((selected++)) || selected=1 ;;
          enter)
            if [[ -n "$digit_buffer" ]]; then
              [[ "$digit_buffer" != "0" ]] || { printf '\033[0m\n'; return 0; }
              selection="$digit_buffer"
              [[ "$selection" =~ ^([1-9]|1[0-3])$ ]] || { warn 'Choose a menu option from 1 to 13.'; selection=""; digit_buffer=""; continue; }
            else
              selection="$selected"
            fi
            digit_buffer=""
            action_ready=1
            ;;
          q|Q|escape) printf '\033[0m\n'; return 0 ;;
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
      SERVER_PUBLIC_IP="${SERVER_PUBLIC_IP:-$(detect_public_ipv4 || true)}"
      show_wordmark
      printf '  %sVPS CONTROL CENTER%s\n' "$C_DIM" "$C_RESET"
      printf '  Service: %s\n  Panel: %s\n\n' \
        "$( [[ -f "$SERVICE_FILE" ]] && systemctl is-active "$SERVICE_NAME" 2>/dev/null || printf 'not installed' )" \
      "$( [[ -s "$DOMAIN_FILE" ]] && printf 'https://%s' "$(<"$DOMAIN_FILE")" || printf 'http://%s:%s' "${SERVER_PUBLIC_IP:-<SERVER_IP>}" "$(panel_port 2>/dev/null || printf '8282')" )"
      case "$UPDATE_STATUS" in
        AVAILABLE) printf '  Update: AVAILABLE  %s -> %s (option 9)\n\n' "$UPDATE_LOCAL_REV" "$UPDATE_REMOTE_REV" ;;
        'UP TO DATE') printf '  Update: UP TO DATE  %s\n\n' "$UPDATE_LOCAL_REV" ;;
        'NOT INSTALLED') printf '  Update: NOT INSTALLED\n\n' ;;
        *) printf '  Update: CHECK FAILED\n\n' ;;
      esac
      printf '  1 Install / repair     2 Status       3 Start          4 Stop\n'
      printf '  5 Restart              6 Logs         7 Change port    8 HTTPS / SSL\n'
      printf '  9 Update              10 Uninstall    11 Purge data\n'
      printf ' 12 Get setup code      13 Change password\n'
      printf '  0 Exit\n\n'
      read -r -p 'Select an option: ' selection || return 0
      selection="$(normalize_menu_digits "${selection//$'\r'/}")"
      [[ "$selection" == "0" ]] && return 0
      valid_menu_choice "$selection" || { warn 'Choose a menu option from 1 to 13.'; continue; }
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
  if [[ "$purge" == "--purge" ]]; then
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
    "$UFW_HELPER" "$PORT_HELPER" "$UPDATE_HELPER" /usr/local/lib/donatello/ufw-helper
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
  sudo arg password
  sudo arg delete [--purge]

Commands:
  install          Build ARG and install its systemd service
  manage           Open the interactive control center
  status           Show service status
  start|stop       Start or stop the systemd service
  restart          Restart the systemd service
  logs [count]     Show recent service logs
  update           Fetch main, build, replace the binary, and roll back on failure
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
  update) update_app ;;
  ssl) configure_https ;;
  password) change_account_password ;;
  delete) delete_app "${2:-}" ;;
  help|-h|--help) usage ;;
  *) usage >&2; exit 2 ;;
esac
