#!/usr/bin/env bash
set -Eeuo pipefail

readonly APP_NAME="DONATELLO"
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
readonly DOMAIN_FILE="/etc/donatello/https-domain"
readonly NGINX_SITE="/etc/nginx/sites-available/donatello"
readonly NGINX_SITE_LINK="/etc/nginx/sites-enabled/donatello"
readonly DATA_DIR="/var/lib/donatello"
readonly SERVICE_USER="donatello"
readonly SERVICE_GROUP="donatello"
readonly DEFAULT_BIND="127.0.0.1:8282"
readonly DEFAULT_DB="/var/lib/donatello/donatello.db"
SERVER_PUBLIC_IP=""

readonly RUST_HOME="${RUSTUP_HOME:-/root/.rustup}"
readonly CARGO_HOME_DIR="${CARGO_HOME:-/root/.cargo}"
export RUSTUP_HOME="$RUST_HOME"
export CARGO_HOME="$CARGO_HOME_DIR"
export PATH="$CARGO_HOME_DIR/bin:$PATH"

if [[ -t 1 && -z "${NO_COLOR:-}" && "${TERM:-}" != "dumb" ]]; then
  readonly C_RESET=$'\033[0m'
  readonly C_BOLD=$'\033[1m'
  readonly C_PURPLE=$'\033[1;35m'
  readonly C_CYAN=$'\033[1;36m'
  readonly C_GREEN=$'\033[1;32m'
  readonly C_YELLOW=$'\033[1;33m'
  readonly C_RED=$'\033[1;31m'
  readonly C_DIM=$'\033[2m'
else
  readonly C_RESET=''
  readonly C_BOLD=''
  readonly C_PURPLE=''
  readonly C_CYAN=''
  readonly C_GREEN=''
  readonly C_YELLOW=''
  readonly C_RED=''
  readonly C_DIM=''
fi

say() { printf '\n%s%s%s\n' "$C_PURPLE" "$*" "$C_RESET"; }
info() { printf '  %s\n' "$*"; }
warn() { printf '%s  WARNING: %s%s\n' "$C_YELLOW" "$*" "$C_RESET" >&2; }
die() { printf '%s  ERROR: %s%s\n' "$C_RED" "$*" "$C_RESET" >&2; exit 1; }
has() { command -v "$1" >/dev/null 2>&1; }

show_banner() {
  printf '%s' "$C_PURPLE"
  cat <<'BANNER'
  ██████╗  ██████╗ ███╗   ██╗ █████╗ ████████╗███████╗██╗     ██╗      ██████╗
  ██╔══██╗██╔═══██╗████╗  ██║██╔══██╗╚══██╔══╝██╔════╝██║     ██║     ██╔═══██╗
  ██║  ██║██║   ██║██╔██╗ ██║███████║   ██║   █████╗  ██║     ██║     ██║   ██║
  ██║  ██║██║   ██║██║╚██╗██║██╔══██║   ██║   ██╔══╝  ██║     ██║     ██║   ██║
  ██████╔╝╚██████╔╝██║ ╚████║██║  ██║   ██║   ███████╗███████╗███████╗╚██████╔╝
  ╚═════╝  ╚═════╝ ╚═╝  ╚═══╝╚═╝  ╚═╝   ╚═╝   ╚══════╝╚══════╝╚══════╝ ╚═════╝
BANNER
  printf '%s\n' "$C_RESET"
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
  local prompt="$1" variable="$2" default="${3:-}" answer=""
  if [[ -r /dev/tty ]]; then
    IFS= read -r -p "$prompt" answer </dev/tty || true
  else
    IFS= read -r -p "$prompt" answer || true
  fi
  printf -v "$variable" '%s' "${answer:-$default}"
}

require_root() {
  [[ "${EUID:-$(id -u)}" -eq 0 ]] && return 0
  if [[ "$1" == "install" ]]; then
    die "Run the installer as root: sudo bash install.sh install"
  fi
  die "Run this command as root: sudo donatello $1"
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
  apt-get install -y --no-install-recommends ca-certificates curl git build-essential pkg-config
}

