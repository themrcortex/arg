#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

branch="$(git branch --show-current)"
[[ "$branch" == main ]] || {
  echo "Stable releases are bumped on main. Use scripts/bump-development-version.sh on development." >&2
  exit 2
}

current="$(sed -n '0,/^version = /s/^version = "\([^"]*\)"/\1/p' Cargo.toml)"
[[ "$current" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || {
  echo "Invalid current version in Cargo.toml: $current" >&2
  exit 1
}
IFS=. read -r major minor patch <<< "$current"
case "${1:-patch}" in
  patch) next="$major.$minor.$((patch + 1))" ;;
  minor) next="$major.$((minor + 1)).0" ;;
  major) next="$((major + 1)).0.0" ;;
  *)
    if [[ $# -eq 1 && "$1" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
      next="$1"
    else
      echo "Usage: scripts/bump-version.sh [patch|minor|major|MAJOR.MINOR.PATCH]" >&2
      exit 2
    fi
    ;;
esac
[[ $# -le 1 ]] || { echo "Pass only one version or bump type." >&2; exit 2; }
if [[ "$next" != "$major.$minor.$((patch + 1))" && "$next" != "$major.$((minor + 1)).0" && "$next" != "$((major + 1)).0.0" ]]; then
  echo "Next version must be the immediate patch, minor, or major release after $current (got $next)." >&2
  exit 2
fi

python3 - "$current" "$next" <<'PY'
import json
import re
import sys
from pathlib import Path

current, version = sys.argv[1:]
files = []
for filename in ("frontend/package.json", "frontend/package-lock.json"):
    path = Path(filename)
    data = json.loads(path.read_text())
    if data.get("version") != current:
        raise SystemExit(f"{filename} has version {data.get('version')}, expected {current}")
    data["version"] = version
    if filename.endswith("package-lock.json"):
        data.setdefault("packages", {}).setdefault("", {})["version"] = version
    files.append((path, data))

cargo = Path("Cargo.toml")
text = cargo.read_text()
updated, count = re.subn(r'(?m)^(version\s*=\s*)"[^"]+"', rf'\g<1>"{version}"', text, count=1)
if count != 1:
    raise SystemExit("Could not update the root Cargo package version")
cargo.write_text(updated)
for path, data in files:
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")

changelog = Path("CHANGELOG.md")
content = changelog.read_text()
heading = f"## v{version} — "
if heading not in content:
    from datetime import datetime, timezone
    date = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    content = content.replace(
        "# ARG Change Log\n",
        f"# ARG Change Log\n\n{heading}{date}\n\n### Changed\n- Add release notes before publishing.\n",
        1,
    )
    changelog.write_text(content)
PY

if [[ -x "$root/.cargo/bin/cargo" ]]; then
  export CARGO_HOME="$root/.cargo"
  export RUSTUP_HOME="$root/.rustup"
  export PATH="$root/.cargo/bin:$PATH"
fi
cargo check
cargo check --locked
node - "$next" <<'JS'
const fs = require('node:fs')
const expected = process.argv[2]
const cargo = fs.readFileSync('Cargo.toml', 'utf8').match(/^version\s*=\s*"([^"]+)"/m)
const frontend = JSON.parse(fs.readFileSync('frontend/package.json', 'utf8')).version
const lock = JSON.parse(fs.readFileSync('frontend/package-lock.json', 'utf8'))
if (cargo?.[1] !== expected || frontend !== expected || lock.version !== expected || lock.packages?.['']?.version !== expected) {
  throw new Error('Rust and frontend package versions are not synchronized')
}
console.log(`Version synchronized: ${expected}`)
JS
