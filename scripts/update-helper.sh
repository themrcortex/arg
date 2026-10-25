#!/usr/bin/env bash
set -Eeuo pipefail

readonly source_dir="${ARG_SOURCE_DIR:-/opt/arg/source}"
readonly repository="${ARG_REPOSITORY:-https://github.com/themrcortex/arg.git}"
readonly update_lock_dir="/run/arg-update.lock"
readonly update_status_file="/run/arg-update-in-progress"
readonly update_state_file="/run/arg-update-state.json"
readonly update_last_unit_file="/run/arg-update-last-unit"

valid_branch() {
  [[ "$1" == main || "$1" == development ]]
}

clear_stale_update_lock() {
  [[ -d "$update_lock_dir" ]] || return 0
  local started current lock_time stale_after=60
  started="$(awk 'NR==1 {print $1}' "$update_status_file" 2>/dev/null || true)"
  current="$(date +%s)"
  if [[ "$started" =~ ^[0-9]+$ ]]; then
    stale_after=21600
    lock_time="$started"
  else
    lock_time="$(stat -c %Y "$update_lock_dir" 2>/dev/null || echo "$current")"
  fi
  if (( current - lock_time > stale_after )); then
    rm -f "$update_status_file"
    rmdir "$update_lock_dir" 2>/dev/null || true
  fi
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
    if [[ "$started" =~ ^[0-9]+$ && "$state" == running ]] && (( $(date +%s) - started > 600 )); then
      owner="$(awk 'NR==1 {print $2}' "$update_status_file" 2>/dev/null || true)"
      unit="$(cat "$update_last_unit_file" 2>/dev/null || true)"
      unit_active=0
      [[ "$unit" =~ ^arg-web-update-[0-9]+$ ]] && systemctl is-active --quiet "$unit.service" && unit_active=1
      owner_active=0
      [[ "$owner" =~ ^[0-9]+$ ]] && kill -0 "$owner" 2>/dev/null && owner_active=1
      if (( ! unit_active && ! owner_active )); then
        export ARG_UPDATE_JOB_ID="$(sed -n 's/.*"job_id":"\([^"]*\)".*/\1/p' "$update_state_file" 2>/dev/null | head -n 1)"
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
    case "${2:-}" in running|succeeded|failed) ;; *) exit 2 ;; esac
    case "${3:-}" in scheduled|preparing|building|installing|restarting|verifying|complete|failed|idle) ;; *) exit 2 ;; esac
    write_update_state "$2" "$3" "${4:-}"
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
    printf '%s %s\n' "$(date +%s)" "$$" > "$update_status_file"
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
