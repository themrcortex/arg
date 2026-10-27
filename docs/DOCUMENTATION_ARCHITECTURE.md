# Documentation architecture and contributor guide

The public docs site is a Vite + React application in `docs-site/`. Markdown remains in `docs/fa/` and `docs/en/`; each article has a stable `id` and a source `file` in `docs-site/src/catalog.ts`. Keep the two languages aligned and preserve IDs because published links use `?page=<id>`.

## Add or update an article

1. Add `docs/fa/<file>.md` and `docs/en/<file>.md` with matching, accurate content. Keep commands, paths, and configuration literals unchanged between translations.
2. Add one typed entry to `pages` in `src/catalog.ts`. Use a stable, descriptive kebab-case ID, a semantic `group`, localized `title` and `summary`, and the shared file basename.
3. Add its topic mapping in `sectionFor` and a bilingual label in `sectionTitles` when it introduces a new subsection. Existing sections include monitoring, network, firewall, Nginx, Docker, files, S3, databases, services, terminal, accounts, audit, settings, updates, maintenance, security, and troubleshooting.
4. `categoryOrder` controls top-level order. The order of entries in `pages` controls article order inside each generated section. `readingOrder` derives previous/next order from the tree.
5. Keep titles and page sections meaningful, and link to the canonical article instead of duplicating setup instructions.

## Categories and nested navigation

Top-level categories are semantic IDs in `categoryOrder`, with bilingual names in `groups`. `navigation` derives each category's subsection and article children from `pages`, `sectionFor`, and `sectionTitles`. The rendered hierarchy is capped at category → subsection → article. Do not add an article-only icon or invent features to fill the tree. Existing stable IDs should not be renamed; if a rename is unavoidable, add an explicit alias in `getInitialPage` and keep the old ID resolving to its replacement.

## Tutorials

Use an existing article as a focused tutorial when it describes a real user outcome. Organize it around goal, prerequisites, required permissions, numbered steps, verification, likely failures, and next steps. Keep a short tutorial on one page with headings; split it only where the tasks stand independently. Do not treat clicking Next as proof that a server operation succeeded.

## Links, language, and routing

Internal links use `?page=<stable-id>` and optional `#<heading-id>`. This query route works on GitHub Pages for direct links, refresh, and browser history without a server-side rewrite. Language switching preserves the page ID when a translation exists. Both localized Markdown files are required for catalog entries; missing content should be fixed rather than hidden behind a silent fallback.

## Search, version, and validation

The predev/prebuild scripts generate `public/search-index.fa.json` and `public/search-index.en.json` from Markdown. The browser fetches the current language index only when search is used. Update these scripts if document IDs or heading-anchor rules change. `src/version.ts` is generated from the package version in the root `Cargo.toml`; do not hardcode a second release number in the UI.

Run:

```sh
cd docs-site
npm ci
npm run check
npm run build
cd ..
bash scripts/tests/test-docs-content.sh
```

The build validates and emits static search indexes under the `/arg/` base path. The Pages workflow builds from `main` and deploys `docs-site/dist`. Keep generated search indexes out of source control; the build creates them from Markdown. `docs/DOCUMENTATION_INVENTORY.csv` records catalog coverage and translation pairs; regenerate or update it whenever entries move or change.