ensure_rust() {
  if [[ ! -x "$CARGO_HOME_DIR/bin/rustup" ]]; then
    say "Installing the Rust stable toolchain"
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
    say "Fetching the latest source from GitHub"
    git -C "$SOURCE_DIR" fetch --depth 1 origin "$BRANCH"
    git -C "$SOURCE_DIR" reset --hard FETCH_HEAD
  elif [[ -e "$SOURCE_DIR" ]]; then
    die "$SOURCE_DIR exists but is not a Git repository. Please inspect it before continuing."
  else
    say "Downloading Donatello source from GitHub"
    git clone --depth 1 --branch "$BRANCH" "$REPOSITORY" "$SOURCE_DIR"
  fi
}

build_release() {
  [[ -f "$SOURCE_DIR/frontend/dist/index.html" ]] || die "Built frontend files are missing. Check the repository and main branch."
  say "Building the optimized release"
  (cd "$SOURCE_DIR" && cargo +stable build --locked --release)
  [[ -x "$SOURCE_DIR/target/release/donatello" ]] || die "The Donatello binary was not created."
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
  has visudo || die "UFW is installed but visudo was not found. Configure the restricted UFW permission manually."
  sudoers_tmp="$(mktemp /etc/sudoers.d/.donatello-ufw.XXXXXX)"
  printf '%s ALL=(root) NOPASSWD: %s status numbered\n' "$SERVICE_USER" "$ufw" > "$sudoers_tmp"
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
    printf 'DONATELLO_BIND=%s\nDONATELLO_DB=%s\n' "$DEFAULT_BIND" "$db_path" > "$ENV_FILE"
  fi
  chown root:"$SERVICE_GROUP" "$ENV_FILE"
  chmod 0640 "$ENV_FILE"
}

configured_db_path() {
  local db_path=""
  if [[ -f "$ENV_FILE" ]]; then
    db_path="$(awk -F= '$1 == "DONATELLO_DB" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE")"
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
  for file in "$db_path" "$db_path-wal" "$db_path-shm"; do
    [[ -f "$file" ]] || continue
    chown "$SERVICE_USER:$SERVICE_GROUP" "$file"
    chmod 0600 "$file"
  done
}

panel_port() {
  local bind
  bind="$(awk -F= '$1 == "DONATELLO_BIND" { sub(/^[^=]*=/, ""); print; exit }' "$ENV_FILE" 2>/dev/null || true)"
  bind="${bind:-$DEFAULT_BIND}"
  printf '%s\n' "${bind##*:}"
}

detect_public_ipv4() {
  local address
  address="$(curl -4 --connect-timeout 3 --max-time 6 -fsS https://api.ipify.org 2>/dev/null || true)"
  [[ "$address" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]] || return 1
  printf '%s\n' "$address"
}

write_bind_config() {
  local bind="$1" db_path temp
  db_path="$(configured_db_path)"
  temp="$(mktemp /etc/donatello/.donatello.env.XXXXXX)"
  printf 'DONATELLO_BIND=%s\nDONATELLO_DB=%s\n' "$bind" "$db_path" > "$temp"
  chown root:"$SERVICE_GROUP" "$temp"
  chmod 0640 "$temp"
  mv -f "$temp" "$ENV_FILE"
}

install_https_dependencies() {
  say "Installing Nginx and Let's Encrypt tools"
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends nginx certbot python3-certbot-nginx
}

write_nginx_proxy() {
  local domain="$1" port="$2" temp
  [[ ! -e "$NGINX_SITE" || -f "$DOMAIN_FILE" ]] || die "$NGINX_SITE already exists and is not managed by DONATELLO."
  install -d -o root -g root -m 0755 /etc/nginx/sites-available /etc/nginx/sites-enabled
  temp="$(mktemp /etc/nginx/sites-available/.donatello.XXXXXX)"
  cat > "$temp" <<NGINX
server {
    listen 80;
    listen [::]:80;
    server_name $domain;

    location / {
        proxy_pass http://127.0.0.1:$port;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
    }
}
NGINX
  chmod 0644 "$temp"
  mv -f "$temp" "$NGINX_SITE"
  ln -sfn "$NGINX_SITE" "$NGINX_SITE_LINK"
  nginx -t
  systemctl enable --now nginx
  systemctl reload nginx
}

