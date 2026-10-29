#!/usr/bin/env bash
set -Eeuo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
helper="$root/scripts/update-helper.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/run"
cat > "$tmp/bin/systemctl" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$ARG_TEST_SYSTEMCTL_LOG"
case "${1:-}" in
  is-active)
    for item in "$@"; do [[ "$item" == arg.service ]] && { [[ "${ARG_TEST_SYSTEMCTL_ACTIVE:-0}" == 1 ]] && exit 0 || exit 1; }; done
    exit 1
    ;;
  stop) exit 0 ;;
  *) exit 0 ;;
esac
MOCK
chmod +x "$tmp/bin/systemctl"
cat > "$tmp/bin/curl" <<'MOCK'
#!/usr/bin/env bash
if [[ "${ARG_TEST_NESTED_UPDATE:-0}" == 1 ]]; then
  if "$ARG_TEST_UPDATE_HELPER" run fast main > "$ARG_TEST_NESTED_UPDATE_LOG" 2>&1; then
    echo "update unexpectedly entered while rollback held the reservation" >&2
    exit 31
  fi
  grep -q 'ARG_UPDATE_IN_PROGRESS' "$ARG_TEST_NESTED_UPDATE_LOG" || { cat "$ARG_TEST_NESTED_UPDATE_LOG" >&2; exit 32; }
fi
[[ "${ARG_TEST_CURL_FAIL:-0}" != 1 ]] || exit 22
exit 0
MOCK
chmod +x "$tmp/bin/curl"
cat > "$tmp/bin/install" <<'MOCK'
#!/usr/bin/env bash
mode=0755
directory=0
paths=()
while (($#)); do
  case "$1" in
    -d) directory=1; shift ;;
    -o|-g) shift 2 ;;
    -m) mode="$2"; shift 2 ;;
    *) paths+=("$1"); shift ;;
  esac
