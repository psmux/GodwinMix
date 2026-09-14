#!/usr/bin/env bash
#
# The three integration plugins against a real core.
#
# Starts a core the way dev/smoke.sh does, adds two test sources, then:
#
#   osc       a ten line Python sender sends /program/take, and event/program.took
#             comes back on /rpc with that source on it
#   tally     a Python listener reads one TSL UMD v5 packet and checks the lamp
#             bits and the label
#   director  runs for 30 s against the two sources and takes on the rules
#
# Each plugin is run standalone with --url and --token, which is how a service
# plugin runs until the mixer instantiates them itself.
#
# Usage: dev/integrations-live.sh [--keep] [--only osc|tally|director]
#
# Needs: cargo, python3 (standard library only), curl.

set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KEEP=0
ONLY="all"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --keep) KEEP=1 ;;
        --only) ONLY="${2:-all}"; shift ;;
        *) echo "usage: dev/integrations-live.sh [--keep] [--only osc|tally|director]" >&2; exit 2 ;;
    esac
    shift
done

WORK="$(mktemp -d "${TMPDIR:-/tmp}/gmx-live.XXXXXX")"
LOG="$WORK/core.log"
TOKEN="live-$RANDOM$RANDOM"
CORE_PID=""
PLUGIN_PID=""
FAILED=0

step() { printf '%-58s' "$1"; }
ok() { printf 'ok\n'; }
bad() { printf 'FAIL\n'; printf '    %s\n' "$1" >&2; FAILED=$((FAILED + 1)); KEEP=1; }

stop_plugin() {
    if [[ -n "$PLUGIN_PID" ]] && kill -0 "$PLUGIN_PID" 2>/dev/null; then
        kill "$PLUGIN_PID" 2>/dev/null
        wait "$PLUGIN_PID" 2>/dev/null
    fi
    PLUGIN_PID=""
}

cleanup() {
    stop_plugin
    if [[ -n "$CORE_PID" ]] && kill -0 "$CORE_PID" 2>/dev/null; then
        kill "$CORE_PID" 2>/dev/null
        for _ in 1 2 3 4 5 6 7 8 9 10; do
            kill -0 "$CORE_PID" 2>/dev/null || break
            sleep 0.5
        done
        kill -9 "$CORE_PID" 2>/dev/null
        wait "$CORE_PID" 2>/dev/null
    fi
    if [[ $KEEP -eq 1 ]]; then echo "kept: $WORK"; else rm -rf "$WORK"; fi
}
trap cleanup EXIT

