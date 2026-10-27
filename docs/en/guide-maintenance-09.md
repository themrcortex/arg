# ARG Doctor and mobile offline shell

Use this guide to inspect the read-only host report and understand what ARG stores for offline use.

## Run ARG Doctor

1. Sign in as an administrator.
2. Open **Server & ARG management** from the sidebar.
3. Review the **ARG Doctor** card. It runs on page entry and can be refreshed manually.
4. Read the status, measured value, and limitation under every check; a green result does not certify a complete security audit.

Doctor reports the OS family string, presence of systemd, Docker executable/socket, root filesystem available bytes, panel certificate/key file presence, selected executable paths, locally stored S3 connection/mount counts, last-known node connectivity, and application/deployment record counts.

Doctor is read-only. It does not perform DNS/TLS handshakes, contact S3 or nodes, run Docker/systemd commands, install a runtime, repair a file, restart services, or reveal a private key path/content. For current remote status, use the relevant S3 or Nodes screen and run its explicit check there.

The OS result is a family hint, not a verified support matrix. “Docker installed” checks executable/socket presence and does not prove the daemon or a container works. S3 and node results come from the last state saved by ARG and can be stale. “Runtime found” only means an executable file exists; it does not confirm version compatibility or application readiness.

## Install the ARG PWA shell

Open ARG over HTTPS in a supported mobile browser. Use the browser’s **Add to Home Screen / Install app** action when offered. ARG caches the navigation shell and a bounded set of same-origin, fingerprinted frontend assets.

The service worker bypasses `/api` completely. It does not cache metrics, account data, server responses, uploads, or pending actions. When offline, ARG can display its cached shell and a notice, but live server details and operations remain unavailable. There is no background write queue; do not expect an offline change to sync later.

When you sign out, ARG clears its versioned shell cache. If offline startup fails, reconnect and reload the page to refresh the shell.

## Verification

- With the server online, load ARG and wait for the shell and page assets to finish loading.
- Use browser developer tools to go offline. Reload: the application shell should appear when its cache is available, with the offline notice.
- Check that dashboard, S3, and other `/api` calls fail as unavailable rather than showing cached server data.
- Sign out while online, then inspect Cache Storage: the `arg-shell-*` cache should be removed.
- Return online and reload to recover live data.
