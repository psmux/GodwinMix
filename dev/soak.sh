#!/usr/bin/env bash
#
# Ten minutes of the things an operator does, over and over, against one core.
#
# Every five seconds it adds a source and removes one, takes between two scenes
# with a transition, opens and closes an MJPEG stream and a PCM stream, and
# installs and removes a plugin. After each round it reads four numbers off the
# running core and writes them down: the programme stall gauge, the core's
# resident memory, its open descriptors and its threads. It also times each of
# the four parts of the round, because "the round got slower" and "the source
# removal got slower" are different bug reports.
#
# What it is looking for is not whether one round works. dev/smoke.sh already
# says that. It is whether the hundredth round costs what the first one did. A
# descriptor left open per take, a thread per plugin install, a slot never
# given back: none of those show up in a single pass and all of them end a show
# two hours in.
#
# Usage: dev/soak.sh [--minutes N] [--machine ID] [--keep] [--skip PHASES]
#   --minutes N   how long to run. 10 by default; the nightly runs 60
#   --machine ID  names the machine in the record. The hostname by default
#   --keep        leave the working directory and the core's log behind
#   --skip P,Q    leave phases out of every round. Names: sources, take,
#                 streams, plugin. For bisecting a number that grows across a
#                 run: take one phase out at a time and see which one the
#                 growth leaves with. A skipped phase times zero and the
#                 record names it, so a run with a phase out is never mistaken
#                 for a clean one. GODWINMIX_SOAK_SKIP sets the same thing.
#
# It writes bench/results/soak-<machine>-<date>.json and prints a summary
# table. A bar that fails also fails the script.
#
# Needs: cargo, python3 (standard library only), curl. On macOS it also needs
# lsof, which is part of the system.

set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Capture before the run: another worktree may commit while a soak is running.
COMMIT="${GODWINMIX_SOAK_COMMIT:-$(cd "$REPO" && git rev-parse --short HEAD 2>/dev/null || echo unknown)}"
MINUTES=10
MACHINE="$(hostname -s 2>/dev/null || hostname)"
KEEP=0
PERIOD=5
SKIP="${GODWINMIX_SOAK_SKIP:-}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --minutes) MINUTES="${2:-}"; shift 2 ;;
        --machine) MACHINE="${2:-}"; shift 2 ;;
        --keep) KEEP=1; shift ;;
        --skip) SKIP="${SKIP:+$SKIP,}${2:-}"; shift 2 ;;
        -h|--help) sed -n '2,32p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $1. dev/soak.sh --help lists them." >&2; exit 2 ;;
    esac
done
if ! [[ "$MINUTES" =~ ^[0-9]+$ ]] || [[ "$MINUTES" -lt 2 ]]; then
    echo "--minutes takes a whole number of at least 2. The warm up alone is one minute." >&2
    exit 2
fi

# Which of the four phases a round runs. `skipping sources` is true when the
# name is in the list, and a name nobody recognises is a typo worth stopping
# for rather than a run that quietly measured all four.
SKIP="${SKIP//[[:space:]]/}"
SKIPPED=""
if [[ -n "$SKIP" ]]; then
    IFS=',' read -r -a SKIP_NAMES <<<"$SKIP"
    for name in "${SKIP_NAMES[@]}"; do
        [[ -z "$name" ]] && continue
        case "$name" in
            sources|take|streams|plugin) SKIPPED="$SKIPPED $name" ;;
            *) echo "--skip takes sources, take, streams or plugin, not '$name'." >&2; exit 2 ;;
        esac
    done
