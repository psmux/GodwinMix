#!/usr/bin/env bash
# Record the regression corpus in tests/sessions/ against a real core.
#
# Starts a core the way dev/smoke.sh does, drives it over /api/v1, and copies
# the session log it wrote into tests/sessions/<name>.jsonl. Then it replays
# each one against a test core and writes the expectations beside it.
#
# Run it when a recorded session needs making again. The files it writes are
# committed, so a normal `cargo test` needs neither this script nor a network.
#
#   dev/record-sessions.sh              record all three
#   dev/record-sessions.sh takes        record one
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$REPO/tests/sessions"
WANT="${1:-all}"
FAILED=0

step() { printf '  %-52s' "$1"; }
ok() { printf 'ok\n'; }
bad() { printf 'FAILED: %s\n' "$1"; FAILED=$((FAILED + 1)); }

echo "building"
(cd "$REPO" && cargo build --quiet) || { echo "the build failed"; exit 1; }
GODWINMIX="$REPO/target/debug/godwinmix"
GMX="$REPO/target/debug/gmx"
mkdir -p "$OUT"

# --------------------------------------------------------------------------
# One core, one session
# --------------------------------------------------------------------------

CORE_PID=""
WORK=""

start_core() {
    WORK="$(mktemp -d "${TMPDIR:-/tmp}/gmx-record.XXXXXX")"
    TOKEN="record-$RANDOM$RANDOM"
    PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
    BASE="http://127.0.0.1:$PORT"
    AUTH=(-H "Authorization: Bearer $TOKEN" -H 'content-type: application/json')
    cat >"$WORK/godwinmix.toml" <<EOF
[canvas]
width = 1280
height = 720
fps = 30
sample_rate = 48000
channels = 2

[control]
bind = "127.0.0.1:$PORT"
token = "$TOKEN"

[multiview]
enabled = false

# A recorded session is re-issued command for command, so nothing else may
# refuse a take while it is being recorded.
[safety]
min_hold_ms = 0
flash_guard = false

# A stall that takes ten seconds to notice would make the recorded session ten
# seconds longer and the replay with it. Three is long enough to be a stall and
# short enough to be a test.
[stall]
restart_after_secs = 3
EOF
    (cd "$WORK" && "$GODWINMIX" --config "$WORK/godwinmix.toml") >"$WORK/core.log" 2>&1 &
    CORE_PID=$!
    for _ in $(seq 1 100); do
        curl -fsS "$BASE/api/v1/core/info" "${AUTH[@]}" >/dev/null 2>&1 && return 0
        kill -0 "$CORE_PID" 2>/dev/null || break
        sleep 0.2
    done
    return 1
}

stop_core() {
    curl -fsS -X POST "$BASE/api/v1/core/shutdown" "${AUTH[@]}" -d '{}' >/dev/null 2>&1
    for _ in $(seq 1 50); do
        kill -0 "$CORE_PID" 2>/dev/null || break
        sleep 0.2
    done
    kill -9 "$CORE_PID" 2>/dev/null
    wait "$CORE_PID" 2>/dev/null
}

api() { curl -fsS -X POST "$BASE/api/v1/$1" "${AUTH[@]}" -d "$2"; }

# Wait for a source to reach a state, or give up quietly: a session that
# records a source failing wants the failure, not a timeout.
wait_state() {
    for _ in $(seq 1 60); do
        if curl -fsS "$BASE/api/v1/sources" "${AUTH[@]}" 2>/dev/null |
            python3 -c "import json,sys; d=json.load(sys.stdin); sys.exit(0 if any(s['id']=='$1' and s['state']=='$2' for s in (d if isinstance(d,list) else d.get('sources',[]))) else 1)"; then
            return 0
        fi
        sleep 0.25
    done
    return 1
}

keep() {
    local name="$1"
    # The runtime directory is `.godwinmix` beside the config, which is where
    # `godwinmix --info` says the session log lives.
    cp "$WORK/.godwinmix/session.jsonl" "$OUT/$name.jsonl" 2>/dev/null
    [ -s "$OUT/$name.jsonl" ]
}

# --------------------------------------------------------------------------
# The three sessions
# --------------------------------------------------------------------------

