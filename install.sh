#!/usr/bin/env bash
set -Eeuo pipefail

readonly APP_NAME="Donatello"
readonly SERVICE_NAME="donatello"
readonly REPOSITORY="https://github.com/parsasajjadi2008/donatello.git"
readonly INSTALL_SCRIPT_URL="https://raw.githubusercontent.com/parsasajjadi2008/donatello/main/install.sh"
readonly BRANCH="main"
readonly INSTALL_DIR="/opt/donatello"
readonly SOURCE_DIR="/opt/donatello/source"
readonly BINARY="/usr/local/lib/donatello/donatello"
readonly CLI="/usr/local/bin/donatello"
readonly LEGACY_INSTALLER="/usr/local/sbin/donatello-installer"
readonly SERVICE_FILE="/etc/systemd/system/donatello.service"
readonly SUDOERS_FILE="/etc/sudoers.d/donatello-ufw-status"
readonly ENV_DIR="/etc/donatello"
readonly ENV_FILE="/etc/donatello/donatello.env"
readonly DATA_DIR="/var/lib/donatello"
readonly SERVICE_USER="donatello"
readonly SERVICE_GROUP="donatello"
readonly DEFAULT_BIND="127.0.0.1:8282"
readonly DEFAULT_DB="/var/lib/donatello/donatello.db"

readonly RUST_HOME="${RUSTUP_HOME:-/root/.rustup}"
readonly CARGO_HOME_DIR="${CARGO_HOME:-/root/.cargo}"
export RUSTUP_HOME="$RUST_HOME"
export CARGO_HOME="$CARGO_HOME_DIR"
export PATH="$CARGO_HOME_DIR/bin:$PATH"

say() { printf '\n\033[1;35m%s\033[0m\n' "$*"; }
info() { printf '  %s\n' "$*"; }
warn() { printf '\033[1;33m  %s\033[0m\n' "$*" >&2; }
die() { printf '\033[1;31mخطا: %s\033[0m\n' "$*" >&2; exit 1; }
has() { command -v "$1" >/dev/null 2>&1; }

require_root() {
  [[ "${EUID:-$(id -u)}" -eq 0 ]] || die "این دستور را با sudo اجرا کنید: sudo bash donatello.sh $1"
}

require_systemd() {
  has systemctl || die "systemd روی این سیستم پیدا نشد."
  [[ -d /run/systemd/system ]] || die "systemd در حال اجرا نیست؛ نصب سرویس ممکن نیست."
}

install_os_dependencies() {
  [[ -r /etc/os-release ]] || die "فقط Debian و Ubuntu پشتیبانی می‌شوند."
  # shellcheck disable=SC1091
  source /etc/os-release
  case "${ID:-}" in
    debian|ubuntu) ;;
    *) die "توزیع ${PRETTY_NAME:-ناشناخته} پشتیبانی نمی‌شود؛ Debian یا Ubuntu لازم است." ;;
  esac
  has apt-get || die "apt-get روی این سیستم پیدا نشد."
  say "نصب پیش‌نیازهای ساخت"
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends ca-certificates curl git build-essential pkg-config
}

ensure_rust() {
  if [[ ! -x "$CARGO_HOME_DIR/bin/rustup" ]]; then
    say "نصب Rust stable"
    local installer
    installer="$(mktemp /tmp/donatello-rustup.XXXXXX)"
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
    say "دریافت آخرین نسخه از GitHub"
    git -C "$SOURCE_DIR" fetch --depth 1 origin "$BRANCH"
    git -C "$SOURCE_DIR" reset --hard FETCH_HEAD
  elif [[ -e "$SOURCE_DIR" ]]; then
    die "$SOURCE_DIR وجود دارد اما یک مخزن Git نیست؛ آن را بررسی کنید."
  else
    say "دریافت Donatello از GitHub"
    git clone --depth 1 --branch "$BRANCH" "$REPOSITORY" "$SOURCE_DIR"
  fi
}