fi
skipping() { [[ " $SKIPPED " == *" $1 "* ]]; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/gmx-soak.XXXXXX")"
LOG="$WORK/core.log"
SAMPLES="$WORK/samples.tsv"
TOKEN="soak-$RANDOM$RANDOM"
CORE_PID=""
FAILED=0

# --- the bars ---------------------------------------------------------------
#
# The acceptance line from the roadmap: 34 ms is one frame at 30 fps plus a
# millisecond, and the gauge it is written against averages over sixty frames,
# so one late thread wake up cannot trip it and a pipeline that really stopped
# cannot hide in it.
STALL_BAR=34
# Memory measured against the warm up sample rather than against the start.
# The first minute is where the pipeline builds its pools, the allocator takes
# its arenas and GStreamer loads the plugins it turns out to need; growth there
# is the core arriving at its working size, not a leak.
RSS_GROWTH_PCT=10
WARMUP_SECS=60
# Descriptors and threads are compared late against warm up with a fixed slack
# rather than demanding they are equal. Two reasons for the slack, and neither
# of them is a leak:
#
#   * a sample is taken while a round's work is still settling. The MJPEG
#     response and the PCM websocket have been closed by then but the kernel
#     may still be holding the socket, and a plugin that was removed a moment
#     ago may still have a pipe open while its process exits.
#   * GStreamer's buffer pools and tokio's blocking pool both grow on demand
#     and keep an idle thread or two alive afterwards rather than tearing the
#     pool down between uses.
#
# So the slack is what one round in flight costs, plus a little. Anything that
# grows per round walks straight through it: a hundred rounds of one leaked
# descriptor is a hundred descriptors, not eight.
FD_SLACK=8
THREAD_SLACK=4

# --- reporting --------------------------------------------------------------

step() { printf '%-58s' "$1"; }
ok() { printf 'ok\n'; }
bad() {
    printf 'FAIL\n'
    printf '    %s\n' "$1" >&2
    FAILED=$((FAILED + 1))
}

cleanup() {
    if [[ -n "$CORE_PID" ]] && kill -0 "$CORE_PID" 2>/dev/null; then
        kill "$CORE_PID" 2>/dev/null
        wait "$CORE_PID" 2>/dev/null
    fi
    if [[ $KEEP -eq 1 ]]; then
        echo "kept: $WORK"
    else
        rm -rf "$WORK"
    fi
}
trap cleanup EXIT

# --- sampling ---------------------------------------------------------------
#
# `ps -o rss=` is kilobytes on both Linux and macOS and needs no branch.
# Descriptors and threads do: Linux has /proc, which is a directory listing and
# costs nothing, and macOS has neither of those files, so it pays for lsof and
# for `ps -M`, which prints one line per thread after a header.

UNAME="$(uname -s)"

# Milliseconds. `date` has no sub second format on macOS and $EPOCHREALTIME
# needs bash 5, which macOS does not ship, so this is python3, which the script
# already needs. Five of these a round is about a tenth of a second of the five
# second period, which is a price worth paying to know which part of a round
# got slower.
now_ms() { python3 -c 'import time;print(int(time.time()*1000))'; }

rss_kb() { ps -o rss= -p "$1" 2>/dev/null | tr -d ' '; }

fd_count() {
    if [[ "$UNAME" == "Linux" ]]; then
        ls "/proc/$1/fd" 2>/dev/null | wc -l | tr -d ' '
    else
        lsof -p "$1" 2>/dev/null | tail -n +2 | wc -l | tr -d ' '
    fi
}

thread_count() {
    if [[ "$UNAME" == "Linux" ]]; then
        ls "/proc/$1/task" 2>/dev/null | wc -l | tr -d ' '
    else
        ps -M -p "$1" 2>/dev/null | tail -n +2 | wc -l | tr -d ' '
    fi
}

# The gauge, not the histogram. gmx_programme_frame_interval_ms is the raw wall
# clock gap between two frames and carries ordinary thread scheduling jitter;
# gmx_programme_frame_stall_ms is the worst average over sixty consecutive
# frames, which is the number the 34 ms bar means.
stall_ms() {
    curl -fsS --max-time 10 "$BASE/metrics" "${AUTH[@]}" 2>/dev/null \
        | awk '$1 == "gmx_programme_frame_stall_ms" { print $2; exit }'
}

# --- the core ---------------------------------------------------------------

PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
BASE="http://127.0.0.1:$PORT"
AUTH=(-H "Authorization: Bearer $TOKEN")
# Every call in a round gets a budget of two periods. A soak compares the
# hundredth round against the first, so a round that stretches to a minute
# because one call took a minute has stopped being the same measurement. The
# budget turns that into a line in the report instead, naming the call.
BUDGET=(--max-time $((PERIOD * 2)))

echo "GodwinMix soak on $BASE for $MINUTES minute(s), a round every ${PERIOD}s"
if [[ -n "$SKIPPED" ]]; then
    echo "phases left out of every round:$SKIPPED. This run cannot be compared with a full one."
fi
echo

# Release, unlike dev/smoke.sh. A debug core misses a 34 ms frame bar on its
# own, before anything has leaked, so a soak built that way measures the
# compiler rather than the mixer.
if [[ -n "${GODWINMIX_SOAK_BIN_DIR:-}" ]]; then
    GMX="$GODWINMIX_SOAK_BIN_DIR/gmx"
    CORE="$GODWINMIX_SOAK_BIN_DIR/godwinmix"
else
    step "build --release"
    if ! (cd "$REPO" && cargo build --release --quiet) >"$WORK/build.log" 2>&1; then
        bad "cargo build --release failed, see $WORK/build.log"
        KEEP=1
        exit 1
    fi
    ok
    GMX="${CARGO_TARGET_DIR:-$REPO/target}/release/gmx"
    CORE="${CARGO_TARGET_DIR:-$REPO/target}/release/godwinmix"
fi
if [[ ! -x "$GMX" || ! -x "$CORE" ]]; then
    bad "release binaries are missing; build them or set GODWINMIX_SOAK_BIN_DIR"
    exit 1
fi

step "config from --example-config"
"$CORE" --example-config >"$WORK/example.toml" 2>/dev/null
python3 - "$WORK/example.toml" "$WORK/godwinmix.toml" "$PORT" "$TOKEN" <<'PY'
import os, re, sys
src, dst, port, token = sys.argv[1:5]
text = open(src).read()
# No configured source and no configured output: the soak drives everything
# through the API, and a camera that is not there would stop it at the door.
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
# Short lingers, because a soak takes a preview stream up and down every five
# seconds and a thirty second linger would mean the mosaic never comes down and
# nothing about its teardown is ever exercised.
text = re.sub(r'(?m)^linger_secs = .*$', 'linger_secs = 2', text)
text = re.sub(r'(?m)^idle_secs = .*$', 'idle_secs = 2', text)
# The three safety rules, stood down. A take every few seconds is exactly what
# the eight second minimum hold, the twelve takes a minute rate limit and the
# flash guard exist to refuse, and colour bars against a two box of colour bars
# is the luminance change the flash guard is written for. All three have their
# own tests in crates/godwinmix-core/src/safety.rs. What is being soaked here
# is what a take costs the hundredth time, not whether the policy works.
text = re.sub(r'(?m)^min_hold_ms = .*$', 'min_hold_ms = 0', text)
text = re.sub(r'(?m)^max_takes_per_minute = .*$', 'max_takes_per_minute = 600', text)
text = re.sub(r'(?m)^flash_guard = .*$', 'flash_guard = false', text)
text = re.sub(
    r'(?m)^# plugins_dir = .*$',
    'plugins_dir = "%s/plugins"' % os.path.dirname(dst),
    text,
)
open(dst, "w").write(text)
PY
if grep -q "^token = " "$WORK/godwinmix.toml" && grep -q "127.0.0.1:$PORT" "$WORK/godwinmix.toml"; then
    ok
else
    bad "the config was not rewritten"
    exit 1
fi

step "core starts"
# `exec`, so that CORE_PID is the core itself. Without it the subshell stays
# in the middle and every sample measures the shell's one thread and seven
# descriptors instead of the mixer's.
(cd "$WORK" && exec "$CORE" --config "$WORK/godwinmix.toml") >"$LOG" 2>&1 &
CORE_PID=$!
for _ in $(seq 1 100); do
    curl -fsS "$BASE/api/v1/core/info" "${AUTH[@]}" >/dev/null 2>&1 && break
    kill -0 "$CORE_PID" 2>/dev/null || break
    sleep 0.2
done
if curl -fsS "$BASE/api/v1/core/info" "${AUTH[@]}" >/dev/null 2>&1; then
    ok
else
    bad "the core never answered; see $LOG"
    KEEP=1
    tail -20 "$LOG" >&2
    exit 1
fi

export GODWINMIX_URL="$BASE" GODWINMIX_TOKEN="$TOKEN"

# --- what each round works on ----------------------------------------------

step "two sources and two scenes to take between"
curl -fsS -X POST "$BASE/api/v1/sources" "${AUTH[@]}" -H 'content-type: application/json' \
    -d '{"id":"bars","uri":"test://smpte","name":"Soak bars"}' >"$WORK/setup.log" 2>&1
curl -fsS -X POST "$BASE/api/v1/sources" "${AUTH[@]}" -H 'content-type: application/json' \
    -d '{"id":"ball","uri":"test://ball","name":"Soak ball"}' >>"$WORK/setup.log" 2>&1
"$GMX" ctl scene new "soak one" bars >>"$WORK/setup.log" 2>&1
"$GMX" ctl scene new "soak two" bars ball >>"$WORK/setup.log" 2>&1
"$GMX" ctl take --scene "soak one" >>"$WORK/setup.log" 2>&1
if curl -fsS "$BASE/api/v1/scenes" "${AUTH[@]}" | grep -q "soak two"; then
    ok
else
    bad "$(tail -5 "$WORK/setup.log" | tr '\n' '; ')"
    KEEP=1
    exit 1
fi

# A shell plugin, scaffolded once and installed and removed every round. Shell
# rather than a binary on purpose: what is being soaked is the install and the
# teardown, not a compiler.
step "a plugin to install and remove every round"
PLUGDIR="$WORK/soak-bars"
if skipping plugin; then
    printf 'skipped\n'
elif "$GMX" plugin new soak-bars --kind source --lang shell --out "$PLUGDIR" \
        >"$WORK/plugin-new.log" 2>&1; then
    ok
else
    bad "$(tail -5 "$WORK/plugin-new.log" | tr '\n' '; ')"
    KEEP=1
    exit 1
fi

# One frame off /mjpeg/program and three off the /pcm/program websocket, then
# both hang up. Python because /pcm is a websocket and curl does not speak one.
cat >"$WORK/streams.py" <<'PY'
"""Open an MJPEG stream and a PCM stream, read a little, close both."""
import base64, os, socket, struct, sys, urllib.request

host, port, token = sys.argv[1], int(sys.argv[2]), sys.argv[3]
base = f"http://{host}:{port}"


def authed(path):
    r = urllib.request.Request(f"{base}{path}")
    r.add_header("Authorization", f"Bearer {token}")
    return r


def one_jpeg():
    resp = urllib.request.urlopen(authed("/mjpeg/program"), timeout=20)
    buf = b""
    while len(buf) < 4_000_000:
        chunk = resp.read(4096)
        if not chunk:
            break
        buf += chunk
        start = buf.find(b"\xff\xd8")
        if start >= 0 and b"\xff\xd9" in buf[start:]:
            break
    resp.close()
    if buf.find(b"\xff\xd8") < 0:
        raise RuntimeError(f"no JPEG in {len(buf)} bytes of /mjpeg/program")


def some_pcm(count=3):
    s = socket.create_connection((host, port), timeout=20)
    key = base64.b64encode(os.urandom(16)).decode()
    s.send(
        (
            f"GET /pcm/program HTTP/1.1\r\nHost: {host}:{port}\r\n"
            f"Upgrade: websocket\r\nConnection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n"
            f"Authorization: Bearer {token}\r\n\r\n"
        ).encode()
    )
    buf = b""
    while b"\r\n\r\n" not in buf:
        part = s.recv(4096)
        if not part:
            raise RuntimeError("the socket closed during the websocket handshake")
        buf += part
    status = buf.split(b"\r\n", 1)[0].decode()
    if "101" not in status:
        raise RuntimeError(f"/pcm/program answered {status}")
    rest = buf.split(b"\r\n\r\n", 1)[1]
    seen, buf = 0, rest
    while seen < count:
        while len(buf) < 2:
            buf += s.recv(65536)
        opcode, length = buf[0] & 0x0F, buf[1] & 0x7F
        offset = 2
        if length == 126:
            while len(buf) < 4:
                buf += s.recv(65536)
            length, offset = struct.unpack(">H", buf[2:4])[0], 4
        elif length == 127:
            while len(buf) < 10:
                buf += s.recv(65536)
            length, offset = struct.unpack(">Q", buf[2:10])[0], 10
        while len(buf) < offset + length:
            buf += s.recv(65536)
        buf = buf[offset + length:]
        if opcode == 2:
            seen += 1
    s.close()


one_jpeg()
some_pcm()
PY

# --- one round --------------------------------------------------------------

ROUND=0
TROUBLE=""

# Remember the worst thing a round said, once per kind, so that a hundred
# rounds of the same broken call do not print a hundred lines.
note() {
    case "$TROUBLE" in
        *"$1"*) ;;
        *) TROUBLE="$TROUBLE$1: $2"$'\n' ;;
    esac
}

