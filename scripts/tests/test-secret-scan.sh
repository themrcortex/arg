#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tool_version="8.30.1"
archive_sha256="551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb"
archive_url="https://github.com/gitleaks/gitleaks/releases/download/v${tool_version}/gitleaks_${tool_version}_linux_x64.tar.gz"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/arg-secret-scan.XXXXXX")"
trap 'rm -rf -- "$tmp_dir"' EXIT

curl --fail --location --silent --show-error --connect-timeout 5 --max-time 60 \
  --output "$tmp_dir/gitleaks.tar.gz" "$archive_url"
printf '%s  %s\n' "$archive_sha256" "$tmp_dir/gitleaks.tar.gz" | sha256sum --check --status
tar --extract --gzip --file "$tmp_dir/gitleaks.tar.gz" --directory "$tmp_dir" gitleaks

cd "$root"
"$tmp_dir/gitleaks" git --redact --no-banner --timeout 180 \
  --report-format json --report-path "$tmp_dir/report.json" .
printf 'Gitleaks %s scan passed (output redacted, verified release checksum).\n' "$tool_version"
