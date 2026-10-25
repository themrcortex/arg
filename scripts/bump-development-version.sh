#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
[[ "$(git branch --show-current)" == development ]] || {
  echo "Switch to the development branch before bumping its version." >&2
  exit 2
}
git diff --quiet && git diff --cached --quiet || {
  echo "Commit or stash tracked changes before bumping the development version." >&2
  exit 2
}
git fetch origin main development --quiet

git merge-base --is-ancestor origin/main HEAD || {
  echo "Merge the latest main into development before bumping its version." >&2
  exit 1
}
main_version="$(git show origin/main:Cargo.toml | sed -n '0,/^version = /s/^version = "\([^"]*\)"/\1/p')"
current="$(sed -n '0,/^version = /s/^version = "\([^"]*\)"/\1/p' Cargo.toml)"
next="$(python3 - "$main_version" "$current" <<'PY'
import re
import sys
main, current = sys.argv[1:]
match = re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", main)
if not match:
    raise SystemExit(f"origin/main must be stable SemVer, got {main!r}")
major, minor, patch = map(int, match.groups())
base = f"{major}.{minor}.{patch + 1}"
dev = re.fullmatch(re.escape(base) + r"-dev\.(0|[1-9]\d*)", current)
sequence = int(dev.group(1)) + 1 if dev else 1
print(f"{base}-dev.{sequence}")
PY
)"

python3 - "$current" "$next" <<'PY'
import json
import re
import sys
from pathlib import Path
current, version = sys.argv[1:]
for filename in ("Cargo.toml", "frontend/package.json", "frontend/package-lock.json"):
    path = Path(filename)
    if filename == "Cargo.toml":
        text = path.read_text()
        updated, count = re.subn(r'(?m)^(version\s*=\s*)"[^"]+"', rf'\g<1>"{version}"', text, count=1)
        if count != 1:
            raise SystemExit("Could not update Cargo.toml")
        path.write_text(updated)
        continue
    data = json.loads(path.read_text())
    if data.get("version") != current:
        raise SystemExit(f"{filename} version {data.get('version')} does not match Cargo.toml {current}")
    data["version"] = version
    if filename.endswith("package-lock.json"):
        data["packages"][""]["version"] = version
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")

from datetime import datetime, timezone
changelog = Path("CHANGELOG.md")
content = changelog.read_text()
heading = f"## v{version} — "
if heading not in content:
    date = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    content = content.replace(
        "# ARG Change Log\n",
        f"# ARG Change Log\n\n{heading}{date}\n\n### Development\n- Development prerelease based on the next patch after main.\n",
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
printf 'Development version set to v%s; run scripts/check-versioning.sh after pushing main and development.\n' "$next"
