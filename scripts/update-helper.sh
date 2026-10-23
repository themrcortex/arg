#!/usr/bin/env bash
set -Eeuo pipefail

readonly source_dir="${ARG_SOURCE_DIR:-/opt/arg/source}"
readonly repository="${ARG_REPOSITORY:-https://github.com/themrcortex/arg.git}"
readonly branch="${ARG_BRANCH:-main}"

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
    exec systemd-run --quiet --collect --unit="arg-web-update-$$" --on-active=2s /usr/local/bin/arg update
    ;;
  *) exit 2 ;;
esac