configure_https() {
  require_root ssl
  require_systemd
  [[ -f "$SERVICE_FILE" ]] || die "DONATELLO is not installed. Install it before configuring HTTPS."

  local address confirmed domain email dns_matches port existing_domain
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
    info "Panel URL: https://$existing_domain"
    info "Certificate renewal is managed by Certbot."
    return 0
  fi

  read_answer 'Domain name pointed to this server (leave blank to skip): ' domain
  [[ -n "$domain" ]] || {
    info "HTTPS setup skipped. The panel remains private on localhost."
    return 0
  }
  [[ "$domain" =~ ^([A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)+[A-Za-z]{2,63}$ ]] || die "Enter a valid fully qualified domain name."

  dns_matches="$(getent ahostsv4 "$domain" | awk '{print $1}' | sort -u || true)"
  if [[ -z "$dns_matches" || ( -n "$address" && " $dns_matches " != *" $address "* ) ]]; then
    warn "The domain does not currently resolve to the detected server IP."
    info "Domain addresses: ${dns_matches:-none}"
    [[ -z "$address" ]] || info "Detected server IP: $address"
    read_answer 'Continue and let certificate validation decide? [y/N]: ' confirmed 'n'
    [[ "${confirmed,,}" == "y" || "${confirmed,,}" == "yes" ]] || return 0
  fi

  read_answer 'Email address for certificate expiry notices: ' email
  [[ "$email" == *@*.* ]] || die "Enter a valid email address for the certificate."
  port="$(panel_port)"

  install_https_dependencies
  write_nginx_proxy "$domain" "$port"
  if certbot --nginx --non-interactive --agree-tos --redirect --email "$email" -d "$domain"; then
    printf '%s\n' "$domain" > "$DOMAIN_FILE"
    chown root:"$SERVICE_GROUP" "$DOMAIN_FILE"
    chmod 0640 "$DOMAIN_FILE"
    if systemctl list-unit-files certbot.timer --no-legend 2>/dev/null | rg -q '^certbot\.timer'; then
      systemctl enable --now certbot.timer
    fi
    say "HTTPS is active"
    info "Panel URL: https://$domain"
    info "Automatic renewal: certbot.timer"
    warn "Make sure inbound TCP ports 80 and 443 are allowed by your provider firewall."
  else
    rm -f "$NGINX_SITE_LINK" "$NGINX_SITE"
    nginx -t >/dev/null 2>&1 && systemctl reload nginx || true
    warn "Certificate issuance failed. The panel remains available only through localhost or an SSH tunnel."
  fi
}

show_access_summary() {
  local address domain port
  address="$SERVER_PUBLIC_IP"
  domain="$(cat "$DOMAIN_FILE" 2>/dev/null || true)"
  port="$(panel_port)"
  say "DONATELLO IS READY"
  if [[ -n "$address" ]]; then
    info "Server IP: $address"
  else
    info "Public IP was not confirmed. Use your server IP in the SSH command below."
  fi
  if [[ -n "$domain" ]]; then
    printf '\n  %sPANEL LOGIN%s\n  %shttps://%s%s\n\n' "$C_CYAN" "$C_RESET" "$C_BOLD" "$domain" "$C_RESET"
  else
    printf '\n  %sPANEL LOGIN%s  (open after connecting the SSH tunnel)\n' "$C_CYAN" "$C_RESET"
    printf '  %shttp://127.0.0.1:%s%s\n\n' "$C_BOLD" "$port" "$C_RESET"
    if [[ -n "$address" ]]; then
      printf '  From your computer, run:\n  %sssh -N -L %s:127.0.0.1:%s <SSH_USER>@%s%s\n' \
        "$C_GREEN" "$port" "$port" "$address" "$C_RESET"
    else
      printf '  From your computer, run:\n  %sssh -N -L %s:127.0.0.1:%s <SSH_USER>@<SERVER_IP>%s\n' \
        "$C_GREEN" "$port" "$port" "$C_RESET"
    fi
    info "Replace <SSH_USER> with your server login, then open the panel link above."
    info "Enable HTTPS with a domain: sudo donatello ssl"
  fi
  printf '\n  %sMANAGEMENT MENU%s\n  Run: %sdonatello%s\n' "$C_CYAN" "$C_RESET" "$C_BOLD" "$C_RESET"
  info "From a non-root account, use: sudo donatello"
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
  take_database_ownership
  activate_binary
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
  say "DONATELLO is installed and running"
  info "Service status: sudo donatello status"
  configure_https
  show_access_summary
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
  [[ -f "$SERVICE_FILE" ]] || die "DONATELLO is not installed. Run 'donatello install' first."
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
    say "DONATELLO was updated successfully"
    info "Source revision: $(git -C "$SOURCE_DIR" rev-parse --short HEAD)"
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
    info "DONATELLO is not installed."
  fi
}

change_port() {
  require_root manage
  [[ -f "$ENV_FILE" ]] || die "Configuration was not found. DONATELLO is not installed."
  local port
  read -r -p "New panel port (1-65535): " port
  [[ "$port" =~ ^[0-9]{1,5}$ ]] && (( port >= 1 && port <= 65535 )) || die "Invalid port. Enter a number from 1 to 65535."
  write_bind_config "127.0.0.1:$port"
  systemctl restart "$SERVICE_NAME"
  wait_until_active || die "The service did not start after changing the port."
  wait_for_panel || die "The service is running but the web panel did not answer on the new port."
  local domain
  domain="$(cat "$DOMAIN_FILE" 2>/dev/null || true)"
  if [[ -n "$domain" && -f "$NGINX_SITE" ]]; then
    sed -E -i "s#proxy_pass http://127\\.0\\.0\\.1:[0-9]+;#proxy_pass http://127.0.0.1:$port;#" "$NGINX_SITE"
    nginx -t && systemctl reload nginx || die "The panel port changed, but the HTTPS proxy could not be updated."
  fi
  say "The panel port is now 127.0.0.1:$port"
}

manage_app() {
  require_root manage
  if [[ "${2:-}" != "" ]]; then
    require_systemd
    case "$2" in
      status) show_status ;;
      start|stop|restart)
        [[ -f "$SERVICE_FILE" ]] || die "DONATELLO is not installed."
        systemctl "$2" "$SERVICE_NAME"
        ;;
      logs) journalctl -u "$SERVICE_NAME" -n "${3:-100}" --no-pager ;;
      port) change_port ;;
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
  [[ -f "$SERVICE_FILE" ]] || die "DONATELLO is not installed."
  systemctl "$action" "$SERVICE_NAME"
}

