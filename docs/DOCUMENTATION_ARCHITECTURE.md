# Documentation architecture and contributor guide

The public docs site is a Vite + React application in `docs-site/`. Markdown remains in `docs/fa/` and `docs/en/`; each article has a stable `id` and a source `file` in `docs-site/src/catalog.ts`. Keep the two languages aligned and preserve IDs because published links use `?page=<id>`.

## Add or update an article

1. Add `docs/fa/<file>.md` and `docs/en/<file>.md` with matching, accurate content. Keep commands, paths, and configuration literals unchanged between translations.
2. Add one typed entry to `pages` in `src/catalog.ts`. Use a stable, descriptive kebab-case ID, a semantic `group`, localized `title` and `summary`, and the shared file basename.
3. Add its topic mapping in `sectionFor` and a bilingual label in `sectionTitles` when it introduces a new subsection. Top-level categories include getting started, monitoring, networking (including Cloudflare DNS), Docker, files and data, application platform, system administration, security and maintenance, and developers and reference. Subsections include monitoring, network, DNS providers, federation, firewall, Nginx, Docker, files, S3, databases, application registry, runtimes and runners, deployments, services, terminal, accounts, audit, settings, updates, maintenance, security, and troubleshooting.
4. `categoryOrder` controls top-level order. The order of entries in `pages` controls article order inside each generated section. `readingOrder` derives previous/next order from the tree.
5. Keep titles and page sections meaningful, and link to the canonical article instead of duplicating setup instructions.

Before adding a new page, search both language folders and the catalog for an existing article that already covers the task. The retired `guide-*` pages were repetitive generated one-task stubs; their useful topic coverage belongs in the canonical bilingual articles. Merge related steps under focused headings when they serve the same reader and task. Keep a separate page only when it answers an independent question with enough verified material to be useful on its own. Do not restore generic filler pages to reach a page-count target.

## Categories and nested navigation

Top-level categories are semantic IDs in `categoryOrder`, with bilingual names in `groups`. `navigation` derives each category's subsection and article children from `pages`, `sectionFor`, and `sectionTitles`. The rendered hierarchy is capped at category → subsection → article. Do not add an article-only icon or invent features to fill the tree. Existing stable IDs should not be renamed; if a rename is unavoidable, add an explicit alias in `getInitialPage` and keep the old ID resolving to its replacement.

The sidebar is an exclusive accordion: at most one top-level category and one subsection are open at a time. The current page's ancestors open automatically, and the preference is stored in `localStorage` under `arg-docs-expanded`. The desktop sidebar is docked to the page's logical edge (right in Persian RTL), uses a solid surface and square corners, and scrolls independently. Keep disclosure controls keyboard-operable and preserve their `aria-expanded` / `aria-controls` relationships.

## Tutorials

Use an existing article as a focused tutorial when it describes a real user outcome. Organize it around goal, prerequisites, required permissions, numbered steps, verification, likely failures, and next steps. Keep a short tutorial on one page with headings; split it only where the tasks stand independently. Do not treat clicking Next as proof that a server operation succeeded.

## Links, language, and routing

Internal links use `?page=<stable-id>` and optional `#<heading-id>`. This query route works on GitHub Pages for direct links, refresh, and browser history without a server-side rewrite. Language switching preserves the page ID when a translation exists. Both localized Markdown files are required for catalog entries; missing content should be fixed rather than hidden behind a silent fallback.

The article renderer removes a Markdown H1 that duplicates the catalog title and turns any remaining H1 into H2, leaving one page-level H1. It generates stable, duplicate-safe IDs from headings and adds a `¶` permalink to the page title and every Markdown heading from H2 through H6. Keep headings unique and descriptive; links to them use the generated ID. Retired `?page=guide-…` IDs are resolved by the legacy prefix map in `catalog.ts` to the canonical bilingual topic, and the browser URL is replaced with the canonical ID.

## Search, version, and validation

The predev/prebuild scripts generate `public/search-index.fa.json` and `public/search-index.en.json` from the canonical catalog Markdown only. They also copy each canonical article into `public/content/<locale>/`; the reader fetches one Markdown article at a time under the configured Pages base path. These generated files are ignored by Git and rebuilt locally/CI. Update the generator if document IDs or heading-anchor rules change. `src/version.ts` is generated from the package version in the root `Cargo.toml`; do not hardcode a second release number in the UI.

Run:

```sh
cd docs-site
npm ci
npm run check
npm run build
cd ..
bash scripts/tests/test-docs-content.sh
```

The build validates and emits static search indexes and article assets for the `/arg/` base path. `.github/workflows/docs-pages.yml` runs on documentation changes to both `main` and `development` and deploys `docs-site/dist` to the repository's Pages site. Keep generated files out of source control; the build creates them from Markdown. `docs/DOCUMENTATION_INVENTORY.csv` records catalog coverage and translation pairs; regenerate or update it whenever entries move or change.