# The soak's own sources that the mixer still has, oldest first. One is the
# healthy answer: each round removes the one the round before it added. The
# list is read rather than remembered, so a round that could not tell whether
# its add landed still finds the source next time.
mine() {
    curl -fsS --max-time 5 "$BASE/api/v1/sources" "${AUTH[@]}" 2>/dev/null \
        | grep -o '"soak-[0-9]\{1,\}"' | tr -d '"' | sort -t- -k2 -n -u
}

round_sources() {
    local ids oldest left
    ids="$(mine)"
    left=0
    [[ -n "$ids" ]] && left="$(printf '%s\n' "$ids" | wc -l | tr -d ' ')"
    oldest="$(printf '%s\n' "$ids" | head -1)"
    if [[ -n "$oldest" ]]; then
        curl -fsS "${BUDGET[@]}" -X DELETE "$BASE/api/v1/sources/$oldest" \
            "${AUTH[@]}" >/dev/null 2>&1 \
            || note "source.remove" "round $ROUND, $oldest did not go inside ${BUDGET[1]} s"
    fi
    # Adding one a round while removals do not keep up turns a slow mixer into
    # an overloaded one, and then every other number in the run is measuring
    # the overload rather than the mixer. Past a small backlog the soak stops
    # adding and keeps draining, oldest first, until the backlog clears. That
    # it happened at all is itself a finding and is said once at the end.
    if [[ "$left" -gt 3 ]]; then
        note "source backlog" \
            "round $ROUND: $left of this soak's sources were still on the mixer, so \
nothing was added that round. Removals were not keeping up with one add every ${PERIOD} s."
        return
    fi
    curl -fsS "${BUDGET[@]}" -X POST "$BASE/api/v1/sources" "${AUTH[@]}" \
        -H 'content-type: application/json' \
        -d "{\"id\":\"soak-$ROUND\",\"uri\":\"test://smpte\"}" >/dev/null 2>&1 \
        || note "source.add" "round $ROUND, no answer inside ${BUDGET[1]} s"
}