done
if (( directory )); then mkdir -p "${paths[@]}"; exit 0; fi
[[ ${#paths[@]} == 2 ]] || exit 2
cp "${paths[0]}" "${paths[1]}"
chmod "$mode" "${paths[1]}"
MOCK
chmod +x "$tmp/bin/install"
cat > "$tmp/bin/chown" <<'MOCK'
#!/usr/bin/env bash
exit 0
MOCK
chmod +x "$tmp/bin/chown"
cat > "$tmp/bin/systemd-run" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$ARG_TEST_SYSTEMD_RUN_LOG"
exit 0
MOCK
chmod +x "$tmp/bin/systemd-run"
export PATH="$tmp/bin:$PATH"
export ARG_TEST_SYSTEMCTL_LOG="$tmp/systemctl.log"
export ARG_TEST_SYSTEMD_RUN_LOG="$tmp/systemd-run.log"
export ARG_TEST_UPDATE_HELPER="$helper"
export ARG_TEST_NESTED_UPDATE_LOG="$tmp/nested-update.log"
export ARG_UPDATE_LOCK_DIR="$tmp/run/arg-update.lock"
export ARG_UPDATE_STATUS_FILE="$tmp/run/arg-update-in-progress"
export ARG_UPDATE_STATE_FILE="$tmp/run/arg-update-state.json"
export ARG_UPDATE_LAST_UNIT_FILE="$tmp/run/arg-update-last-unit"
export ARG_UPDATE_CANCEL_FILE="$tmp/run/arg-update-cancel-requested"
export ARG_ROLLBACK_DIR="$tmp/rollback"
export ARG_UPDATE_BINARY_DIR="$tmp/lib"
export ARG_UPDATE_BINARY="$tmp/lib/arg"
export ARG_UPDATE_ENV_FILE="$tmp/arg.env"
export ARG_DATABASE_BACKUP_TOOL="$tmp/bin/db-backup"
mkdir -p "$ARG_ROLLBACK_DIR" "$ARG_UPDATE_BINARY_DIR"
printf 'ARG_DB=%s/data.db\n' "$tmp" > "$ARG_UPDATE_ENV_FILE"
cat > "$ARG_DATABASE_BACKUP_TOOL" <<'MOCK'
#!/usr/bin/env bash
[[ "$1" == database-backup ]] || exit 2
cp "$2" "$3"
chmod 0600 "$3"
MOCK
chmod +x "$ARG_DATABASE_BACKUP_TOOL"

# The live update state retains branch and build mode so the UI can describe the
# exact source and compile policy while the job is running.
export ARG_UPDATE_JOB_ID=metadata-test ARG_UPDATE_OPERATION=update ARG_UPDATE_BRANCH=development ARG_UPDATE_MODE=fast ARG_UPDATE_STRATEGY=source
"$helper" state running building "Compiling development branch."
grep -q '"operation":"update"' "$ARG_UPDATE_STATE_FILE"
grep -q '"branch":"development"' "$ARG_UPDATE_STATE_FILE"
grep -q '"build_mode":"fast"' "$ARG_UPDATE_STATE_FILE"
grep -q '"strategy":"source"' "$ARG_UPDATE_STATE_FILE"
unset ARG_UPDATE_JOB_ID ARG_UPDATE_OPERATION ARG_UPDATE_BRANCH ARG_UPDATE_MODE ARG_UPDATE_STRATEGY

# Scheduling persists and forwards the selected strategy to the isolated updater process.
"$helper" run fast development source
grep -q -- '--setenv=ARG_UPDATE_STRATEGY=source' "$ARG_TEST_SYSTEMD_RUN_LOG"
grep -q '"branch":"development"' "$ARG_UPDATE_STATE_FILE"
grep -q '"strategy":"source"' "$ARG_UPDATE_STATE_FILE"
rm -f "$ARG_UPDATE_STATUS_FILE" "$ARG_UPDATE_LAST_UNIT_FILE"
rmdir "$ARG_UPDATE_LOCK_DIR"
if "$helper" run fast main 'source;touch /tmp/arg-update-test' >/dev/null 2>&1; then
  echo 'invalid update strategy was accepted' >&2
  exit 1
fi

# A dead owner and inactive transient unit must clear immediately, without the six-hour wait.
mkdir "$ARG_UPDATE_LOCK_DIR"
printf '%s 99999999 1\n' "$(date +%s)" > "$ARG_UPDATE_STATUS_FILE"
printf '%s\n' 'arg-web-update-99999999' > "$ARG_UPDATE_LAST_UNIT_FILE"
printf '%s\n' '{"job_id":"stale-test","state":"running","phase":"building","started_at":1,"updated_at":1,"message":""}' > "$ARG_UPDATE_STATE_FILE"
"$helper" progress >/dev/null
test ! -e "$ARG_UPDATE_STATUS_FILE"
test ! -d "$ARG_UPDATE_LOCK_DIR"
grep -q '"state":"failed"' "$ARG_UPDATE_STATE_FILE"

# A scheduled update can be cancelled and its lock is released with a terminal state.
mkdir "$ARG_UPDATE_LOCK_DIR"
printf '%s 99999999 1\n' "$(date +%s)" > "$ARG_UPDATE_STATUS_FILE"
printf '%s\n' 'arg-web-update-99999999' > "$ARG_UPDATE_LAST_UNIT_FILE"
printf '%s\n' '{"job_id":"cancel-test","state":"running","phase":"scheduled","started_at":1,"updated_at":1,"message":""}' > "$ARG_UPDATE_STATE_FILE"
"$helper" cancel >/dev/null
test ! -e "$ARG_UPDATE_STATUS_FILE"
test ! -d "$ARG_UPDATE_LOCK_DIR"
grep -q '"state":"cancelled"' "$ARG_UPDATE_STATE_FILE"
grep -q 'stop arg-web-update-99999999.timer arg-web-update-99999999.service' "$ARG_TEST_SYSTEMCTL_LOG"

# Rollback restores the matching database snapshot and retains the displaced release for recovery.
printf 'current-build\n' > "$ARG_UPDATE_BINARY"
printf 'previous-build\n' > "$ARG_ROLLBACK_DIR/arg.previous"
printf 'current-db\n' > "$tmp/data.db"
printf 'previous-db\n' > "$ARG_ROLLBACK_DIR/arg.db.previous"
chmod +x "$ARG_UPDATE_BINARY" "$ARG_ROLLBACK_DIR/arg.previous"

export ARG_TEST_SYSTEMCTL_ACTIVE=1
export ARG_TEST_NESTED_UPDATE=1
"$helper" rollback >/dev/null
unset ARG_TEST_NESTED_UPDATE
grep -q 'previous-build' "$ARG_UPDATE_BINARY"
grep -q 'current-build' "$ARG_ROLLBACK_DIR/arg.previous"
grep -q 'previous-db' "$tmp/data.db"
grep -q 'current-db' "$ARG_ROLLBACK_DIR/arg.db.previous"
test ! -e "$ARG_UPDATE_STATUS_FILE"
test ! -d "$ARG_UPDATE_LOCK_DIR"
grep -q '"state":"succeeded"' "$ARG_UPDATE_STATE_FILE"
grep -q '"phase":"rolled_back"' "$ARG_UPDATE_STATE_FILE"

# Rollback is a protected binary/database transaction and cannot be interrupted
# through the generic update cancellation action.
mkdir "$ARG_UPDATE_LOCK_DIR"
printf '%s 99999999 1\n' "$(date +%s)" > "$ARG_UPDATE_STATUS_FILE"
printf '%s\n' '{"job_id":"rollback-active","state":"running","phase":"installing","operation":"rollback","started_at":1,"updated_at":1,"message":""}' > "$ARG_UPDATE_STATE_FILE"
if "$helper" cancel >/dev/null 2>&1; then
  echo "rollback cancellation unexpectedly succeeded" >&2
  exit 1
fi
test -d "$ARG_UPDATE_LOCK_DIR"
test -s "$ARG_UPDATE_STATUS_FILE"
test ! -e "$ARG_UPDATE_CANCEL_FILE"
rm -f "$ARG_UPDATE_STATUS_FILE"
rmdir "$ARG_UPDATE_LOCK_DIR"

# A rollback that fails its health check restores the currently running binary
# and matching database snapshot, and leaves a clear terminal error state.
printf 'current-good-build\n' > "$ARG_UPDATE_BINARY"
printf 'previous-build\n' > "$ARG_ROLLBACK_DIR/arg.previous"
printf 'current-good-db\n' > "$tmp/data.db"
printf 'previous-db\n' > "$ARG_ROLLBACK_DIR/arg.db.previous"
chmod +x "$ARG_UPDATE_BINARY" "$ARG_ROLLBACK_DIR/arg.previous"
export ARG_TEST_CURL_FAIL=1 ARG_ROLLBACK_HEALTH_ATTEMPTS=1
if "$helper" rollback >/dev/null 2>&1; then
  echo "rollback unexpectedly passed a failed health check" >&2
  exit 1
fi
unset ARG_TEST_CURL_FAIL ARG_ROLLBACK_HEALTH_ATTEMPTS
grep -q 'current-good-build' "$ARG_UPDATE_BINARY"
grep -q 'current-good-db' "$tmp/data.db"
test ! -e "$ARG_UPDATE_STATUS_FILE"
test ! -d "$ARG_UPDATE_LOCK_DIR"
grep -q '"state":"failed"' "$ARG_UPDATE_STATE_FILE"

# A failed rollback also releases its reservation and reports a terminal failure.
rm -f "$ARG_ROLLBACK_DIR/arg.previous"
if "$helper" rollback >/dev/null 2>&1; then
  echo "rollback without a saved binary unexpectedly succeeded" >&2
  exit 1
fi
test ! -e "$ARG_UPDATE_STATUS_FILE"
test ! -d "$ARG_UPDATE_LOCK_DIR"
grep -q '"state":"failed"' "$ARG_UPDATE_STATE_FILE"
printf 'current-build\n' > "$ARG_UPDATE_BINARY"
printf 'previous-build\n' > "$ARG_ROLLBACK_DIR/arg.previous"
chmod +x "$ARG_UPDATE_BINARY" "$ARG_ROLLBACK_DIR/arg.previous"

# Cancelling after the binary swap restores both the last binary and its matching database.
printf 'broken-build\n' > "$ARG_UPDATE_BINARY"
printf 'known-good-build\n' > "$ARG_ROLLBACK_DIR/arg.previous"
printf 'broken-db\n' > "$tmp/data.db"
printf 'known-good-db\n' > "$ARG_ROLLBACK_DIR/arg.db.previous"
mkdir "$ARG_UPDATE_LOCK_DIR"
printf '%s 99999999 1\n' "$(date +%s)" > "$ARG_UPDATE_STATUS_FILE"
printf '%s\n' 'arg-web-update-99999999' > "$ARG_UPDATE_LAST_UNIT_FILE"
printf '%s\n' '{"job_id":"cancel-install-test","state":"running","phase":"installing","started_at":1,"updated_at":1,"message":""}' > "$ARG_UPDATE_STATE_FILE"
"$helper" cancel >/dev/null
grep -q 'known-good-build' "$ARG_UPDATE_BINARY"
grep -q 'known-good-db' "$tmp/data.db"
test ! -e "$ARG_UPDATE_STATUS_FILE"
test ! -d "$ARG_UPDATE_LOCK_DIR"
grep -q '"state":"cancelled"' "$ARG_UPDATE_STATE_FILE"
printf 'update-helper stale-lock, cancellation, and binary/database rollback tests passed\n'
