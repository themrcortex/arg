#!/usr/bin/env bash
set -Eeuo pipefail

readonly source_dir="${ARG_SOURCE_DIR:-/opt/arg/source}"
readonly repository="${ARG_REPOSITORY:-https://github.com/themrcortex/arg.git}"
readonly update_lock_dir="${ARG_UPDATE_LOCK_DIR:-/run/arg-update.lock}"
readonly update_status_file="${ARG_UPDATE_STATUS_FILE:-/run/arg-update-in-progress}"
readonly update_state_file="${ARG_UPDATE_STATE_FILE:-/run/arg-update-state.json}"
readonly update_last_unit_file="${ARG_UPDATE_LAST_UNIT_FILE:-/run/arg-update-last-unit}"
readonly update_cancel_file="${ARG_UPDATE_CANCEL_FILE:-/run/arg-update-cancel-requested}"
readonly rollback_dir="${ARG_ROLLBACK_DIR:-/var/lib/arg/rollback}"
readonly arg_binary_dir="${ARG_UPDATE_BINARY_DIR:-/usr/local/lib/arg}"
readonly arg_binary="${ARG_UPDATE_BINARY:-$arg_binary_dir/arg}"
readonly database_backup_tool="${ARG_DATABASE_BACKUP_TOOL:-$arg_binary_dir/arg-db-backup}"
readonly environment_file="${ARG_UPDATE_ENV_FILE:-/etc/arg/arg.env}"
readonly rollback_binary="$rollback_dir/arg.previous"
readonly rollback_database="$rollback_dir/arg.db.previous"

valid_branch() {
  [[ "$1" == main || "$1" == development ]]
}

update_job_active() {
  local unit owner owner_start actual_start command_line
  unit="$(cat "$update_last_unit_file" 2>/dev/null || true)"
  if [[ "$unit" =~ ^arg-web-update-[0-9]+$ ]] && { systemctl is-active --quiet "$unit.service" || systemctl is-active --quiet "$unit.timer"; }; then
    return 0
  fi
  owner="$(awk 'NR==1 {print $2}' "$update_status_file" 2>/dev/null || true)"
  [[ "$owner" =~ ^[0-9]+$ && -r "/proc/$owner/cmdline" ]] || return 1
  owner_start="$(awk 'NR==1 {print $3}' "$update_status_file" 2>/dev/null || true)"
  actual_start="$(awk '{print $22}' "/proc/$owner/stat" 2>/dev/null || true)"
  if [[ "$owner_start" =~ ^[0-9]+$ ]]; then
    [[ "$actual_start" == "$owner_start" ]] && kill -0 "$owner" 2>/dev/null
    return $?
  fi
  command_line="$(tr '\0' ' ' <"/proc/$owner/cmdline" 2>/dev/null || true)"
  [[ "$command_line" == *"/usr/local/bin/arg"* && "$command_line" == *"update"* ]] || return 1
  kill -0 "$owner" 2>/dev/null
}

clear_stale_update_lock() {
  [[ -d "$update_lock_dir" ]] || return 0
  local started
  update_job_active && return 0
  started="$(awk 'NR==1 {print $1}' "$update_status_file" 2>/dev/null || true)"
  rm -f "$update_status_file" "$update_cancel_file"
  rmdir "$update_lock_dir" 2>/dev/null || true
  export ARG_UPDATE_JOB_ID="$(sed -n 's/.*\"job_id\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
  write_update_state failed failed "The stale update lock was cleared because no updater process or systemd job is active."
}

rollback_health_check() {
  local bind port scheme attempt
  bind="$(awk -F= '$1 == "ARG_TLS_BIND" {tls=substr($0,index($0,"=")+1)} $1 == "ARG_BIND" {http=substr($0,index($0,"=")+1)} END {print tls != "" ? tls : http}' "$environment_file" 2>/dev/null)"
  [[ -n "$bind" ]] || bind="0.0.0.0:8282"
  port="${bind##*:}"
  scheme=http
  grep -q '^ARG_TLS_BIND=' "$environment_file" 2>/dev/null && scheme=https
  for attempt in {1..20}; do
    systemctl is-active --quiet arg.service && curl -k --connect-timeout 1 --max-time 2 -fsS "$scheme://127.0.0.1:$port/api/state" -o /dev/null && return 0
    sleep 1
  done
  return 1
}

install_rollback_binary() {
  [[ -x "$rollback_binary" ]] || { echo "No previous ARG binary is available for rollback." >&2; return 1; }
  install -d -o root -g root -m 0700 "$arg_binary_dir"
  local staged
  staged="$(mktemp "$arg_binary_dir/.arg-rollback.XXXXXX")"
  install -o root -g root -m 0755 "$rollback_binary" "$staged"
  mv -f "$staged" "$arg_binary"
}