round_take() {
    # Round one takes the scene the setup did not, so that every take in the
    # run is a real change of picture rather than a take of what is already on
    # air. Alternating from there.
    local scene="soak two" code
    [[ $((ROUND % 2)) -eq 0 ]] && scene="soak one"
    # The status code and the body, rather than curl's `-f`, because a refused
    # take answers with the rule that refused it and that sentence is the
    # whole diagnosis.
    code="$(curl -sS "${BUDGET[@]}" -o "$WORK/take.json" -w '%{http_code}' \
        -X POST "$BASE/api/v1/program/take" "${AUTH[@]}" \
        -H 'content-type: application/json' \
        -d "{\"scene\":\"$scene\",\"transition\":{\"type\":\"fade\",\"duration_ms\":300}}" 2>/dev/null)"
    [[ "$code" == 2?? ]] \
        || note "program.take" \
            "round $ROUND, $scene: $code $(tr '\n' ' ' <"$WORK/take.json" | cut -c1-200)"
}

round_streams() {
    python3 "$WORK/streams.py" 127.0.0.1 "$PORT" "$TOKEN" >"$WORK/streams.log" 2>&1 \
        || note "streams" "round $ROUND: $(tail -1 "$WORK/streams.log")"
}

round_plugin() {
    "$GMX" plugin add "$PLUGDIR" >"$WORK/plugin.log" 2>&1 \
        || note "plugin.add" "round $ROUND: $(tail -1 "$WORK/plugin.log")"
    "$GMX" plugin remove soak-bars >>"$WORK/plugin.log" 2>&1 \
        || note "plugin.remove" "round $ROUND: $(tail -1 "$WORK/plugin.log")"
}

