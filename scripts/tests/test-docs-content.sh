#!/usr/bin/env bash
set -Eeuo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
python3 - "$root" <<'PY'
import re
import sys
from pathlib import Path
root = Path(sys.argv[1])
catalog = (root / 'docs-site/src/catalog.ts').read_text()
entries = re.findall(r"id: '([^']+)'.*?file: '([^']+)'", catalog)
if len(entries) < 15:
    raise SystemExit(f'Expected at least 15 navigable pages, found {len(entries)}')
for page_id, file_name in entries:
    for lang in ('fa', 'en'):
        path = root / 'docs' / lang / f'{file_name}.md'
        if not path.is_file():
            raise SystemExit(f'Missing {lang} article for {page_id}: {path}')
        content = path.read_text().strip()
        if len(content) < 500 or not re.search(r'^## ', content, re.M):
            raise SystemExit(f'Article is unexpectedly short or has no section headings: {path}')
print(f'Bilingual documentation content checks passed ({len(entries)} pages).')
PY
