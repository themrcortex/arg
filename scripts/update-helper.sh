#!/usr/bin/env bash
set -Eeuo pipefail

readonly source_dir="${ARG_SOURCE_DIR:-/opt/arg/source}"
readonly repository="${ARG_REPOSITORY:-https://github.com/themrcortex/arg.git}"
readonly update_lock_dir="${ARG_UPDATE_LOCK_DIR:-/run/arg-update.lock}"
readonly update_mutex_file="${ARG_UPDATE_MUTEX_FILE:-${update_lock_dir}.mutex}"
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

clear_stale_update_lock_locked() {
  [[ -d "$update_lock_dir" ]] || return 0
  update_job_active && return 0
  export ARG_UPDATE_JOB_ID="$(sed -n 's/.*\"job_id\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
  write_update_state failed failed "The stale update lock was cleared because no updater process or systemd job is active."
  rm -f "$update_status_file" "$update_cancel_file"
  rmdir "$update_lock_dir" 2>/dev/null || true
}

reserve_update() {
  local operation="$1" phase="$2" branch="$3" build_mode="$4" owner owner_start job_id mutex_fd
  exec {mutex_fd}>"$update_mutex_file"
  chmod 0600 "$update_mutex_file"
  flock -x "$mutex_fd" || { exec {mutex_fd}>&-; echo "Could not lock ARG update state." >&2; return 1; }
  clear_stale_update_lock_locked
  if ! mkdir "$update_lock_dir" 2>/dev/null; then
    flock -u "$mutex_fd" || true
    exec {mutex_fd}>&-
    echo "ARG_UPDATE_IN_PROGRESS: another update or rollback is already running." >&2
    return 5
  fi
  owner="${BASHPID:-$$}"
  owner_start="$(awk '{print $22}' "/proc/$owner/stat" 2>/dev/null || true)"
  job_id="arg-$(cat /proc/sys/kernel/random/uuid 2>/dev/null || date +%s)-$owner"
  export ARG_UPDATE_JOB_ID="$job_id" ARG_UPDATE_OPERATION="$operation" ARG_UPDATE_BRANCH="$branch" ARG_UPDATE_MODE="$build_mode"
  printf '%s %s %s\n' "$(date +%s)" "$owner" "$owner_start" > "$update_status_file"
  chmod 0644 "$update_status_file"
  write_update_state running "$phase" "$([[ "$operation" == rollback ]] && printf 'Preparing rollback.' || printf 'Update queued; waiting for the service helper.')"
  flock -u "$mutex_fd" || true
  exec {mutex_fd}>&-
}

rollback_health_check() {
  local bind port scheme attempt attempts
  bind="$(awk -F= '$1 == "ARG_TLS_BIND" {tls=substr($0,index($0,"=")+1)} $1 == "ARG_BIND" {http=substr($0,index($0,"=")+1)} END {print tls != "" ? tls : http}' "$environment_file" 2>/dev/null)"
  [[ -n "$bind" ]] || bind="0.0.0.0:8282"
  port="${bind##*:}"
  scheme=http
  grep -q '^ARG_TLS_BIND=' "$environment_file" 2>/dev/null && scheme=https
  attempts="${ARG_ROLLBACK_HEALTH_ATTEMPTS:-20}"
  [[ "$attempts" =~ ^[0-9]+$ ]] && (( attempts > 0 && attempts <= 60 )) || attempts=20
  for ((attempt = 1; attempt <= attempts; attempt++)); do
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
  local unit phase operation owner owner_start actual_start current_command process_tree pid rollback_ok=1 mutex_fd target_job current_job current_state current_operation
  unit="$(cat "$update_last_unit_file" 2>/dev/null || true)"
  phase="$(sed -n 's/.*\"phase\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
  operation="$(sed -n 's/.*\"operation\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
  if [[ "$operation" == rollback ]]; then
    echo "ARG rollback cannot be cancelled safely while its binary and database are being switched. Wait for its health check to finish." >&2
    return 1
  fi
  target_job="$(sed -n 's/.*\"job_id\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
  exec {mutex_fd}>"$update_mutex_file"
  chmod 0600 "$update_mutex_file"
  flock -x "$mutex_fd" || { exec {mutex_fd}>&-; echo "Could not lock ARG update state during cancellation." >&2; return 1; }
  current_job="$(sed -n 's/.*\"job_id\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
  current_state="$(sed -n 's/.*\"state\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
  current_operation="$(sed -n 's/.*\"operation\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
  if [[ ! -d "$update_lock_dir" || "$current_job" != "$target_job" || "$current_operation" == rollback || ( "$current_state" != running && "$current_state" != cancelling ) ]]; then
    flock -u "$mutex_fd" || true
    exec {mutex_fd}>&-
    echo "No matching active update remains; no files were changed." >&2
    return 1
  fi
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
  # systemctl stop is synchronous, but direct CLI updates are not systemd jobs.
  # Do not restore files while their EXIT trap is still cleaning up the update.
  if [[ ! "$unit" =~ ^arg-web-update-[0-9]+$ && "$owner" =~ ^[0-9]+$ && "$owner" != "$$" ]]; then
    for _ in {1..10}; do
      actual_start="$(awk '{print $22}' "/proc/$owner/stat" 2>/dev/null || true)"
      [[ "$actual_start" == "$owner_start" ]] || break
      sleep 1
    done
    actual_start="$(awk '{print $22}' "/proc/$owner/stat" 2>/dev/null || true)"
    if [[ "$actual_start" == "$owner_start" ]]; then
      flock -u "$mutex_fd" || true
      exec {mutex_fd}>&-
      echo "The updater has not exited yet; its recovery trap is still running. Retry cancellation after it finishes." >&2
      return 1
    fi
  fi
  current_job="$(sed -n 's/.*\"job_id\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
  if [[ ! -d "$update_lock_dir" || "$current_job" != "${ARG_UPDATE_JOB_ID:-}" ]]; then
    # The updater's own trap completed the cancellation and released its reservation.
    if [[ "$(sed -n 's/.*\"state\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)" == cancelled ]]; then
      flock -u "$mutex_fd" || true
      exec {mutex_fd}>&-
      echo "ARG update cancelled."
      return 0
    fi
    flock -u "$mutex_fd" || true
    exec {mutex_fd}>&-
    echo "The update reservation changed while cancellation was running; no recovery files were modified." >&2
    return 1
  fi
  if [[ "$phase" == installing || "$phase" == restarting || "$phase" == verifying ]]; then
    systemctl stop arg.service 2>/dev/null || true
    restore_previous_database || { echo "Warning: the previous SQLite snapshot could not be restored; the database was left unchanged." >&2; rollback_ok=0; }
    install_rollback_binary || { echo "Update cancelled, but automatic binary restore failed." >&2; rollback_ok=0; }
    systemctl start arg.service || true
    rollback_health_check || { echo "Previous binary restored, but ARG did not pass its health check." >&2; rollback_ok=0; }
  fi
  if (( rollback_ok )); then
    write_update_state cancelled cancelled "The update was cancelled. The previous ARG version is running."
  else
    write_update_state failed failed "The update was cancelled, but automatic recovery needs attention. Check ARG service logs."
  fi
  rm -f "$update_status_file" "$update_cancel_file"
  rmdir "$update_lock_dir" 2>/dev/null || true
  flock -u "$mutex_fd" || true
  exec {mutex_fd}>&-
  (( rollback_ok )) || return 1
  echo "ARG update cancelled."
}