# One row, or a refusal to write one.
#
# A sample taken from a core that has gone is four zeros, and four zeros pass
# every bar in this script: no growth, no stall. So a row is written only when
# the process is still there to be measured, and the caller stops the run when
# it is not. A scrape that did not answer is written as -1 rather than 0 for
# the same reason: the verdict counts those and leaves them out of the worst,
# instead of reading a mixer too busy to answer as a mixer keeping up.
sample() {
    local elapsed="$1"
    local stall rss fds threads
    stall="$(stall_ms)"
    rss="$(rss_kb "$CORE_PID")"
    [[ -z "$rss" || "$rss" == "0" ]] && return 1
    fds="$(fd_count "$CORE_PID")"
    threads="$(thread_count "$CORE_PID")"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$ROUND" "$elapsed" "${stall:--1}" "$rss" "${fds:-0}" "${threads:-0}" \
        "$ROUND_MS" "$SOURCES_MS" "$TAKE_MS" "$STREAMS_MS" "$PLUGIN_MS" \
        >>"$SAMPLES"
}

# --- the soak ---------------------------------------------------------------

printf '%-8s %-8s %10s %10s %8s %8s\n' round elapsed stall_ms rss_mb fds threads
START="$(date +%s)"
DEADLINE=$((START + MINUTES * 60))
NEXT_LINE=0
printf 'round\telapsed\tstall_ms\trss_kb\tfds\tthreads\tround_ms\tsources_ms\ttake_ms\tstreams_ms\tplugin_ms\n' >"$SAMPLES"

