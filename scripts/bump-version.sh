#!/usr/bin/env bash
set -Eeuo pipefail

if [[ $# -ne 1 || ! "$1" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "Usage: scripts/bump-version.sh MAJOR.MINOR.PATCH" >&2
  exit 2
fi

version="$1"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
if [[ -x "$root/.cargo/bin/cargo" ]]; then
  export CARGO_HOME="$root/.cargo"
  export RUSTUP_HOME="$root/.rustup"
  export PATH="$root/.cargo/bin:$PATH"
fi

python3 - "$version" <<'PY'
import json
import re
import sys
from pathlib import Path

version = sys.argv[1]
cargo = Path("Cargo.toml")
text = cargo.read_text()
updated, count = re.subn(r'(?m)^(version\s*=\s*)"[^"]+"', rf'\g<1>"{version}"', text, count=1)
if count != 1:
    raise SystemExit("Could not update the root Cargo package version")
cargo.write_text(updated)

for filename in ("frontend/package.json", "frontend/package-lock.json"):
    path = Path(filename)
    data = json.loads(path.read_text())
    data["version"] = version
    if filename.endswith("package-lock.json"):
        data.setdefault("packages", {}).setdefault("", {})["version"] = version
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")
PY

cargo check
cargo check --locked
node - "$version" <<'JS'
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
