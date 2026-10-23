#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
version="$(sed -n '0,/^version = /s/^version = "\([^"]*\)"/\1/p' Cargo.toml)"
tag="v$version"

frontend_version="$(node -p "require('./frontend/package.json').version")"
lock_version="$(node -p "require('./frontend/package-lock.json').version")"
[[ "$version" == "$frontend_version" && "$version" == "$lock_version" ]] || {
  echo "Rust and frontend versions do not match." >&2
  exit 1
}
[[ -z "$(git status --porcelain)" ]] || {
  echo "Commit or stash local changes before publishing." >&2
  exit 1
}
git fetch origin main --quiet
git merge-base --is-ancestor HEAD origin/main || {
  echo "Push this commit to origin/main before publishing." >&2
  exit 1
}

notes="$(mktemp)"
trap 'rm -f "$notes"' EXIT
awk -v heading="## $tag " 'index($0, heading) == 1 { found=1; next } found && /^## / { exit } found { print }' CHANGELOG.md > "$notes"
[[ -s "$notes" ]] || {
  echo "No changelog section found for $tag." >&2
  exit 1
}

gh release create "$tag" --title "Donatello $tag" --notes-file "$notes" --target "$(git rev-parse HEAD)" install.sh --latest
