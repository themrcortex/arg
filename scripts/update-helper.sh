#!/usr/bin/env bash
set -Eeuo pipefail

readonly source_dir="/opt/donatello/source"
readonly repository="https://github.com/themrcortex/donatello.git"
readonly branch="main"

case "${1:-}" in
  check)
    [[ -d "$source_dir/.git" ]] || exit 3
    local_rev="$(git -C "$source_dir" rev-parse --verify HEAD)"
    remote_rev="$(GIT_TERMINAL_PROMPT=0 timeout 8s git -c credential.helper= ls-remote "$repository" "refs/heads/$branch" | awk 'NR==1 {print $1}')"
    [[ "$local_rev" =~ ^[0-9a-f]{40}$ && "$remote_rev" =~ ^[0-9a-f]{40}$ ]] || exit 4
    printf '%s %s\n' "$local_rev" "$remote_rev"
    ;;
  run)
    exec systemd-run --quiet --collect --unit="donatello-web-update-$$" --on-active=2s /usr/local/bin/donatello update
    ;;
  *) exit 2 ;;
esac