rollback_update() {
  local phase
  if [[ -d "$update_lock_dir" ]]; then
    phase="$(sed -n 's/.*\"phase\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
    cancel_update || return 1
    if [[ "$phase" != installing && "$phase" != restarting && "$phase" != verifying ]]; then
      echo "The active update was cancelled before installation. The installed release was retained; run rollback again to restore the previous release." >&2
      return 1
    fi
    echo "The active update was cancelled during installation; the previous release was restored."
    return 0
  fi
  # A rollback is a mutating maintenance job too. Reserve the same lock as updates
  # so an update cannot start while binaries or the database are being swapped.
  (
    set -Eeuo pipefail
    reserve_update rollback installing "" "" || exit $?
    write_update_state running installing "Saving the current release and restoring the previous ARG version."
    rollback_cleanup() {
      local result="$?" final_state
      final_state="$(sed -n 's/.*\"state\":\"\([^\"]*\)\".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
      if (( result != 0 )) && [[ "$final_state" == running ]]; then
        write_update_state failed failed "Rollback did not complete. Check ARG service logs and the saved recovery files."
      fi
      rm -f "$update_status_file" "$update_cancel_file"
      rmdir "$update_lock_dir" 2>/dev/null || true
      return "$result"
    }
    trap rollback_cleanup EXIT
    rollback_update_locked
  )
}

rollback_update_locked() {
  local current staged current_staged phase database_path current_database=""
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
  current_staged="$(mktemp "$arg_binary_dir/.arg-recovery.XXXXXX")"
  install -o root -g root -m 0755 "$current" "$current_staged"
  if ! systemctl stop arg.service 2>/dev/null; then
    rm -f "$staged" "$current_staged" "$current" "$current_database"
    echo "Could not stop ARG safely; rollback was not applied." >&2
    return 1
  fi
  if ! restore_previous_database; then
    rm -f "$staged" "$current_staged" "$current" "$current_database"
    systemctl start arg.service || true
    echo "Could not restore the previous database snapshot; rollback was not applied." >&2
    return 1
  fi
  if ! mv -f "$staged" "$arg_binary"; then
    if [[ -n "$current_database" ]] && ! restore_database_snapshot "$current_database"; then
      rm -f "$staged" "$current_staged" "$current" "$current_database"
      echo "Could not install the rollback binary or restore the current database snapshot. ARG remains stopped to avoid a binary/database mismatch." >&2
      return 1
    fi
    rm -f "$staged" "$current_staged" "$current" "$current_database"
    systemctl start arg.service || true
    echo "Could not install the previous binary; the current ARG release and database were retained." >&2
    return 1
  fi
  systemctl start arg.service || true
  if rollback_health_check; then
    install -o root -g root -m 0755 "$current" "$rollback_binary"
    if [[ -n "$current_database" ]]; then mv -f "$current_database" "$rollback_database"; fi
    rm -f "$current_staged" "$current"
    write_update_state succeeded rolled_back "The previous ARG binary and matching database snapshot passed the health check."
    printf 'ARG rolled back and passed its health check.\n'
    return 0
  fi
  if ! systemctl stop arg.service 2>/dev/null; then
    echo "Rollback health check failed and ARG could not be stopped. The rollback version and its database were left in place; recovery snapshots are retained." >&2
    return 1
  fi
  if ! mv -f "$current_staged" "$arg_binary"; then
    echo "Rollback health check failed and the current binary could not be restored. ARG remains stopped; recovery snapshots are retained." >&2
    return 1
  fi
  if [[ -n "$current_database" ]] && ! restore_database_snapshot "$current_database"; then
    echo "The current binary was restored, but its database snapshot could not be restored. ARG remains stopped to avoid starting with a mismatched schema." >&2
    return 1
  fi
  rm -f "$current_database" "$current"
  systemctl start arg.service || true
  write_update_state failed failed "Rollback failed its health check; the previously running binary and database were restored."
  echo "Rollback failed its health check; the previously running binary was restored." >&2
  return 1
}