build_release() {
  [[ -f "$SOURCE_DIR/frontend/dist/index.html" ]] || die "فایل‌های فرانت‌اند پیدا نشدند. مخزن یا شاخهٔ main را بررسی کنید."
  say "ساخت نسخهٔ release"
  (cd "$SOURCE_DIR" && cargo +stable build --locked --release)
  [[ -x "$SOURCE_DIR/target/release/donatello" ]] || die "ساخت باینری Donatello ناموفق بود."
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

configure_ufw_status() {
  local ufw sudoers_tmp
  ufw="$(command -v ufw || true)"
  if [[ -z "$ufw" ]]; then
    rm -f "$SUDOERS_FILE"
    return
  fi
  has visudo || die "ufw نصب است اما visudo پیدا نشد؛ مجوز محدود UFW را دستی تنظیم کنید."
  sudoers_tmp="$(mktemp /etc/sudoers.d/.donatello-ufw.XXXXXX)"
  printf '%s ALL=(root) NOPASSWD: %s status numbered\n' "$SERVICE_USER" "$ufw" > "$sudoers_tmp"
  chmod 0440 "$sudoers_tmp"
  if ! visudo -cf "$sudoers_tmp" >/dev/null; then
    rm -f "$sudoers_tmp"
    die "فایل مجوز فقط‌خواندنی UFW معتبر نیست."
  fi
  mv -f "$sudoers_tmp" "$SUDOERS_FILE"
}

write_default_config() {
  if [[ ! -f "$ENV_FILE" ]]; then
    install -o root -g "$SERVICE_GROUP" -m 0640 /dev/null "$ENV_FILE"
    printf 'DONATELLO_BIND=%s\nDONATELLO_DB=%s\n' "$DEFAULT_BIND" "$DEFAULT_DB" > "$ENV_FILE"
  fi
  chown root:"$SERVICE_GROUP" "$ENV_FILE"
  chmod 0640 "$ENV_FILE"
}

write_service_unit() {
  cat > "$SERVICE_FILE" <<'UNIT'
[Unit]
Description=Donatello VPS Management Panel
Documentation=https://github.com/parsasajjadi2008/donatello
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=donatello
Group=donatello
WorkingDirectory=/var/lib/donatello
EnvironmentFile=/etc/donatello/donatello.env
ExecStart=/usr/local/lib/donatello/donatello
Restart=on-failure
RestartSec=3
UMask=0027
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
ReadWritePaths=/var/lib/donatello
RestrictSUIDSGID=true
LockPersonality=true
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
UNIT
  chmod 0644 "$SERVICE_FILE"
  systemctl daemon-reload
}

activate_binary() {
  local built="$SOURCE_DIR/target/release/donatello"
  local staged
  install -d -o root -g root -m 0755 "$(dirname "$BINARY")"
  staged="$(mktemp /usr/local/lib/donatello/.donatello.XXXXXX)"
  install -o root -g root -m 0755 "$built" "$staged"
  mv -f "$staged" "$BINARY"
}

install_cli() {
  local staged
  staged="$(mktemp /usr/local/bin/.donatello-cli.XXXXXX)"
  curl --proto '=https' --tlsv1.2 -fsSL "$INSTALL_SCRIPT_URL" -o "$staged"
  chmod 0755 "$staged"
  chown root:root "$staged"
  mv -f "$staged" "$CLI"
  rm -f "$LEGACY_INSTALLER"
}

install_app() {
  require_root install
  require_systemd
  install_os_dependencies
  ensure_rust
  prepare_source
  build_release
  ensure_service_account
  configure_ufw_status
  write_default_config
  activate_binary
  write_service_unit
  install_cli
  systemctl enable "$SERVICE_NAME"
  systemctl restart "$SERVICE_NAME"
  if ! systemctl is-active --quiet "$SERVICE_NAME"; then
    systemctl --no-pager --full status "$SERVICE_NAME" || true
    journalctl -u "$SERVICE_NAME" -n 50 --no-pager || true
    die "سرویس بالا نیامد؛ گزارش بالا را بررسی کنید."
  fi
  say "نصب Donatello کامل شد"
  info "وضعیت سرویس: sudo donatello status"
  info "مدیریت: sudo donatello manage"
  info "آدرس پیش‌فرض پنل: http://127.0.0.1:8282"
  info "برای اتصال امن از SSH tunnel استفاده کنید؛ پنل به‌صورت پیش‌فرض عمومی نیست."
}

wait_until_active() {
  local attempt
  for attempt in {1..15}; do
    systemctl is-active --quiet "$SERVICE_NAME" && return 0
    sleep 1
  done
  return 1
}

update_app() {
  require_root update
  require_systemd
  [[ -f "$SERVICE_FILE" ]] || die "Donatello نصب نیست؛ ابتدا install را اجرا کنید."
  prepare_source
  build_release
  local previous staged
  previous="$(mktemp /var/tmp/donatello.previous.XXXXXX)"
  install -d -o root -g root -m 0755 "$(dirname "$BINARY")"
  staged="$(mktemp /usr/local/lib/donatello/.donatello.XXXXXX)"
  if [[ -x "$BINARY" ]]; then
    cp -p "$BINARY" "$previous"
  else
    rm -f "$previous"
  fi
  install -o root -g root -m 0755 "$SOURCE_DIR/target/release/donatello" "$staged"
  mv -f "$staged" "$BINARY"
  write_service_unit
  install_cli
  systemctl restart "$SERVICE_NAME"
  if wait_until_active; then
    rm -f "$previous"
    say "Donatello با موفقیت به‌روز شد"
    info "نسخهٔ منبع: $(git -C "$SOURCE_DIR" rev-parse --short HEAD)"
    return
  fi
  if [[ -s "$previous" ]]; then
    warn "نسخهٔ جدید بالا نیامد؛ نسخهٔ قبلی بازگردانده می‌شود."
    install -o root -g root -m 0755 "$previous" "$BINARY"
    systemctl restart "$SERVICE_NAME" || true
  fi
  rm -f "$previous"
  journalctl -u "$SERVICE_NAME" -n 50 --no-pager || true
  die "به‌روزرسانی فعال نشد."
}

show_status() {
  require_systemd
  if [[ -f "$SERVICE_FILE" ]]; then
    systemctl --no-pager --full status "$SERVICE_NAME" || true
  else
    info "Donatello نصب نیست."
  fi
}

change_port() {
  require_root manage
  [[ -f "$ENV_FILE" ]] || die "تنظیمات پیدا نشد؛ Donatello نصب نیست."
  local port
  read -r -p "پورت جدید پنل (۱ تا ۶۵۵۳۵): " port
  [[ "$port" =~ ^[0-9]{1,5}$ ]] && (( port >= 1 && port <= 65535 )) || die "پورت نامعتبر است."
  local db_path
  db_path="$(awk -F= '$1 == "DONATELLO_DB" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE")"
  [[ -n "$db_path" ]] || db_path="$DEFAULT_DB"
  local temp
  temp="$(mktemp /etc/donatello/.donatello.env.XXXXXX)"
  printf 'DONATELLO_BIND=127.0.0.1:%s\nDONATELLO_DB=%s\n' "$port" "$db_path" > "$temp"
  chown root:"$SERVICE_GROUP" "$temp"
  chmod 0640 "$temp"
  mv -f "$temp" "$ENV_FILE"
  systemctl restart "$SERVICE_NAME"
  wait_until_active || die "سرویس پس از تغییر پورت بالا نیامد."
  say "پورت پنل به 127.0.0.1:$port تغییر کرد."
}

