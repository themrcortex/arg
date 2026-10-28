#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

while IFS= read -r line; do
  reference="${line#*uses:}"
  reference="${reference%%#*}"
  reference="${reference//[[:space:]]/}"
  if [[ ! "$reference" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+@[a-f0-9]{40}$ ]]; then
    printf 'Workflow action is not pinned to a full commit SHA: %s\n' "$line" >&2
    exit 1
  fi
done < <(grep -hE '^[[:space:]]+uses:' .github/workflows/*.yml)

awk '
  /^[[:space:]]+- uses: actions\/checkout@[a-f0-9]{40}/ {
    if (awaiting_option && !has_persist_false) exit 1
    indent = match($0, /[^ ]/) - 1
    found++
    awaiting_option = 1
    has_persist_false = 0
    next
  }
  awaiting_option {
    current_indent = match($0, /[^ ]/) - 1
    if (current_indent <= indent) {
      if (!has_persist_false) exit 1
      awaiting_option = 0
    }
    if (awaiting_option && $0 ~ /^[[:space:]]+persist-credentials: false[[:space:]]*$/) {
      has_persist_false = 1
    }
  }
  END {
    if (found == 0 || (awaiting_option && !has_persist_false)) exit 1
  }
' .github/workflows/*.yml

awk '
  /^permissions:$/ { in_permissions = 1; found = 1; next }
  in_permissions && /^[^[:space:]]/ { exit }
  in_permissions && /(pages|id-token): write/ { exit 1 }
  END { if (!found) exit 1 }
' .github/workflows/docs-pages.yml

awk '
  /^  deploy:$/ { in_deploy = 1; next }
  in_deploy && /^  [^[:space:]]/ { exit }
  in_deploy && /^    permissions:$/ { found = 1 }
  in_deploy && /^      pages: write$/ { pages = 1 }
  in_deploy && /^      id-token: write$/ { identity = 1 }
  END { if (!found || !pages || !identity) exit 1 }
' .github/workflows/docs-pages.yml

printf 'Workflow action pinning and Pages permission scope checks passed.\n'
