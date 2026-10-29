#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
branch="$(git branch --show-current)"
case "$branch" in main|development) ;; *) echo "Run this script from main or development." >&2; exit 2 ;; esac

git diff --quiet && git diff --cached --quiet || {
  echo "Commit tracked changes before publishing a version tag." >&2
  exit 2
}
git fetch origin main development --quiet
remote_ref="origin/$branch"
[[ "$(git rev-parse HEAD)" == "$(git rev-parse "$remote_ref")" ]] || {
  echo "Push the current $branch commit before publishing its release tag." >&2
  exit 2
}

version="$(sed -n '0,/^version = /s/^version = "\([^"]*\)"/\1/p' Cargo.toml)"
case "$branch:$version" in
  main:*) [[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || { echo "main must contain a stable X.Y.Z version, got $version." >&2; exit 2; } ;;
  development:*) [[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-dev\.(0|[1-9][0-9]*)$ ]] || { echo "development must contain a X.Y.Z-dev.N version, got $version." >&2; exit 2; } ;;
esac

node - "$version" <<'JS'
const fs = require('node:fs')
const expected = process.argv[2]
const cargo = fs.readFileSync('Cargo.toml', 'utf8').match(/^version\s*=\s*"([^"]+)"/m)?.[1]
const frontend = JSON.parse(fs.readFileSync('frontend/package.json', 'utf8')).version
const lock = JSON.parse(fs.readFileSync('frontend/package-lock.json', 'utf8'))
if (cargo !== expected || frontend !== expected || lock.version !== expected || lock.packages?.['']?.version !== expected) {
  throw new Error(`Rust and frontend versions do not all match ${expected}`)
}
JS

tag="v$version"
if git show-ref --verify --quiet "refs/tags/$tag" || git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1; then
  echo "Release tag $tag already exists; release tags are immutable." >&2
  exit 2
fi

tag_created=0
cleanup_tag() {
  if (( tag_created )); then git tag -d "$tag" >/dev/null 2>&1 || true; fi
}
trap cleanup_tag EXIT
git tag -a "$tag" -m "ARG $version"
tag_created=1
git push origin "refs/tags/$tag"
tag_created=0
trap - EXIT
printf 'Pushed %s from %s. GitHub Actions will build and publish its matching release assets.\n' "$tag" "$branch"
