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