manage_app() {
  require_root manage
  require_systemd
  if [[ "${2:-}" != "" ]]; then
    case "$2" in
      status) show_status ;;
      start|stop|restart)
        [[ -f "$SERVICE_FILE" ]] || die "Donatello نصب نیست."
        systemctl "$2" "$SERVICE_NAME"
        ;;
      logs) journalctl -u "$SERVICE_NAME" -n "${3:-100}" --no-pager ;;
      port) change_port ;;
      update) update_app ;;
      delete) delete_app "${3:-}" ;;
      *) die "گزینهٔ مدیریت ناشناخته است: $2" ;;
    esac
    return
  fi
  while true; do
    printf '\n\033[1;35mDonatello · مدیریت سرویس\033[0m\n'
    printf '  1) وضعیت سرویس\n  2) شروع\n  3) توقف\n  4) راه‌اندازی دوباره\n  5) مشاهدهٔ لاگ\n  6) تغییر پورت پنل\n  7) به‌روزرسانی\n  8) حذف\n  0) خروج\n'
    read -r -p 'انتخاب: ' selection
    case "$selection" in
      1) show_status ;;
      2) systemctl start "$SERVICE_NAME" ;;
      3) systemctl stop "$SERVICE_NAME" ;;
      4) systemctl restart "$SERVICE_NAME" ;;
      5) journalctl -u "$SERVICE_NAME" -n 100 --no-pager ;;
      6) change_port ;;
      7) update_app ;;
      8) delete_app ""; [[ ! -f "$SERVICE_FILE" ]] && return ;;
      0) return ;;
      *) warn "گزینهٔ نامعتبر است." ;;
    esac
  done
}