while :; do
    NOW="$(date +%s)"
    [[ $NOW -ge $DEADLINE ]] && break
    ROUND=$((ROUND + 1))
    # One phase at a time, each timed. Which part of a round got slower is a
    # different bug report from "the round got slower", and the four phases
    # fail in four different places.
    T0="$(now_ms)"
    skipping sources || round_sources
    T1="$(now_ms)"
    skipping take || round_take
    T2="$(now_ms)"
    skipping streams || round_streams
    T3="$(now_ms)"
    skipping plugin || round_plugin
    T4="$(now_ms)"
    SOURCES_MS=$((T1 - T0))
    TAKE_MS=$((T2 - T1))
    STREAMS_MS=$((T3 - T2))
    PLUGIN_MS=$((T4 - T3))
    ROUND_MS=$((T4 - T0))
    ELAPSED=$(( $(date +%s) - START ))
    # Liveness before the sample, not after it: a core that has gone must stop
    # the run rather than contribute a row of zeros to the verdict.
    if ! kill -0 "$CORE_PID" 2>/dev/null; then
        echo
        bad "the core died at round $ROUND, $ELAPSED s in. The last of its log:"
        tail -20 "$LOG" >&2
        KEEP=1
        break
    fi
    if ! sample "$ELAPSED"; then
        echo
        bad "the core stopped answering at round $ROUND, $ELAPSED s in. The last of its log:"
        tail -20 "$LOG" >&2
        KEEP=1
        break
    fi
    read -r _ _ S R F T _ _ _ _ _ < <(tail -1 "$SAMPLES")
    # A line every half minute rather than every round. A round takes as long
    # as it takes (a plugin install and a mosaic are both slower than the five
    # second period asks for), so counting rounds would print a wall of text on
    # a fast machine and three lines on a slow one.
    if [[ $ELAPSED -ge $NEXT_LINE ]]; then
        printf '%-8s %-8s %10s %10s %8s %8s\n' \
            "$ROUND" "${ELAPSED}s" "$S" \
            "$(awk -v k="$R" 'BEGIN { printf "%.1f", k / 1024 }')" "$F" "$T"
        NEXT_LINE=$((ELAPSED + 30))
    fi
    NEXT=$((START + ROUND * PERIOD))
    NOW="$(date +%s)"
    [[ $NOW -lt $NEXT ]] && sleep $((NEXT - NOW))
done

echo

# --- the verdict ------------------------------------------------------------
#
# Python rather than awk because the same pass writes the record, and one
# reader of the samples is easier to keep honest than two.

DATE="$(date -u +%Y-%m-%d)"
RECORD="$REPO/bench/results/soak-$MACHINE-$DATE.json"
[[ -n "$SKIPPED" ]] && RECORD="$REPO/bench/results/soak-$MACHINE-$DATE${SKIPPED// /-no}.json"

python3 - "$SAMPLES" "$RECORD" "$MACHINE" "$COMMIT" "$MINUTES" "$PERIOD" \
    "$STALL_BAR" "$RSS_GROWTH_PCT" "$WARMUP_SECS" "$FD_SLACK" "$THREAD_SLACK" \
    "$SKIPPED" <<'PY'
import json, platform, sys, datetime

