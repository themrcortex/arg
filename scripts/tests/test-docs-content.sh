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
if len(entries) < 150:
    raise SystemExit(f'Expected at least 150 navigable documentation pages, found {len(entries)}')
page_ids = [page_id for page_id, _ in entries]
if len(page_ids) != len(set(page_ids)):
    raise SystemExit('Documentation page IDs must be unique')
for page_id, file_name in entries:
    for lang in ('fa', 'en'):
        path = root / 'docs' / lang / f'{file_name}.md'
        if not path.is_file():
            raise SystemExit(f'Missing {lang} article for {page_id}: {path}')
        content = path.read_text().strip()
        if len(content) < 500 or not re.search(r'^## ', content, re.M):
            raise SystemExit(f'Article is unexpectedly short or has no section headings: {path}')
        for target in re.findall(r'\]\(\?page=([\w-]+)\)', content):
            if target not in page_ids:
                raise SystemExit(f'Broken documentation page link in {path}: {target}')
for lang in ('fa', 'en'):
    guide_files = list((root / 'docs' / lang).glob('guide-*.md'))
    if len(guide_files) < 150:
        raise SystemExit(f'Expected at least 150 focused {lang} guides, found {len(guide_files)}')
semantic_groups = set(re.findall(r"\{ id: '([^']+)', group: '([^']+)'", catalog))
if any(group.isdigit() for _, group in semantic_groups):
    raise SystemExit('Documentation categories must use semantic IDs, not numeric group IDs')
expected_categories = {'gettingStarted', 'dashboard', 'network', 'docker', 'data', 'system', 'security', 'reference'}
catalog_groups = {group for _, group in semantic_groups}
if not expected_categories.issubset(catalog_groups):
    raise SystemExit(f'Missing semantic navigation categories: {sorted(expected_categories - catalog_groups)}')
print(f'Bilingual documentation content checks passed ({len(entries)} pages, {len(entries) * 2} localized articles).')
PY