delete_app() {
  require_root delete
  require_systemd
  local purge="${1:-}"
  if [[ "$purge" == "--purge" ]]; then
    warn "این کار سرویس، سورس، تنظیمات و همهٔ داده‌های Donatello را پاک می‌کند."
    read -r -p 'برای تأیید دقیقاً PURGE DONATELLO را وارد کنید: ' confirmation
    [[ "$confirmation" == "PURGE DONATELLO" ]] || die "پاک‌سازی لغو شد."
  else
    warn "سرویس و باینری حذف می‌شوند؛ پایگاه داده و تنظیمات در /var/lib/donatello و /etc/donatello باقی می‌مانند."
    read -r -p 'برای تأیید دقیقاً DELETE DONATELLO را وارد کنید: ' confirmation
    [[ "$confirmation" == "DELETE DONATELLO" ]] || die "حذف لغو شد."
  fi
  systemctl disable --now "$SERVICE_NAME" 2>/dev/null || true
  rm -f "$SERVICE_FILE" "$BINARY" "$CLI" "$LEGACY_INSTALLER" "$SUDOERS_FILE"
  rmdir /usr/local/lib/donatello 2>/dev/null || true
  rm -rf "$SOURCE_DIR"
  systemctl daemon-reload
  systemctl reset-failed "$SERVICE_NAME" 2>/dev/null || true
  if [[ "$purge" == "--purge" ]]; then
    rm -rf "$INSTALL_DIR" "$DATA_DIR" "$ENV_DIR"
    userdel "$SERVICE_USER" 2>/dev/null || true
    groupdel "$SERVICE_GROUP" 2>/dev/null || true
    say "Donatello و همهٔ داده‌هایش پاک شدند."
  else
    say "Donatello حذف شد؛ داده‌ها و تنظیمات برای نصب دوباره نگه داشته شدند."
  fi
}

usage() {
  cat <<'HELP'
Donatello · installer and system service manager

Usage:
  sudo bash donatello.sh install
  sudo donatello manage [status|start|stop|restart|logs|port|update|delete]
  sudo donatello update
  sudo donatello delete [--purge]

Commands:
  install          Build Donatello and install its systemd service
  manage           Open the interactive service manager
  update           Fetch main, build, replace the binary, and roll back on failure
  delete           Remove the service and binary after typed confirmation
  delete --purge   Also remove configuration, database, and service account

Supported systems: Debian and Ubuntu with systemd.
HELP
}

case "${1:-help}" in
  install) install_app ;;
  manage) manage_app "$@" ;;
  status) show_status ;;
  start|stop|restart)
    require_root "$1"
    require_systemd
    [[ -f "$SERVICE_FILE" ]] || die "Donatello نصب نیست."
    systemctl "$1" "$SERVICE_NAME"
    ;;
  logs)
    require_root logs
    require_systemd
    journalctl -u "$SERVICE_NAME" -n "${2:-100}" --no-pager
    ;;
  update) update_app ;;
  delete) delete_app "${2:-}" ;;
  help|-h|--help) usage ;;
  *) usage >&2; exit 2 ;;
esac
