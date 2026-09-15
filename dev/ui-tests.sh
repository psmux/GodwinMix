#!/usr/bin/env bash
#
# The DOM harness, against a real core, in a real browser, with no toolchain.
#
# Starts a core on a free port with `GMX_UI_DEV=1`, opens `/test/` in headless
# Chrome, and reads the result out of the page over the DevTools protocol. The
# page's own suites run first (selection, meters, the legacy adapter, the
# number keys, the designer kits against their fixtures), then the live suite
# drives the real Scenes panel and the real composer over `/rpc`: two inputs
# dragged onto empty space, F2, a colour, a take, a layout pasted onto another
# scene, a drag in the composer with its timings, Apply, Delete and undo.
#
# The number key suite is the one that covers 1 to 9. It asserts the map still
# binds them to `tray.take-slot` and `0` to `program.black`, and it drives the
# tray's `takeSlot` to prove a number counts the scenes when the collection has
# any and the inputs when it has none.
#
# Usage: dev/ui-tests.sh [--keep] [--show]
#   --keep   leave the working directory and the core's log behind
#   --show   run Chrome with a window, to watch it
#
# Needs: cargo, python3, curl, node and Google Chrome.

set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KEEP=0
HEADLESS="--headless=new"
for arg in "$@"; do
    [[ "$arg" == "--keep" ]] && KEEP=1
    [[ "$arg" == "--show" ]] && HEADLESS=""
done

CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
if [[ ! -x "$CHROME" ]]; then
    CHROME="$(command -v google-chrome || command -v chromium || command -v chromium-browser || true)"
fi
if [[ -z "$CHROME" || ! -x "$CHROME" ]]; then
    echo "no Chrome found. Set CHROME to the browser binary." >&2
    exit 1
fi

WORK="$(mktemp -d "${TMPDIR:-/tmp}/gmx-ui.XXXXXX")"
LOG="$WORK/core.log"
TOKEN="ui-$RANDOM$RANDOM"
CORE_PID=""
CHROME_PID=""

cleanup() {
    [[ -n "$CHROME_PID" ]] && kill "$CHROME_PID" 2>/dev/null
    if [[ -n "$CORE_PID" ]] && kill -0 "$CORE_PID" 2>/dev/null; then
        kill "$CORE_PID" 2>/dev/null
        # Politely, then not. A core wedged on its way out is still a core
        # holding a port and a few percent of a CPU when the next run starts.
        for _ in 1 2 3 4 5 6 7 8 9 10; do
            kill -0 "$CORE_PID" 2>/dev/null || break
            sleep 0.5
        done
        kill -9 "$CORE_PID" 2>/dev/null
        wait "$CORE_PID" 2>/dev/null
    fi
    if [[ $KEEP -eq 1 ]]; then
        echo "kept: $WORK"
    else
        rm -rf "$WORK"
    fi
}
trap cleanup EXIT

free_port() {
    python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()'
}

PORT="$(free_port)"
BASE="http://127.0.0.1:$PORT"

echo "building"
(cd "$REPO" && cargo build --quiet) >"$WORK/build.log" 2>&1 || {
    echo "cargo build failed, see $WORK/build.log" >&2
    KEEP=1
    exit 1
}

cat >"$WORK/godwinmix.toml" <<TOML
[control]
bind = "127.0.0.1:$PORT"
token = "$TOKEN"

[safety]
# The harness takes several times in as many seconds, which is what the
# shipped hold exists to refuse. The hold has its own tests.
min_hold_ms = 0
flash_guard = false
TOML

echo "starting a core on $BASE"
# `exec`, and it matters. Without it the subshell is one process and the core
# is its child, `$!` names the subshell, and the cleanup below kills the
# subshell and leaves the core running: two orphaned mixers at forty percent
# of a core each were found on this machine after a few runs of this script.
(cd "$WORK" && GMX_UI_DEV=1 exec "$REPO/target/debug/godwinmix" --config "$WORK/godwinmix.toml") >"$LOG" 2>&1 &
CORE_PID=$!
for _ in $(seq 1 120); do
    curl -fsS -H "Authorization: Bearer $TOKEN" "$BASE/api/v1/core/info" >/dev/null 2>&1 && break
    kill -0 "$CORE_PID" 2>/dev/null || break
    sleep 0.25
done
if ! curl -fsS -H "Authorization: Bearer $TOKEN" "$BASE/api/v1/core/info" >/dev/null 2>&1; then
    echo "the core never answered; see $LOG" >&2
    KEEP=1
    exit 1
fi

# Chrome is left running and driven over the DevTools protocol, because the
# harness has real work to do after the page loads: it talks to the core that
# served it. `--dump-dom` prints the page before any of that has happened, and
# `--virtual-time-budget` races the page's own waits past a socket that is
# still connecting.
DEBUG_PORT="$(free_port)"
echo "running the harness"
"$CHROME" $HEADLESS \
    --disable-gpu \
    --no-first-run \
    --no-default-browser-check \
    --disable-background-networking \
    --disable-component-update \
    --user-data-dir="$WORK/chrome" \
    --window-size=1400,900 \
    --remote-debugging-port="$DEBUG_PORT" \
    about:blank >"$WORK/chrome.log" 2>&1 &
CHROME_PID=$!

node "$REPO/dev/browser-run.mjs" "$DEBUG_PORT" "$BASE/test/?token=$TOKEN" 180
RESULT=$?

if [[ $RESULT -ne 0 ]]; then
    KEEP=1
    echo "the core's log is at $LOG and Chrome's at $WORK/chrome.log" >&2
fi
exit $RESULT
