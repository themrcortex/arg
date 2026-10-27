# Static site publishing and SSR applications

This guide explains how ARG publishes React, Vue, and Vite output or runs Next.js, Nuxt, and general Node.js applications from the Applications page. The selected framework determines whether Nginx serves files directly or a Node process handles web requests.

## Choose a delivery mode

When creating or editing an app, select **Auto detect** or choose a framework explicitly. Auto mode reads the project manifest:

| Detected framework | Deployment behavior |
| --- | --- |
| React, Vue, or Vite | Run `scripts.build` and publish the output with Nginx; no Node web service is started |
| Next.js or Nuxt | Install dependencies and build, then run SSR on loopback in a systemd service using `scripts.start` or the standard framework entrypoint |
| Generic Node.js | Require `scripts.start`; run a restricted systemd service behind an Nginx reverse proxy |
| Prebuilt static files | Publish a directory containing `index.html`; Node is not required |

Manually selecting React SPA, Vue SPA, Vite SPA, or prebuilt static files forces static delivery. Selecting Next.js SSR, Nuxt SSR, or Node.js SSR forces runtime delivery. Auto mode classifies React/Vue/Vite dependencies as static and Next/Nuxt or a generic app with `start` as SSR.

## Output directory and SPA fallback

**Static output path** is relative to the release and defaults to `auto`. Auto mode checks framework-specific paths: React commonly uses `build`, Vite/Vue use `dist`, Next export uses `out`, and Nuxt uses `.output/public`. The directory must contain a regular `index.html`. Enter a custom relative path such as `web/public-site` when needed. `.` means the release root itself contains `index.html`.

For prebuilt static content, put the final files in the selected path in the source directory or ZIP. You can use a custom path such as `customer/site`; in auto mode ARG checks that path for prebuilt content when there is no `package.json`. If the project has `package.json`, one supported lockfile, and `scripts.build`, ARG installs dependencies in a constrained build environment and runs the build. Otherwise, ARG treats the source as prebuilt and requires an `index.html`; it does not run an arbitrary package manager command.

Enable SPA fallback when browser-side routing uses paths such as `/settings/profile`; Nginx then serves `index.html` for those routes. Disable it for conventional multi-page sites so unknown paths return `404`.

## Node versions and build scripts

Node-based projects use versions reported by the selected VPS runtime inventory. Build and runtime versions are separate. React/Vue/Vite need Node at build time, but no Node server remains after publication. Next/Nuxt and generic Node apps also need Node at runtime.

ARG requires exactly one supported lockfile: `package-lock.json`, `pnpm-lock.yaml`, or a Yarn Classic `yarn.lock`. Conflicting lockfiles and Yarn Berry are rejected. `scripts.build` and `scripts.start` are read from the project manifest; they are not arbitrary strings submitted by the UI. If `scripts.start` is missing, Next uses its standard `next start` CLI and Nuxt uses `.output/server/index.mjs`; these entrypoints are bound to the configured loopback address and port. The service sets `HOST`, `PORT`, `NITRO_HOST`, and `NITRO_PORT` for local binding. The **Build-time public variables** form accepts only `VITE_`, `NEXT_PUBLIC_`, `NUXT_PUBLIC_`, `REACT_APP_`, or `PUBLIC_` prefixes and passes the values to the build step only; install, runtime, and the private `.env` do not receive them. Common frameworks embed these values in browser-delivered JavaScript, so never enter a password, token, or credential. Dependency lifecycle scripts run only in a temporary systemd unit with a DynamicUser, resource/time limits, and a restricted filesystem.

## Domain and Nginx

Set the app domain to let ARG create a reverse proxy or publish a static site. The name must not conflict with an existing app or Nginx site. Static publication creates an app-specific site whose root follows `current/<output>`. Rollback recalculates the Nginx path for the selected previous release. ARG validates the output and Nginx configuration before reporting success; on failure it restores the release pointer.

Assets with hash-like names containing at least eight hexadecimal characters get a long immutable cache policy. Hidden files (except permitted challenge paths) and sensitive extensions such as `.env`, `.ini`, `.log`, `.sql`, and `.zip` are denied. Initial site configuration uses HTTP. After validating the site, issue a certificate from the Nginx SSL tools. Static apps do not receive a web systemd service and do not require an application port.

## Deploy, verify, and roll back

From an app card, select a public Git repository, ZIP, or allowed VPS directory. Every deployment creates a separate release, validates its source, runs a build when applicable, and checks for `index.html` before changing the active pointer or Nginx. Job steps are persisted and remain available after a browser refresh.

For SSR, ARG restarts the Node service only if it was already active, then verifies the local listener and HTTP response. Static deployment updates Nginx without starting a process. Rollback activates the previous release; static apps also get their Nginx output path recalculated. Rollback does not undo external side effects or database migrations.

## Troubleshooting

- **Output not found:** Inspect the release and set the output path to a directory containing `index.html`; paths are case-sensitive.
- **Build script missing:** Select prebuilt static files or define `scripts.build` in the project manifest.
- **Conflicting lockfiles:** Keep only the lockfile for the package manager selected by the repository.
- **Domain already used:** Review the existing app or Nginx site. ARG does not overwrite configuration it does not own.
- **SSR did not start:** Check `scripts.start` or the standard Next/Nuxt entrypoint, the requested runtime version, an unused `listen_port`, and the systemd logs. ARG stops the service if it listens outside loopback.
- **SPA routes fail on refresh:** Enable SPA fallback and deploy again.

## Current boundaries

This implementation supports prebuilt static files, common SPA outputs, standard Next/Nuxt entrypoints, and general SSR applications launched by `scripts.start`. Custom Next/Nuxt adapters, CDN management, wildcard domains, secure build-time secret injection, and full disposable-host validation remain separate work. Passing local tests does not replace testing Nginx and systemd on a Debian/Ubuntu VPS.