(samples, record, machine, commit, minutes, period,
 stall_bar, rss_pct, warmup, fd_slack, thread_slack, skipped) = sys.argv[1:13]
skipped = skipped.split()
stall_bar, rss_pct = float(stall_bar), float(rss_pct)
warmup, fd_slack, thread_slack = int(warmup), int(fd_slack), int(thread_slack)

rows = []
with open(samples) as f:
    next(f, None)
    for line in f:
        parts = line.split()
        if len(parts) != 11:
            continue
        r, e, s, rss, fds, th, took, src, take, streams, plug = parts
        rows.append({
            "round": int(r), "elapsed_s": int(e), "stall_ms": float(s),
            "rss_kb": int(rss), "fds": int(fds), "threads": int(th),
            "round_ms": int(took), "sources_ms": int(src), "take_ms": int(take),
            "streams_ms": int(streams), "plugin_ms": int(plug),
        })

if not rows:
    print("no rounds ran, so there is nothing to judge.")
    sys.exit(1)

# The warm up sample is the first one taken at or after the warm up period.
# Everything after it is compared against that one, not against round one.
warm = next((r for r in rows if r["elapsed_s"] >= warmup), rows[0])
last = rows[-1]
# A scrape that did not answer is -1 and is left out of the worst rather than
# read as a mixer that never stalled.
scraped = [r for r in rows if r["stall_ms"] >= 0]
missed = len(rows) - len(scraped)
if not scraped:
    print("no scrape of /metrics answered, so there is no stall reading to judge.")
    sys.exit(1)
worst_stall = max(r["stall_ms"] for r in scraped)
worst_row = next(r for r in scraped if r["stall_ms"] == worst_stall)
peak_rss = max(r["rss_kb"] for r in rows)
peak_fds = max(r["fds"] for r in rows)
peak_threads = max(r["threads"] for r in rows)

rss_growth = 0.0
if warm["rss_kb"]:
    rss_growth = (last["rss_kb"] - warm["rss_kb"]) * 100.0 / warm["rss_kb"]
fd_growth = last["fds"] - warm["fds"]
thread_growth = last["threads"] - warm["threads"]

checks = [
    {
        "name": "programme stall",
        "warm": f'{warm["stall_ms"]:.1f}' if warm["stall_ms"] >= 0 else "-",
        "last": f'{scraped[-1]["stall_ms"]:.1f}',
        "worst": f"{worst_stall:.1f}",
        "bar": f"at most {stall_bar:.0f} ms",
        "ok": worst_stall <= stall_bar,
        "why": f'round {worst_row["round"]}, {worst_row["elapsed_s"]} s in, '
               f"{worst_stall:.1f} ms",
    },
    {
        "name": "resident memory (MB)",
        "warm": f'{warm["rss_kb"] / 1024:.1f}',
        "last": f'{last["rss_kb"] / 1024:.1f}',
        "worst": f"{peak_rss / 1024:.1f}",
        "bar": f"at most +{rss_pct:.0f}% on warm up",
        "ok": rss_growth <= rss_pct,
        "why": f"{rss_growth:+.1f}% against the warm up sample",
    },
    {
        "name": "open descriptors",
        "warm": str(warm["fds"]),
        "last": str(last["fds"]),
        "worst": str(peak_fds),
        "bar": f"at most +{fd_slack} on warm up",
        "ok": fd_growth <= fd_slack,
        "why": f"{fd_growth:+d} against the warm up sample",
    },
    {
        "name": "threads",
        "warm": str(warm["threads"]),
        "last": str(last["threads"]),
        "worst": str(peak_threads),
        "bar": f"at most +{thread_slack} on warm up",
        "ok": thread_growth <= thread_slack,
        "why": f"{thread_growth:+d} against the warm up sample",
    },
]

head = f'{"Measure":<22}{"warm up":>10}{"last":>10}{"worst":>10}  {"Bar":<28}Verdict'
print(head)
print("-" * len(head))
for c in checks:
    print(f'{c["name"]:<22}{c["warm"]:>10}{c["last"]:>10}{c["worst"]:>10}  '
          f'{c["bar"]:<28}{"within" if c["ok"] else "OVER"}')
print()
phases = ("sources", "take", "streams", "plugin")
rounds_ms = [r["round_ms"] for r in rows]
slowest = max(rows, key=lambda r: r["round_ms"])
print(f'{len(rows)} rounds over {last["elapsed_s"]} s, asked for one every {period} s. '
      f'Warm up sample at {warm["elapsed_s"]} s.')