write_update_state() {
  local state="$1" phase="$2" message="${3:-}" started job_id updated tmp previous_job operation branch build_mode previous_phase=""
  job_id="${ARG_UPDATE_JOB_ID:-arg-$(cat /proc/sys/kernel/random/uuid 2>/dev/null || date +%s)}"
  started="$(sed -n 's/.*"started_at":\([0-9][0-9]*\).*/\1/p' "$update_state_file" 2>/dev/null | head -n 1 || true)"
  previous_job="$(sed -n 's/.*"job_id":"\([^"]*\)".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1 || true)"
  if [[ ! "$started" =~ ^[0-9]+$ || "$previous_job" != "$job_id" ]]; then started="$(date +%s)"; fi
  if [[ "$previous_job" == "$job_id" ]]; then
    operation="$(sed -n 's/.*"operation":"\([^"]*\)".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1 || true)"
    branch="$(sed -n 's/.*"branch":"\([^"]*\)".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1 || true)"
    build_mode="$(sed -n 's/.*"build_mode":"\([^"]*\)".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1 || true)"
    previous_phase="$(sed -n 's/.*"phase":"\([^"]*\)".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1 || true)"
  fi
  operation="${ARG_UPDATE_OPERATION:-${operation:-update}}"
  branch="${ARG_UPDATE_BRANCH:-${branch:-}}"
  build_mode="${ARG_UPDATE_MODE:-${build_mode:-}}"
  [[ "$operation" == rollback ]] || operation=update
  valid_branch "$branch" || branch=""
  case "$build_mode" in fast|complete) ;; *) build_mode="" ;; esac
  case "$previous_phase" in scheduled|preparing|building|installing|restarting|verifying|cancelling|rollback|complete|rolled_back|failed|cancelled|idle) ;; *) previous_phase="" ;; esac
  updated="$(date +%s)"
  # Messages are fixed internal strings; strip control characters and JSON quotes defensively.
  message="${message//$'\n'/ }"
  message="${message//\"/}"
  tmp="${update_state_file}.tmp.$$"
  printf '{"job_id":"%s","state":"%s","phase":"%s","previous_phase":"%s","operation":"%s","branch":"%s","build_mode":"%s","started_at":%s,"updated_at":%s,"message":"%s"}\n' \
    "$job_id" "$state" "$phase" "${previous_phase:-$phase}" "$operation" "$branch" "$build_mode" "$started" "$updated" "$message" > "$tmp"
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
    # Polling can race with a new reservation. Re-check under the shared mutex
    # before marking a job stale or removing its lock.
    exec {mutex_fd}>"$update_mutex_file"
    chmod 0600 "$update_mutex_file"
    flock -x "$mutex_fd" || { exec {mutex_fd}>&-; echo "Could not lock ARG update state." >&2; exit 1; }
    started="$(awk 'NR==1 {print $1}' "$update_status_file" 2>/dev/null || true)"
    state="$(sed -n 's/.*"state":"\([^"]*\)".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
    job_id="$(sed -n 's/.*"job_id":"\([^"]*\)".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
    if [[ "$started" =~ ^[0-9]+$ && "$state" == running && -d "$update_lock_dir" ]]; then
      owner="$(awk 'NR==1 {print $2}' "$update_status_file" 2>/dev/null || true)"
      unit="$(cat "$update_last_unit_file" 2>/dev/null || true)"
      if ! update_job_active; then
        export ARG_UPDATE_JOB_ID="$job_id"
        write_update_state failed failed "The update stopped responding. Review its logs before retrying."
        rm -f "$update_status_file" "$update_cancel_file"
        rmdir "$update_lock_dir" 2>/dev/null || true
      fi
    fi
    flock -u "$mutex_fd" || true
    exec {mutex_fd}>&-
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
    reserve_update update scheduled "$branch" "$mode" || exit $?
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
