#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

for ref in origin/main origin/development; do
  git rev-parse --verify "$ref^{commit}" >/dev/null 2>&1 || {
    echo "Missing $ref. Run 'git fetch origin main development' first." >&2
    exit 2
  }
done

git merge-base --is-ancestor origin/main origin/development || {
  echo "The development branch must include the current main branch." >&2
  exit 1
}

main_version="$(git show origin/main:Cargo.toml | sed -n '0,/^version = /s/^version = "\([^"]*\)"/\1/p')"
dev_version="$(git show origin/development:Cargo.toml | sed -n '0,/^version = /s/^version = "\([^"]*\)"/\1/p')"
local_version="$(sed -n '0,/^version = /s/^version = "\([^"]*\)"/\1/p' Cargo.toml)"
frontend_version="$(node -p "require('./frontend/package.json').version")"
lock_version="$(node -p "require('./frontend/package-lock.json').version")"
lock_root_version="$(node -p "require('./frontend/package-lock.json').packages[''].version")"

python3 - "$main_version" "$dev_version" "$local_version" "$frontend_version" "$lock_version" "$lock_root_version" "$(git branch --show-current)" <<'PY'
import re
import sys

main, development, local, frontend, lock, lock_root, branch = sys.argv[1:]
stable = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
dev = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)-dev\.(0|[1-9]\d*)$")
main_match = stable.fullmatch(main)
dev_match = dev.fullmatch(development)
if not main_match:
    raise SystemExit(f"main must use a stable SemVer version, got {main!r}")
if not dev_match:
    raise SystemExit(f"development must use X.Y.Z-dev.N, got {development!r}")
main_tuple = tuple(map(int, main_match.groups()))
dev_tuple = tuple(map(int, dev_match.groups()[:3]))
expected = (*main_tuple[:2], main_tuple[2] + 1)
if dev_tuple != expected:
    raise SystemExit(f"development must be based on the next patch after main ({expected[0]}.{expected[1]}.{expected[2]}-dev.N); got {development}")
if not (local == frontend == lock == lock_root):
    raise SystemExit(f"Rust/frontend versions differ: Cargo={local}, package={frontend}, locks={lock}/{lock_root}")
if branch == "main" and local != main:
    raise SystemExit(f"Local main version {local} does not match origin/main {main}")
if branch == "development" and local != development:
    raise SystemExit(f"Local development version {local} does not match origin/development {development}")
if branch not in {"main", "development"}:
    raise SystemExit(f"Version check must run on main or development, got {branch!r}")
print(f"Version control OK: main v{main}; development v{development} (ahead)")
PY
