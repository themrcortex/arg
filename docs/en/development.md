## Repository layout

```text
src/                  Rust + Axum API and system logic
src/s3/               S3 connections, bucket/object/mount operations
src/files.rs          File and ZIP operations
src/nginx/            Nginx site and config manager
frontend/src/         React + TypeScript + Tailwind + shadcn-style UI
frontend/dist/        Embedded UI build, tracked in Git
docs/fa/, docs/en/    Official bilingual Markdown
docs-site/            Documentation website, Vite + React + Tailwind
install.sh            Installer and SSH management menu
scripts/              Tests, versioning, and update helpers
```

## Local development

Requirements: stable Rust, Node.js 22 (or a version compatible with the lockfile), npm, and system build dependencies for the selected target.

```bash
npm ci --prefix frontend
npm run check --prefix frontend
npm run build --prefix frontend
cargo check --locked
cargo test --locked
cargo build --locked --release
bash -n install.sh scripts/update-helper.sh
```

`frontend/dist` is part of deployment; rebuild it from the matching React source before building the Rust binary.

### Documentation checks

```bash
npm ci --prefix docs-site
npm run check --prefix docs-site
npm run build --prefix docs-site
bash scripts/tests/test-docs-content.sh
```

## Change guidelines

- Add APIs to appropriate modules instead of growing large route/page files when a natural module boundary exists.
- Write endpoints need admin authorization, origin validation, input validation, size/time limits, and audit coverage.
- Never interpolate user input into shell strings. Use fixed `Command::new` arguments or a structured helper protocol.
- Do not store secrets in plaintext DB, frontend, query strings, process arguments, logs, or Git.
- SQLite migrations must be idempotent on existing databases and preserve user data.
- Report unsupported measurements as unavailable; never substitute demo data for real measurements.
- Test Persian RTL, English LTR, mobile, keyboard, reduced motion, and loading/error/empty states.

## Version and release

`main` follows stable SemVer; `Cargo.toml`, `frontend/package.json`, the lockfile, and displayed version should agree. Use `development` for prereleases. Development must include current main in ancestry and use the prerelease version based on the next patch.

```bash
# on main, after tests and changelog updates
scripts/bump-version.sh patch

# on development, after merging main
scripts/bump-development-version.sh

scripts/check-versioning.sh
```

Every functional change needs the appropriate changelog entry. Stable GitHub releases use a tag on main and the repository's release workflow. Update both language docs in the same PR/commit as related code changes.

## Contributing

Issues should include goal, version/commit, OS, and reproduction steps. Keep PRs focused; include tests and explain security/data impact. Do not submit destructive changes without preflight and recovery plans. Documentation stays in this repository so code and docs are versioned together.
