# ARG Documentation Site

The public, bilingual documentation portal is built with Vite, React, TypeScript, Tailwind CSS, and local shadcn-style primitives. Markdown sources live in `../docs/fa` and `../docs/en` so they remain readable directly in GitHub and can be reviewed alongside code.

## Local preview

```bash
npm ci
npm run check
npm run dev
```

Vite prints a local URL. Production builds use the GitHub Pages project base `/arg/`:

```bash
npm run build
```

## Publishing

`.github/workflows/docs-pages.yml` builds and deploys the site to GitHub Pages on changes to `docs/**` or `docs-site/**` on `main` or `development`. Both branches publish to the repository's Pages site at `https://themrcortex.github.io/arg/`; the repository Pages source must be set to **GitHub Actions** in repository settings.

## Add or update a guide

1. Edit both `../docs/fa/<page>.md` and `../docs/en/<page>.md`. The predev/prebuild scripts copy catalog Markdown into `public/content/<locale>/` for one-article-at-a-time loading; generated copies are ignored by Git.
2. Add/adjust the page entry and navigation group in `src/catalog.ts`.
3. Keep heading levels ordered (`##`, then `###`), use fenced code blocks, and clearly label unavailable or risky behavior.
4. Run `npm run check`, `npm run build`, and `bash ../scripts/tests/test-docs-content.sh` from the repository root or respective directory.

Do not put secrets, live host data, private keys, customer details, or unverified feature claims in public docs.