restore_database_snapshot() {
  local snapshot="$1"
  [[ -s "$snapshot" ]] || return 0
  local database_path staged
  database_path="$(awk -F= '$1 == "ARG_DB" {sub(/^[^=]*=/, ""); print; exit}' "$environment_file" 2>/dev/null || true)"
  [[ -n "$database_path" ]] || database_path="/var/lib/arg/arg.db"
  [[ "$database_path" == /* ]] || database_path="/var/lib/arg/$database_path"
  staged="$(mktemp "${database_path}.rollback.XXXXXX")"
  cp -p "$snapshot" "$staged"
  chown arg:arg "$staged"
  chmod 0600 "$staged"
  mv -f "$staged" "$database_path"
  rm -f "$database_path-wal" "$database_path-shm"
}

restore_previous_database() { restore_database_snapshot "$rollback_database"; }

update_process_tree() {
  local pid="$1" child
  printf '%s\n' "$pid"
  while read -r child; do
    [[ "$child" =~ ^[0-9]+$ ]] || continue
    update_process_tree "$child"
  done < <(ps -o pid= --ppid "$pid" 2>/dev/null || true)
}

cancel_update() {
  [[ -d "$update_lock_dir" && -s "$update_status_file" ]] || { echo "No active ARG update was found."; return 1; }
  local unit phase owner owner_start actual_start current_command process_tree pid rollback_ok=1
  unit="$(cat "$update_last_unit_file" 2>/dev/null || true)"
  phase="$(sed -n 's/.*\"phase\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
  owner="$(awk 'NR==1 {print $2}' "$update_status_file" 2>/dev/null || true)"
  owner_start="$(awk 'NR==1 {print $3}' "$update_status_file" 2>/dev/null || true)"
  export ARG_UPDATE_JOB_ID="$(sed -n 's/.*\"job_id\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
  : > "$update_cancel_file"
  write_update_state running cancelling "Cancellation requested; stopping the updater safely."
  if [[ "$unit" =~ ^arg-web-update-[0-9]+$ ]]; then
    systemctl stop "$unit.timer" "$unit.service" >/dev/null 2>&1 || true
  elif [[ "$owner" =~ ^[0-9]+$ && "$owner" != "$$" && -r "/proc/$owner/cmdline" ]]; then
    current_command="$(tr '\0' ' ' <"/proc/$owner/cmdline" 2>/dev/null || true)"
    actual_start="$(awk '{print $22}' "/proc/$owner/stat" 2>/dev/null || true)"
    if { [[ "$owner_start" =~ ^[0-9]+$ && "$actual_start" == "$owner_start" ]] || [[ "$current_command" == *"/usr/local/bin/arg"* && "$current_command" == *"update"* ]]; }; then
      process_tree="$(update_process_tree "$owner" | sort -rn)"
      while read -r pid; do [[ "$pid" =~ ^[0-9]+$ ]] && kill -TERM "$pid" 2>/dev/null || true; done <<<"$process_tree"
      sleep 2
      while read -r pid; do [[ "$pid" =~ ^[0-9]+$ ]] && kill -KILL "$pid" 2>/dev/null || true; done <<<"$process_tree"
    fi
  fi
  if [[ "$phase" == installing || "$phase" == restarting || "$phase" == verifying ]]; then
    systemctl stop arg.service 2>/dev/null || true
    restore_previous_database || { echo "Warning: the previous SQLite snapshot could not be restored; the database was left unchanged." >&2; rollback_ok=0; }
    install_rollback_binary || { echo "Update cancelled, but automatic binary restore failed." >&2; rollback_ok=0; }
    systemctl start arg.service || true
    rollback_health_check || { echo "Previous binary restored, but ARG did not pass its health check." >&2; rollback_ok=0; }
  fi
  rm -f "$update_status_file" "$update_cancel_file"
  rmdir "$update_lock_dir" 2>/dev/null || true
  if (( rollback_ok )); then
    write_update_state cancelled cancelled "The update was cancelled. The previous ARG version is running."
  else
    write_update_state failed failed "The update was cancelled, but automatic recovery needs attention. Check ARG service logs."
  fi
  (( rollback_ok )) || return 1
  echo "ARG update cancelled."
}

rollback_update() {
  local current staged phase database_path current_database=""
  if [[ -d "$update_lock_dir" ]]; then
    phase="$(sed -n 's/.*\"phase\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
    cancel_update || return 1
    if [[ "$phase" != installing && "$phase" != restarting && "$phase" != verifying ]]; then
      echo "The active update was cancelled before swapping binaries; the installed version was retained. Run rollback again after cancellation completes to restore the previous release." >&2
      return 1
    fi
    echo "The active update was cancelled during installation; the previous binary was restored."
    return 0
  fi
  [[ -x "$rollback_binary" ]] || { echo "No previous ARG version is available yet." >&2; return 1; }
  if [[ -s "$rollback_database" ]]; then
    [[ -x "$database_backup_tool" ]] || { echo "The database backup tool is unavailable; rollback was not started." >&2; return 1; }
    database_path="$(awk -F= '$1 == "ARG_DB" {sub(/^[^=]*=/, ""); print; exit}' "$environment_file" 2>/dev/null || true)"
    [[ -n "$database_path" ]] || database_path="/var/lib/arg/arg.db"
    [[ "$database_path" == /* ]] || database_path="/var/lib/arg/$database_path"
    current_database="$(mktemp "$rollback_dir/arg.db.current.XXXXXX")"
    rm -f "$current_database"
    if ! "$database_backup_tool" database-backup "$database_path" "$current_database"; then
      rm -f "$current_database"
      echo "Could not snapshot the current database; rollback was not started." >&2
      return 1
    fi
  fi
  current="$(mktemp "$rollback_dir/arg.current.XXXXXX")"
  cp -p "$arg_binary" "$current"
  staged="$(mktemp "$arg_binary_dir/.arg-rollback.XXXXXX")"
  install -o root -g root -m 0755 "$rollback_binary" "$staged"
  systemctl stop arg.service 2>/dev/null || true
  if ! restore_previous_database; then
    rm -f "$staged" "$current" "$current_database"
    systemctl start arg.service || true
    echo "Could not restore the previous database snapshot; rollback was not applied." >&2
    return 1
  fi
  if ! mv -f "$staged" "$arg_binary"; then
    restore_database_snapshot "$current_database" 2>/dev/null || true
    rm -f "$staged" "$current" "$current_database"
    systemctl start arg.service || true
    echo "Could not install the previous binary; the current ARG release and database were retained." >&2
    return 1
  fi
  systemctl start arg.service || true
  if rollback_health_check; then
    install -o root -g root -m 0755 "$current" "$rollback_binary"
    if [[ -n "$current_database" ]]; then mv -f "$current_database" "$rollback_database"; fi
    rm -f "$current"
    printf 'ARG rolled back and passed its health check.\n'
    return 0
  fi
  systemctl stop arg.service 2>/dev/null || true
  install -o root -g root -m 0755 "$current" "$arg_binary"
  if [[ -n "$current_database" ]]; then
    restore_database_snapshot "$current_database" || echo "Could not restore the current database snapshot after rollback failure." >&2
  fi
  rm -f "$current_database"
  rm -f "$current"
  systemctl start arg.service || true
  echo "Rollback failed its health check; the previously running binary was restored." >&2
  return 1
}

write_update_state() {
  local state="$1" phase="$2" message="${3:-}" started job_id updated tmp previous_job
  job_id="${ARG_UPDATE_JOB_ID:-arg-$(cat /proc/sys/kernel/random/uuid 2>/dev/null || date +%s)}"
  started="$(sed -n 's/.*"started_at":\([0-9][0-9]*\).*/\1/p' "$update_state_file" 2>/dev/null | head -n 1 || true)"
  previous_job="$(sed -n 's/.*"job_id":"\([^"]*\)".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1 || true)"
  if [[ ! "$started" =~ ^[0-9]+$ || "$previous_job" != "$job_id" ]]; then started="$(date +%s)"; fi
  updated="$(date +%s)"
  # Messages are fixed internal strings; strip control characters and JSON quotes defensively.
  message="${message//$'\n'/ }"
  message="${message//\"/}"
  tmp="${update_state_file}.tmp.$$"
  printf '{"job_id":"%s","state":"%s","phase":"%s","started_at":%s,"updated_at":%s,"message":"%s"}\n' \
    "$job_id" "$state" "$phase" "$started" "$updated" "$message" > "$tmp"
  chmod 0644 "$tmp"
  mv -f "$tmp" "$update_state_file"
}

case "${1:-}" in
  logs)
    unit="$(cat "$update_last_unit_file" 2>/dev/null || true)"
    [[ "$unit" =~ ^arg-web-update-[0-9]+$ ]] || exit 0
    journalctl --no-pager --output=short-iso --lines=100 --unit="$unit.service" 2>&1 || true
    ;;
  progress)
    started="$(awk 'NR==1 {print $1}' "$update_status_file" 2>/dev/null || true)"
    state="$(sed -n 's/.*"state":"\([^"]*\)".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
    if [[ "$started" =~ ^[0-9]+$ && "$state" == running ]]; then
      owner="$(awk 'NR==1 {print $2}' "$update_status_file" 2>/dev/null || true)"
      unit="$(cat "$update_last_unit_file" 2>/dev/null || true)"
      if ! update_job_active; then
        export ARG_UPDATE_JOB_ID="$(sed -n 's/.*\"job_id\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
        write_update_state failed failed "The update stopped responding. Review its logs before retrying."
        rm -f "$update_status_file"
        rmdir "$update_lock_dir" 2>/dev/null || true
      fi
    fi
    if [[ -r "$update_state_file" ]]; then
      cat "$update_state_file"
    else
      printf '{"job_id":null,"state":"idle","phase":"idle","started_at":null,"updated_at":null,"message":""}\n'
    fi
    ;;
  state)
    case "${2:-}" in running|succeeded|failed|cancelled) ;; *) exit 2 ;; esac
    case "${3:-}" in scheduled|preparing|building|installing|restarting|verifying|complete|cancelling|cancelled|rolled_back|failed|idle) ;; *) exit 2 ;; esac
    write_update_state "$2" "$3" "${4:-}"
    ;;
  cancel)
    cancel_update
    ;;
  rollback)
    rollback_update
    ;;
  check)
    branch="${2:-main}"
    valid_branch "$branch" || { echo "Invalid update branch. Allowed: main, development." >&2; exit 2; }
    [[ -d "$source_dir/.git" ]] || { echo "ARG source checkout is missing at $source_dir; run 'sudo arg update' to repair it." >&2; exit 3; }
    local_rev="$(git -C "$source_dir" rev-parse --verify HEAD 2>/dev/null || true)"
    if ! remote_rev="$(GIT_TERMINAL_PROMPT=0 timeout 8s git -c credential.helper= ls-remote "$repository" "refs/heads/$branch" 2>&1 | awk 'NR==1 {print $1}')" || [[ ! "$remote_rev" =~ ^[0-9a-f]{40}$ ]]; then
      remote_rev="$(curl --connect-timeout 2 --max-time 6 -fsSL -H 'Accept: application/vnd.github+json' "https://api.github.com/repos/themrcortex/arg/commits/$branch" 2>/dev/null | sed -n 's/^[[:space:]]*"sha":[[:space:]]*"\([0-9a-f]\{40\}\)".*/\1/p' | head -n 1 || true)"
    fi
    [[ "$local_rev" =~ ^[0-9a-f]{40}$ ]] || { echo "Could not read the installed ARG revision from $source_dir; run 'sudo arg update' to repair it." >&2; exit 4; }
    [[ "$remote_rev" =~ ^[0-9a-f]{40}$ ]] || { echo "Could not reach GitHub to check ARG updates. Check DNS, HTTPS access, and the server clock, then retry." >&2; exit 4; }
    remote_version="$(curl --connect-timeout 2 --max-time 4 -fsSL "https://raw.githubusercontent.com/themrcortex/arg/$branch/Cargo.toml" 2>/dev/null | sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1 || true)"
    [[ "$remote_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][A-Za-z0-9.-]+)?$ ]] || remote_version="unknown"
    printf '%s %s %s\n' "$local_rev" "$remote_rev" "$remote_version"
    ;;
  run)
    mode="${2:-complete}"
    branch="${3:-main}"
    case "$mode" in fast|complete) ;; *) echo "Invalid update build mode." >&2; exit 2 ;; esac
    valid_branch "$branch" || { echo "Invalid update branch. Allowed: main, development." >&2; exit 2; }
    clear_stale_update_lock
    if ! mkdir "$update_lock_dir" 2>/dev/null; then
      echo "ARG_UPDATE_IN_PROGRESS: another update is already running." >&2
      exit 5
    fi
    printf '%s %s %s\n' "$(date +%s)" "$$" "$(awk '{print $22}' "/proc/$$/stat")" > "$update_status_file"
    chmod 0644 "$update_status_file"
    export ARG_UPDATE_JOB_ID="arg-$(cat /proc/sys/kernel/random/uuid 2>/dev/null || date +%s)-$$"
    write_update_state running scheduled "Update queued; waiting for the service helper."
    unit="arg-web-update-$$"
    if ! systemd-run --quiet --collect --unit="$unit" --on-active=2s --setenv=ARG_UPDATE_RESERVED=1 --setenv="ARG_UPDATE_MODE=$mode" --setenv="ARG_UPDATE_BRANCH=$branch" --setenv="ARG_UPDATE_JOB_ID=$ARG_UPDATE_JOB_ID" /usr/local/bin/arg update; then
      write_update_state failed scheduling "The system could not start the update job."
      rm -f "$update_status_file"
      rmdir "$update_lock_dir" 2>/dev/null || true
      exit 1
    fi
    printf '%s\n' "$unit" > "$update_last_unit_file"
    chmod 0644 "$update_last_unit_file"
    ;;
  *) exit 2 ;;
esac
