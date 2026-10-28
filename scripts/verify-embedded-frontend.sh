#!/usr/bin/env bash
set -Eeuo pipefail

changed="$(git status --short --untracked-files=all -- frontend/dist)"
if [[ -n "$changed" ]]; then
  printf 'The embedded frontend is out of date. Run `cd frontend && npm ci && npm run build`, then commit frontend/dist with the source changes.\n\n%s\n' "$changed" >&2
  exit 1
fi

echo 'Embedded frontend is current.'