if skipped:
    print(f'{", ".join(skipped)} did not run this time, so this record is a bisection '
          f'rather than a verdict on the build.')
print(f'A round took {sum(rounds_ms) / len(rounds_ms) / 1000:.1f} s on average and '
      f'{slowest["round_ms"] / 1000:.1f} s at its worst, on round {slowest["round"]}.')
if missed:
    print(f'{missed} of {len(rows)} scrapes of /metrics did not answer inside 10 s and '
          f"are not in the stall column. A mixer too busy to answer is a finding of its "
          f"own, and it is not a mixer that never stalled.")
print()
head = f'{"Phase":<12}{"first":>10}{"median":>10}{"worst":>10}{"last":>10}   (seconds)'
print(head)
print("-" * len(head))
for name in phases:
    got = sorted(r[f"{name}_ms"] for r in rows)
    mid = got[len(got) // 2]
    print(f'{name:<12}{rows[0][f"{name}_ms"] / 1000:>10.2f}{mid / 1000:>10.2f}'
          f'{got[-1] / 1000:>10.2f}{last[f"{name}_ms"] / 1000:>10.2f}')
print()
print("A phase whose last is far above its first is the thing to chase: the round "
      "did not get slower by itself, one of its four calls did.")
for c in checks:
    if not c["ok"]:
        print(f'  over: {c["name"]} is {c["why"]}')

out = {
    "kind": "soak",
    "machine": machine,
    "commit": commit,
    "date": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
    "os": f"{platform.system().lower()} {platform.machine()} {platform.release()}",
    "minutes": int(minutes),
    "period_s": int(period),
    "warmup_s": warmup,
    "rounds": len(rows),
    "skipped_phases": skipped,
    "bars": {
        "stall_ms": stall_bar,
        "rss_growth_pct": rss_pct,
        "fd_slack": fd_slack,
        "thread_slack": thread_slack,
    },
    "warm_up_sample": warm,
    "last_sample": last,
    "worst_stall_ms": worst_stall,
    "scrapes_missed": missed,
    "round_ms_mean": round(sum(rounds_ms) / len(rounds_ms)),
    "round_ms_worst": slowest["round_ms"],
    "phase_ms": {
        name: {
            "first": rows[0][f"{name}_ms"],
            "median": sorted(r[f"{name}_ms"] for r in rows)[len(rows) // 2],
            "worst": max(r[f"{name}_ms"] for r in rows),
            "last": last[f"{name}_ms"],
        }
        for name in phases
    },
    "rss_growth_pct": round(rss_growth, 2),
    "fd_growth": fd_growth,
    "thread_growth": thread_growth,
    "checks": [{k: c[k] for k in ("name", "bar", "ok", "why")} for c in checks],
    "samples": rows,
}
with open(record, "w") as f:
    json.dump(out, f, indent=1)
    f.write("\n")
print()
print(f"wrote {record}")
sys.exit(0 if all(c["ok"] for c in checks) else 1)
PY
VERDICT=$?

echo
if [[ -n "$TROUBLE" ]]; then
    echo "calls that did not answer, one line per kind:"
    printf '%s' "$TROUBLE"
    FAILED=$((FAILED + 1))
    KEEP=1
fi

step "the core is still up"
if kill -0 "$CORE_PID" 2>/dev/null; then ok; else bad "it is not"; fi

step "the log has no panic"
if grep -qiE "panicked at" "$LOG"; then
    bad "$(grep -iE -m 3 'panicked at' "$LOG")"
else
    ok
fi

curl -fsS -X POST "$BASE/api/v1/core/shutdown" "${AUTH[@]}" -d '{}' \
    -H 'content-type: application/json' >/dev/null 2>&1
for _ in $(seq 1 60); do
    kill -0 "$CORE_PID" 2>/dev/null || { CORE_PID=""; break; }
    sleep 0.25
done

echo
if [[ $VERDICT -ne 0 ]]; then
    echo "a bar was missed"
    KEEP=1
    exit 1
fi
if [[ $FAILED -ne 0 ]]; then
    echo "$FAILED thing(s) went wrong"
    exit 1
fi
echo "every bar held"
exit 0