view_logs() {
  local lines="${1:-100}"
  [[ "$lines" =~ ^[0-9]+$ ]] || die "Log line count must be a positive number."
  require_root logs
  require_systemd
  journalctl -u "$SERVICE_NAME" -n "$lines" --no-pager
}

main_menu() {
  local selection
  while true; do
    printf '\n%s%s  DONATELLO CONTROL CENTER%s\n' "$C_BOLD" "$C_CYAN" "$C_RESET"
    if [[ -f "$SERVICE_FILE" ]]; then
      printf '  %s● Installed%s\n\n' "$C_GREEN" "$C_RESET"
    else
      printf '  %s○ Not installed%s\n\n' "$C_DIM" "$C_RESET"
    fi
    printf '  %sINSTALLATION%s\n' "$C_DIM" "$C_RESET"
    printf '   1) Install / repair DONATELLO\n\n'
    printf '  %sSERVICE MANAGEMENT%s\n' "$C_DIM" "$C_RESET"
    printf '   2) Service status       3) Start service\n'
    printf '   4) Stop service         5) Restart service\n'
    printf '   6) View service logs    7) Change panel port\n\n'
    printf '  %sMAINTENANCE%s\n' "$C_DIM" "$C_RESET"
    printf '   8) Update DONATELLO     9) Configure HTTPS / SSL\n'
    printf '  10) Uninstall (keep data)\n'
    printf '  11) Uninstall and purge all data\n\n'
    printf '   H) Help                 0) Exit\n\n'
    read -r -p 'Select an option: ' selection || return 0
    case "$selection" in
      1) install_app; pause_menu ;;
      2) show_status; pause_menu ;;
      3) run_service_action start; pause_menu ;;
      4) run_service_action stop; pause_menu ;;
      5) run_service_action restart; pause_menu ;;
      6) view_logs 100; pause_menu ;;
      7) change_port; pause_menu ;;
      8) update_app; pause_menu ;;
      9) configure_https; pause_menu ;;
      10) delete_app ""; pause_menu ;;
      11) delete_app "--purge"; pause_menu ;;
      h|H) usage; pause_menu ;;
      0) return 0 ;;
      *) warn "Invalid selection. Choose an option from the menu."; pause_menu ;;
    esac
  done
}