free_port() { python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()'; }

PORT="$(free_port)"
BASE="http://127.0.0.1:$PORT"
AUTH=(-H "Authorization: Bearer $TOKEN")

echo "GodwinMix integrations live test on $BASE"
echo

# --- build ------------------------------------------------------------------

step "build the core and the plugins"
if ! "$REPO/dev/plugins.sh" build >"$WORK/build.log" 2>&1; then
    bad "see $WORK/build.log"
    exit 1
fi
if ! (cd "$REPO" && cargo build --quiet -p godwinmix) >>"$WORK/build.log" 2>&1; then
    bad "see $WORK/build.log"
    exit 1
fi
ok

# --- the core ---------------------------------------------------------------

step "config"
"$REPO/target/debug/godwinmix" --example-config >"$WORK/example.toml" 2>/dev/null
python3 - "$WORK/example.toml" "$WORK/godwinmix.toml" "$PORT" "$TOKEN" <<'PY'
import re, sys
src, dst, port, token = sys.argv[1:5]
text = open(src).read()
out, skipping = [], False
for line in text.splitlines():
    stripped = line.strip()
    if stripped.startswith("[["):
        skipping = stripped in ("[[sources]]", "[[outputs]]")
    elif stripped.startswith("[") and not stripped.startswith("[["):
        skipping = False
    out.append("# " + line if skipping and not line.startswith("#") else line)
text = "\n".join(out) + "\n"
text = re.sub(r'(?m)^bind = .*$', f'bind = "127.0.0.1:{port}"', text)
text = re.sub(r'(?m)^# token = .*$', f'token = "{token}"', text)
text = re.sub(r'(?m)^linger_secs = .*$', 'linger_secs = 2', text)
# The shipped minimum shot length is 8 s, which is right for a service and
# wrong for a test that wants to see several takes in half a minute. The
# mechanism is what is being tested, not the shipped number.
text = re.sub(r'(?m)^min_hold_ms = .*$', 'min_hold_ms = 500', text)
open(dst, "w").write(text)
PY
grep -q "^token = " "$WORK/godwinmix.toml" && ok || { bad "the config was not rewritten"; exit 1; }

step "core starts"
# `exec`, so `$!` names the core and not the subshell around it. Without it
# the cleanup below kills the subshell and leaves the mixer running with its
# port held. See the same line in dev/smoke.sh.
(cd "$WORK" && exec "$REPO/target/debug/godwinmix" --config "$WORK/godwinmix.toml") >"$LOG" 2>&1 &
CORE_PID=$!
for _ in $(seq 1 100); do
    curl -fsS "$BASE/api/v1/core/info" "${AUTH[@]}" >/dev/null 2>&1 && break
    kill -0 "$CORE_PID" 2>/dev/null || break
    sleep 0.2
done
curl -fsS "$BASE/api/v1/core/info" "${AUTH[@]}" >/dev/null 2>&1 && ok || { bad "the core never answered; see $LOG"; tail -20 "$LOG" >&2; exit 1; }

step "two test sources, live"
for id in cam1 cam2; do
    curl -fsS -X POST "$BASE/api/v1/sources" "${AUTH[@]}" -H 'Content-Type: application/json' \
        -d "{\"id\":\"$id\",\"uri\":\"test://smpte\",\"name\":\"$id\"}" >/dev/null 2>&1
done
LIVE=0
for _ in $(seq 1 60); do
    if [[ "$(curl -fsS "$BASE/api/v1/sources" "${AUTH[@]}" | python3 -c 'import json,sys; print(sum(1 for s in json.load(sys.stdin) if s["state"]=="live"))')" == "2" ]]; then
        LIVE=1; break
    fi
    sleep 0.3
done
[[ $LIVE -eq 1 ]] && ok || { bad "the sources never went live; see $LOG"; exit 1; }

# --- osc --------------------------------------------------------------------

run_osc() {
    local osc_port; osc_port="$(free_port)"
    local out_port; out_port="$(free_port)"

    # The listener binds before anything is sent to it. A datagram to a port
    # with nothing on it earns an ICMP unreachable that the sender only learns
    # about on its next send, and a test should not be the thing that finds
    # that out.
    python3 "$REPO/dev/osc-listen.py" "$out_port" --for 20 >"$WORK/osc-out.txt" 2>&1 &
    local listener=$!
    sleep 0.5

    step "gmx-osc listens on $osc_port"
    "$REPO/plugins/osc/bin/gmx-osc" --url "$BASE" --token "$TOKEN" \
        --listen "127.0.0.1:$osc_port" --send-to "127.0.0.1:$out_port" \
        >"$WORK/osc.log" 2>&1 &
    PLUGIN_PID=$!
    for _ in $(seq 1 50); do grep -q "listening for OSC" "$WORK/osc.log" && break; sleep 0.2; done
    grep -q "listening for OSC" "$WORK/osc.log" && ok || { bad "$(cat "$WORK/osc.log")"; kill $listener 2>/dev/null; return; }

    step "an OSC take lands as event/program.took"
    local result
    result="$(python3 "$REPO/dev/osc-take.py" "$BASE" "$TOKEN" "127.0.0.1:$osc_port" cam2 2>&1)"
    if [[ "$result" == "took cam2" ]]; then ok; else bad "$result"; fi

    step "tally and programme come back out as OSC"
    wait $listener 2>/dev/null
    local seen; seen="$(cat "$WORK/osc-out.txt")"
    if [[ "$seen" == *"/gmx/program ['cam2']"* && "$seen" == *"/gmx/tally/cam2"* ]]; then
        ok
        echo "$seen" | head -4 | sed 's/^/    /'
    else
        bad "no /gmx/program and /gmx/tally came back. Saw: $seen"
    fi

    stop_plugin
}

# --- tally ------------------------------------------------------------------

run_tally() {
    local tsl_port; tsl_port="$(free_port)"

    # Start from a known programme, so the packet the test looks for is the
    # one the take produces rather than whatever the last test left on air.
    curl -fsS -X POST "$BASE/api/v1/program/take" "${AUTH[@]}" -H 'Content-Type: application/json' \
        -d '{"source":"cam2"}' >/dev/null 2>&1
    sleep 1

    step "a TSL packet arrives with the right lamp bits"
    python3 "$REPO/dev/tsl-listen.py" "$tsl_port" --for 12 >"$WORK/tsl.out" 2>&1 &
    local listener=$!
    sleep 0.5
    "$REPO/plugins/tally/bin/gmx-tally" --url "$BASE" --token "$TOKEN" \
        --to "127.0.0.1:$tsl_port" --refresh 2 \
        --lamp "cam1:0:CAM 1" --lamp "cam2:1:CAM 2" >"$WORK/tally.log" 2>&1 &
    PLUGIN_PID=$!
    sleep 2
    curl -fsS -X POST "$BASE/api/v1/program/take" "${AUTH[@]}" -H 'Content-Type: application/json' \
        -d '{"source":"cam1"}' >/dev/null 2>&1 || bad "the take was refused"
    wait $listener 2>/dev/null
    local seen; seen="$(cat "$WORK/tsl.out")"
    if [[ "$seen" == *"index=0 right=red text=red left=off brightness=3 label='CAM 1'"* ]]; then
        ok
        echo "$seen" | sed 's/^/    /'
    else
        bad "no red lamp on index 0. Saw: $seen ; plugin log: $(cat "$WORK/tally.log")"
    fi
    stop_plugin
}

# --- director ---------------------------------------------------------------

run_director() {
    step "the director takes on the rules inside 30 s"
    curl -fsS -X POST "$BASE/api/v1/program/take" "${AUTH[@]}" -H 'Content-Type: application/json' \
        -d '{"source":null}' >/dev/null 2>&1
    "$REPO/plugins/director/bin/gmx-director" --url "$BASE" --token "$TOKEN" \
        --interval 1 --min-hold 3 --slow-look 6 >"$WORK/director.log" 2>&1 &
    PLUGIN_PID=$!
    local takes=0
    for _ in $(seq 1 60); do
        takes="$(grep -c '^gmx-director: take ' "$WORK/director.log" 2>/dev/null)"
        takes="${takes:-0}"
        [[ "$takes" -ge 2 ]] && break
        sleep 0.5
    done
    stop_plugin
    if [[ "$takes" -ge 2 ]]; then
        ok
        grep '^gmx-director: \(take\|hold\)' "$WORK/director.log" | head -6 | sed 's/^/    /'
    else
        bad "only $takes take(s) in 30 s: $(cat "$WORK/director.log")"
    fi
}

case "$ONLY" in
    all) run_osc; run_tally; run_director ;;
    osc) run_osc ;;
    tally) run_tally ;;
    director) run_director ;;
    *) echo "unknown --only '$ONLY'" >&2; exit 2 ;;
esac

echo
if [[ $FAILED -eq 0 ]]; then
    echo "every live test passed."
else
    echo "$FAILED live test(s) failed."
    exit 1
fi
