#!/usr/bin/env bash
set -Eeuo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
python3 - "$root" <<'PY'
import re
import sys
import unicodedata
from pathlib import Path
from urllib.parse import parse_qs, unquote, urlsplit

root = Path(sys.argv[1])
catalog_path = root / 'docs-site/src/catalog.ts'
catalog = catalog_path.read_text()
entries = re.findall(r"\{ id: '([^']+)', group: '([^']+)', title: \{.*?\}, summary: \{.*?\}, file: '([^']+)' \}", catalog)
if len(entries) < 28:
    raise SystemExit(f'Expected at least 28 canonical documentation pages, found {len(entries)}')
page_ids = [page_id for page_id, _, _ in entries]
if len(page_ids) != len(set(page_ids)):
    raise SystemExit('Documentation page IDs must be unique')
by_file = {file_name: page_id for page_id, _, file_name in entries}
legacy = dict(re.findall(r"'(guide-[\w-]*-)': '([^']+)'", catalog))
for prefix, target in legacy.items():
    if target not in page_ids:
        raise SystemExit(f'Legacy alias target does not exist: {prefix} -> {target}')

heading_re = re.compile(r'^#{1,6}\s+(.+?)\s*#*\s*$', re.M)
link_re = re.compile(r'(?<!!)\[[^\]]*\]\(([^)\s]+)(?:\s+[^)]*)?\)')
def heading_ids(content):
    used = {}
    ids = set()
    for match in heading_re.finditer(content):
        heading = match.group(1).strip()
        heading = re.sub(r'`([^`]*)`', r'\1', heading)
        heading = re.sub(r'\[([^\]]+)\]\([^)]+\)', r'\1', heading)
        heading = re.sub(r'<[^>]*>', '', heading)
        heading = re.sub(r'[*_~]', '', heading)
        value = unicodedata.normalize('NFKD', heading.lower())
        value = ''.join(char for char in value if not (0x300 <= ord(char) <= 0x36f))
        chars = []
        pending_dash = False
        for char in value:
            if char.isalpha() or char.isdigit():
                if pending_dash and chars:
                    chars.append('-')
                chars.append(char)
                pending_dash = False
            else:
                pending_dash = True
        slug = ''.join(chars).strip('-') or 'section'
        count = used.get(slug, 0)
        used[slug] = count + 1
        ids.add(f'{slug}-{count + 1}' if count else slug)
    return ids

def resolve_alias(value):
    if value in page_ids:
        return value
    prefix = next((candidate for candidate in legacy if value.startswith(candidate)), None)
    return legacy.get(prefix) if prefix else None

errors = []
markdown_cache = {}
for page_id, file_name in by_file.items():
    for lang in ('fa', 'en'):
        path = root / 'docs' / lang / f'{file_name}.md'
        if not path.is_file():
            errors.append(f'Missing {lang} article for {page_id}: {path.relative_to(root)}')
            continue
        content = path.read_text()
        markdown_cache[(lang, file_name)] = content
        if len(content.strip()) < 500 or not re.search(r'^## ', content, re.M):
            errors.append(f'Article is unexpectedly short or has no section headings: {path.relative_to(root)}')
        ids = heading_ids(content)
        for match in link_re.finditer(content):
            href = match.group(1).strip('<>')
            previous = content[max(0, match.start() - 1):match.start()]
            if previous == '!':
                continue
            parts = urlsplit(href)
            if parts.scheme.lower() in {'http', 'https', 'mailto', 'tel'} or parts.netloc:
                continue
            fragment = unquote(parts.fragment)
            if parts.path.startswith('?') or (not parts.path and parts.query):
                query = parse_qs(parts.query if parts.query else parts.path[1:])
                target = (query.get('page') or [''])[0]
                if not resolve_alias(target):
                    errors.append(f'Broken page link in {path.relative_to(root)}: {href}')
                    continue
                target_page = resolve_alias(target)
                target_file = next(file for candidate, file in by_file.items() if candidate == target_page)
                target_lang = (query.get('lang') or [lang])[0]
                target_content = markdown_cache.get((target_lang, target_file))
                if target_content is None:
                    target_content = (root / 'docs' / target_lang / f'{target_file}.md').read_text() if target_lang in {'fa', 'en'} and (root / 'docs' / target_lang / f'{target_file}.md').is_file() else ''
                if fragment and target_content and fragment not in heading_ids(target_content):
                    errors.append(f'Broken heading link in {path.relative_to(root)}: {href}')
                continue
            if not parts.path:
                if fragment and fragment not in ids:
                    errors.append(f'Broken heading link in {path.relative_to(root)}: {href}')
                continue
            decoded_path = unquote(parts.path)
            if decoded_path.lower().endswith('.md'):
                target_name = Path(decoded_path).name[:-3]
                if target_name not in by_file:
                    errors.append(f'Broken Markdown page link in {path.relative_to(root)}: {href}')
                    continue
                segments = Path(decoded_path).parts
                target_lang = next((segment for segment in segments if segment in {'fa', 'en'}), lang)
                target_path = root / 'docs' / target_lang / f'{target_name}.md'
                if not target_path.is_file():
                    errors.append(f'Broken localized link in {path.relative_to(root)}: {href}')
                    continue
                target_ids = heading_ids(target_path.read_text())
                if fragment and fragment not in target_ids:
                    errors.append(f'Broken heading link in {path.relative_to(root)}: {href} (no #{fragment})')
                continue
            if decoded_path.startswith('/'):
                # Public site routes/assets are not local documentation source paths.
                continue
            local_target = (path.parent / decoded_path).resolve()
            if not local_target.exists():
                errors.append(f'Broken local link in {path.relative_to(root)}: {href}')

for lang in ('fa', 'en'):
    leftovers = list((root / 'docs' / lang).glob('guide-*.md'))
    if leftovers:
        errors.append(f'One-task guides should be merged into canonical pages; found {len(leftovers)} {lang} files')
semantic_groups = {group for _, group, _ in entries}
if any(group.isdigit() for group in semantic_groups):
    errors.append('Documentation categories must use semantic IDs, not numeric group IDs')
expected_categories = {'gettingStarted', 'dashboard', 'network', 'docker', 'data', 'system', 'security', 'reference', 'applications'}
if not expected_categories.issubset(semantic_groups):
    errors.append(f'Missing semantic navigation categories: {sorted(expected_categories - semantic_groups)}')
if errors:
    raise SystemExit('\n'.join(errors))
print(f'Bilingual documentation checks passed ({len(entries)} canonical pages, {len(entries) * 2} localized articles; internal page and heading links resolve; {len(legacy)} legacy guide prefixes resolve).')
PY
