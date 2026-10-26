## Interactive terminal

The web terminal uses a real PTY so shells and interactive programs behave as expected. Multiple independent tabs can be opened and named. Switching panel pages preserves terminal state in the current browser session; a full browser refresh can disconnect WebSockets.

## Per-tab tools

- Create/close tabs, rename them, and reconnect to a session.
- Copy selected text and paste from the clipboard; browser or mobile clipboard permission may be required.
- `Ctrl+Shift+C` to copy and `Ctrl+Shift+V` to paste where the browser delivers those shortcuts to the page. Browser/OS shortcuts may take precedence.
- Clearing the viewport only clears the screen; it does not erase server-side shell history.
- Resize and scroll within the terminal; mouse-wheel input should scroll terminal history.

## Root Terminal

Root Terminal can fully alter or destroy the host. It requires an administrator, explicit warning confirmation, and re-authentication. The grant is short-lived, one-time, and tied to the current account/session. Close root sessions when finished and review pasted commands before running them.

There is no fixed UI limit on the number of root tabs, but each shell consumes resources. Close unused sessions. Never execute a command merely because it appears in documentation.

## Connection loss

If the WebSocket disconnects, check the reconnect message and reason. The server shell may still exist or may have been closed by timeout. Check processes/services from a separate SSH session first; repeatedly reconnecting can create extra tabs.

## Tabs and connection

The terminal may preserve the previously open tab after navigation. Use **+** for a new terminal. Each tab is an independent PTY session; rename it so production and staging are easy to distinguish. Closing a tab ends that session; visiting another page does not necessarily close it. Reconnect only when the tab is disconnected, and verify a fresh prompt before sending commands.

Clipboard actions use the browser/system clipboard and may require permission. Paste can insert or immediately execute a command; inspect clipboard contents before Ctrl+Shift+V. Select text and use the action or documented shortcut to copy. Copying passwords or private keys to clipboard can expose them. Scroll within the terminal area; while an interactive process is running, arrow keys control that process.

### Regular and Root Terminal

The regular terminal runs with the configured ARG service/user permissions; whether `sudo` works depends on host policy and is not guaranteed by the UI. Open **Terminal Root** as a separate tab only when elevated access is necessary. Its warning is serious: root commands can destroy system files and services without restriction. Do not use Root for routine browsing or remote script execution.

### Safe example

To inspect Nginx, run an appropriate read-only status command and review the output. Before changing a configuration file, create a backup, edit carefully, run `nginx -t`, and reload only if it passes. Avoid destructive commands such as `rm -rf`, `mkfs`, `iptables -F`, or pipe-to-shell unless you understand the effect and rollback. Terminal output may contain tokens or sensitive paths; redact it before sharing.
