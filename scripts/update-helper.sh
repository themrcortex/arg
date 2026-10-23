#!/usr/bin/env bash
set -Eeuo pipefail

readonly source_dir="${ARG_SOURCE_DIR:-/opt/arg/source}"
readonly repository="${ARG_REPOSITORY:-https://github.com/themrcortex/arg.git}"
readonly branch="${ARG_BRANCH:-main}"
readonly update_lock_dir="/run/arg-update.lock"
readonly update_status_file="/run/arg-update-in-progress"

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

case "${1:-}" in
  check)
    [[ -d "$source_dir/.git" ]] || { echo "ARG source checkout is missing at $source_dir; run 'sudo arg update' to repair it." >&2; exit 3; }
    local_rev="$(git -C "$source_dir" rev-parse --verify HEAD 2>/dev/null || true)"
    if ! remote_rev="$(GIT_TERMINAL_PROMPT=0 timeout 25s git -c credential.helper= ls-remote "$repository" "refs/heads/$branch" 2>&1 | awk 'NR==1 {print $1}')" || [[ ! "$remote_rev" =~ ^[0-9a-f]{40}$ ]]; then
      remote_rev="$(curl --connect-timeout 5 --max-time 20 -fsSL -H 'Accept: application/vnd.github+json' "https://api.github.com/repos/themrcortex/arg/commits/$branch" 2>/dev/null | sed -n 's/^[[:space:]]*"sha":[[:space:]]*"\([0-9a-f]\{40\}\)".*/\1/p' | head -n 1 || true)"
    fi
    [[ "$local_rev" =~ ^[0-9a-f]{40}$ ]] || { echo "Could not read the installed ARG revision from $source_dir; run 'sudo arg update' to repair it." >&2; exit 4; }
    [[ "$remote_rev" =~ ^[0-9a-f]{40}$ ]] || { echo "Could not reach GitHub to check ARG updates. Check DNS, HTTPS access, and the server clock, then retry." >&2; exit 4; }
    printf '%s %s\n' "$local_rev" "$remote_rev"
    ;;
  run)
    clear_stale_update_lock
    if ! mkdir "$update_lock_dir" 2>/dev/null; then
      echo "ARG_UPDATE_IN_PROGRESS: another update is already running." >&2
      exit 5
    fi
    printf '%s %s\n' "$(date +%s)" "$$" > "$update_status_file"
    chmod 0644 "$update_status_file"
    if ! systemd-run --quiet --collect --unit="arg-web-update-$$" --on-active=2s --setenv=ARG_UPDATE_RESERVED=1 /usr/local/bin/arg update; then
      rm -f "$update_status_file"
      rmdir "$update_lock_dir" 2>/dev/null || true
      exit 1
    fi
    ;;
  *) exit 2 ;;
esac