record_takes() {
    api sources '{"id":"cam1","uri":"test://smpte"}' >/dev/null
    api sources '{"id":"cam2","uri":"test://ball"}' >/dev/null
    wait_state cam1 live
    wait_state cam2 live
    api program/take '{"source":"cam1"}' >/dev/null
    sleep 1
    api program/take '{"source":"cam2"}' >/dev/null
    sleep 1
    api program/revert '{}' >/dev/null
    sleep 1
    api program/take '{}' >/dev/null
    sleep 1
}

record_ad_break() {
    api sources '{"id":"cam1","uri":"test://smpte"}' >/dev/null
    wait_state cam1 live
    api program/take '{"source":"cam1"}' >/dev/null
    sleep 1
    api adbreak/start "{\"uri\":\"$CLIP\",\"return_to\":\"cam1\"}" >/dev/null
    sleep 2
    api adbreak/end '{}' >/dev/null
    sleep 1
}

record_source_stall() {
    api sources '{"id":"cam1","uri":"test://smpte"}' >/dev/null
    wait_state cam1 live
    api program/take '{"source":"cam1"}' >/dev/null
    sleep 1
    # A camera at an address nothing is listening on. It fails the same way on
    # every machine and needs no network, which is what makes the recording
    # replayable; the remove and the add that follow are the rebuild.
    api sources '{"id":"hall","uri":"rtmp://127.0.0.1:1/live/hall"}' >/dev/null
    sleep 4
    curl -fsS -X DELETE "$BASE/api/v1/sources/hall" "${AUTH[@]}" >/dev/null
    sleep 1
    api sources '{"id":"hall","uri":"test://ball"}' >/dev/null
    wait_state hall live
    api program/take '{"source":"hall"}' >/dev/null
    sleep 1
}

# A two second clip for the ad break.
#
# Written to the same path and with the same recipe `gmx session replay` uses
# when it has to stand one in, so a replay of these sessions rolls exactly the
# file that was recorded. Nothing binary is committed.
make_clip() {
    CLIP="${TMPDIR:-/tmp}/gmx-replay-ad.mkv"
    [ -s "$CLIP" ] && return 0
    gst-launch-1.0 -q \
        videotestsrc num-buffers=60 pattern=smpte ! video/x-raw,width=320,height=180,framerate=30/1 ! \
        videoconvert ! theoraenc ! matroskamux name=m ! filesink location="$CLIP" \
        audiotestsrc num-buffers=94 ! audioconvert ! vorbisenc ! m. >/dev/null 2>&1
    [ -s "$CLIP" ] || gst-launch-1.0 -q \
        videotestsrc num-buffers=60 pattern=smpte ! video/x-raw,width=320,height=180,framerate=30/1 ! \
        videoconvert ! jpegenc ! avimux ! filesink location="$CLIP" >/dev/null 2>&1
    [ -s "$CLIP" ]
}

one() {
    local name="$1"
    step "recording $name"
    if ! start_core; then
        bad "the core did not start"
        [ -n "$WORK" ] && cat "$WORK/core.log" | tail -5
        return
    fi
    if ! make_clip; then
        bad "gst-launch-1.0 could not make the two second clip"
        stop_core
        return
    fi
    "record_${name//-/_}"
    stop_core
    if keep "$name"; then ok; else bad "no session log was written"; fi
    rm -rf "$WORK"
}

for name in takes ad-break source-stall; do
    if [ "$WANT" = "all" ] || [ "$WANT" = "$name" ]; then
        one "$name"
    fi
done

# --------------------------------------------------------------------------
# Expectations, from a replay of what was just recorded
# --------------------------------------------------------------------------

for name in takes ad-break source-stall; do
    [ "$WANT" = "all" ] || [ "$WANT" = "$name" ] || continue
    [ -s "$OUT/$name.jsonl" ] || continue
    step "expectations for $name"
    # Not --fast: the recorded gaps are what put the events in the order the
    # night put them in, and an expectation written from a race is worthless.
    if "$GMX" session replay "$OUT/$name.jsonl" --against test-core \
        --write-expectations >"$OUT/$name.expect_changes.json" 2>"$OUT/.$name.err"; then
        ok
    else
        bad "$(tail -3 "$OUT/.$name.err" | tr '\n' ' ')"
    fi
    rm -f "$OUT/.$name.err"
done

if [ "$FAILED" -gt 0 ]; then
    echo "$FAILED step(s) failed"
    exit 1
fi
echo "the corpus in tests/sessions/ is recorded"