delete_app() {
  require_root delete
  require_systemd
  local purge="${1:-}"
  local keep_database="n" db_path entry answer domain
  if [[ "$purge" == "--purge" ]]; then
    warn "This removes the service, installer, source, configuration, and database."
    read_answer 'Type PURGE DONATELLO to confirm: ' confirmation
    [[ "$confirmation" == "PURGE DONATELLO" ]] || die "Purge cancelled."
  else
    warn "DONATELLO services, binaries, source, system account, and configuration will be removed."
    read_answer 'Keep the database for the next installation? [Y/n]: ' answer 'y'
    case "${answer,,}" in
      n|no) keep_database="n" ;;
      y|yes) keep_database="y" ;;
      *) die "Choose Y to keep the database or N to remove it." ;;
    esac
    read_answer 'Type DELETE DONATELLO to confirm: ' confirmation
    [[ "$confirmation" == "DELETE DONATELLO" ]] || die "Uninstall cancelled."
  fi
  db_path="$(configured_db_path)"
  domain="$(cat "$DOMAIN_FILE" 2>/dev/null || true)"
  if [[ "$keep_database" == "y" ]]; then
    install -d -o root -g root -m 0750 "$DATA_DIR"
    printf '%s\n' "$db_path" > "$DATA_DIR/.database-path"
    chmod 0600 "$DATA_DIR/.database-path"
  fi
  systemctl disable --now "$SERVICE_NAME" 2>/dev/null || true
  rm -f "$SERVICE_FILE" "$BINARY" "$CLI" "$LEGACY_INSTALLER" "$SUDOERS_FILE"
  if [[ -n "$domain" ]]; then
    rm -f "$NGINX_SITE_LINK" "$NGINX_SITE"
    if has certbot; then
      certbot delete --cert-name "$domain" --non-interactive >/dev/null 2>&1 || true
    fi
    if has nginx; then
      nginx -t >/dev/null 2>&1 && systemctl reload nginx 2>/dev/null || true
    fi
  fi
  rmdir /usr/local/lib/donatello 2>/dev/null || true
  rm -rf "$SOURCE_DIR"
  rmdir "$INSTALL_DIR" 2>/dev/null || true
  rm -rf "$ENV_DIR"
  systemctl daemon-reload
  systemctl reset-failed "$SERVICE_NAME" 2>/dev/null || true
  userdel "$SERVICE_USER" 2>/dev/null || true
  groupdel "$SERVICE_GROUP" 2>/dev/null || true
  if [[ "$purge" == "--purge" || "$keep_database" != "y" ]]; then
    rm -rf "$DATA_DIR"
    if [[ "$purge" == "--purge" ]]; then
      say "DONATELLO and all application data have been removed."
    else
      say "DONATELLO has been uninstalled and its database was removed."
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
    say "DONATELLO has been uninstalled. The database is kept for the next installation."
    info "Retained database: $db_path"
  fi
}

usage() {
  cat <<'HELP'
DONATELLO · Installer and System Service Manager

Usage:
  sudo donatello                  Open the interactive control center
  sudo donatello menu             Open the interactive control center
  sudo bash install.sh             Open the interactive installer menu
  sudo bash install.sh install     Install DONATELLO
  sudo donatello manage [status|start|stop|restart|logs|port|ssl|update|delete]
  sudo donatello ssl
  sudo donatello update
  sudo donatello delete [--purge]

Commands:
  install          Build Donatello and install its systemd service
  manage           Open the interactive control center
  status           Show service status
  start|stop       Start or stop the systemd service
  restart          Restart the systemd service
  logs [count]     Show recent service logs
  update           Fetch main, build, replace the binary, and roll back on failure
  ssl              Configure HTTPS with Nginx and Let's Encrypt
  delete           Uninstall the service while preserving application data
  delete --purge   Uninstall and remove all configuration and application data

Supported systems: Debian and Ubuntu with systemd.
HELP
}

show_banner

case "${1:-menu}" in
  install) install_app ;;
  manage) manage_app "$@" ;;
  menu) main_menu ;;
  status) show_status ;;
  start|stop|restart)
    require_root "$1"
    require_systemd
    [[ -f "$SERVICE_FILE" ]] || die "DONATELLO is not installed."
    systemctl "$1" "$SERVICE_NAME"
    ;;
  logs)
    view_logs "${2:-100}"
    ;;
  update) update_app ;;
  ssl) configure_https ;;
  delete) delete_app "${2:-}" ;;
  help|-h|--help) usage ;;
  *) usage >&2; exit 2 ;;
esac
