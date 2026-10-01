#!/usr/bin/env bash
#
# A headend at scale, measured: N live MPEG-TS feeds into a release station,
# one show per feed, every process sampled once a second, every output
# checked, and one markdown report under dev/bench/results/.
#
# It runs in two parts. First the feeds alone, checked straight off the
# generator, so the report says what was offered and what the generator cost.
# Then the station: shows added in bulk, a settle, and the measured run.
#
# Usage: dev/bench/scale.sh [options]
#   --feeds N        how many feeds (200)
#   --seconds S      how long the measured run lasts (60)
#   --mode M         auto, direct, legacy or feeds. auto picks direct when the
#                    station has show.add_many and legacy when it does not.
#                    direct: one show per feed, compositing off, show.add_many.
#                    legacy: today's shape, a compositing show per feed with a
#                    udp/source and a udp/output, for --legacy-shows of them.
#                    feeds: the generator and the checker only, no station
#   --legacy-shows N how many shows in legacy mode (8)
#   --transport T    unicast or multicast for the feeds (unicast; see the how
#                    to page for why multicast on one Mac loses packets)
#   --format F       each output's format: copy, or a rendition preset (copy)
#   --port P         the station's control port on 127.0.0.1 (18480)
#   --title T        the report's title
#   --note TEXT      a sentence for the top of the report
#   --no-build       use the binaries already built
#   --keep           leave the run folder (its path is printed) after the run
#   --machine ID     names the machine in the report's file name. The CPU by
#                    default, as m4pro or x86_64; never the host name
#
# Needs: cargo, ffmpeg (once, for the clips), curl, tmux. Every program it
# starts runs in a tmux session named gmx-scale-*, killed when it ends.
# The how to page is docs/how-to/benchmark-at-scale.md.

set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FEEDS=200; MEASURE=60; MODE=auto; LEGACY=8; TRANSPORT=unicast; FORMAT=copy
PORT=18480; TITLE=""; NOTE=""; BUILD=1; KEEP=0; SETTLE=15; WARM=20; MACHINE=""
ARGS="$*"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --feeds) FEEDS="$2"; shift 2 ;;
        --seconds) MEASURE="$2"; shift 2 ;;
        --mode) MODE="$2"; shift 2 ;;
        --legacy-shows) LEGACY="$2"; shift 2 ;;
        --transport) TRANSPORT="$2"; shift 2 ;;
        --format) FORMAT="$2"; shift 2 ;;
        --port) PORT="$2"; shift 2 ;;
        --title) TITLE="$2"; shift 2 ;;
        --note) NOTE="$2"; shift 2 ;;
        --no-build) BUILD=0; shift ;;
        --keep) KEEP=1; shift ;;
        --machine) MACHINE="$2"; shift 2 ;;
        -h|--help) sed -n '2,35p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $1. dev/bench/scale.sh --help lists them." >&2; exit 2 ;;
    esac
done

STAMP="$(date +%Y-%m-%d-%H%M)"
if [[ -z "$MACHINE" ]]; then
    MACHINE="$(sysctl -n machdep.cpu.brand_string 2>/dev/null | tr '[:upper:]' '[:lower:]' | sed 's/apple //; s/[^a-z0-9]//g')"
    [[ -z "$MACHINE" ]] && MACHINE="$(uname -m)"
fi
[[ "$MODE" == feeds ]] && WARM="$MEASURE"
# Other work on the machine skews every number, so the report says how busy it was.
LOAD="$( (sysctl -n vm.loadavg 2>/dev/null || cut -d" " -f1-3 /proc/loadavg) | tr -d "{}" | xargs)"
RUN="${TMPDIR:-/tmp}"; RUN="${RUN%/}/gmx-scale-$STAMP"
TOOL="$REPO/tools/scale/target/release/gmx-scale"
BIN="$REPO/target/release/godwinmix"
MEDIA="$REPO/dev/bench/media"
ADDR="127.0.0.1:$PORT"
OUT="udp://127.0.0.1:30000"
if [[ "$TRANSPORT" == multicast ]]; then IN="udp://239.77.0.1:5000"; else IN="udp://127.0.0.1:20000"; fi
mkdir -p "$RUN/plugins" "$REPO/dev/bench/results"

log() { printf '%s  %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
die() { log "$*"; exit 1; }

# wait_for SECONDS WHAT COMMAND...: run COMMAND until it succeeds, or give up.
wait_for() {
    local until=$((SECONDS + $1)) what="$2"; shift 2
    until "$@" >/dev/null 2>&1; do
        (( SECONDS >= until )) && die "gave up waiting for $what"
        sleep 0.5
    done
}

pane_pid() { tmux list-panes -t "$1" -F '#{pane_pid}' 2>/dev/null | head -1; }
api() { curl -s -m 30 -X "$1" "http://$ADDR$2" -H 'content-type: application/json' ${3:+-d "$3"}; }

# Whatever is still running from this run's folder: shows and their plugins.
leftovers() { pgrep -f "$RUN/(shows|plugins)/" 2>/dev/null; }

cleanup() {
    for s in station feeds check; do tmux kill-session -t "gmx-scale-$s" 2>/dev/null; done
    leftovers | xargs kill -9 2>/dev/null
    if [[ $KEEP == 1 ]]; then log "the run folder is $RUN"; else rm -rf "$RUN"; fi
}
trap cleanup EXIT

build() {
    log "building the station, the udp plugin and gmx-scale (release)"
    (cd "$REPO" && cargo build --release -p godwinmix -p gmx-udp) || die "the station did not build"
    cargo build --release --manifest-path "$REPO/tools/scale/Cargo.toml" || die "gmx-scale did not build"
}

# clip NAME ARGS...: one ten second MPEG-TS file, made once and kept.
clip() {
    local name="$1"; shift
    [[ -s "$MEDIA/$name" ]] && return 0
    log "making $name with ffmpeg (once)"
    ffmpeg -hide_banner -loglevel error -y "$@" -t 10 -pix_fmt yuv420p -f mpegts "$MEDIA/$name" || die "ffmpeg could not make $name"
}

media() {
    mkdir -p "$MEDIA"
    local x264="-c:v libx264 -preset veryfast -x264-params nal-hrd=cbr:keyint=30:min-keyint=30:scenecut=0"
    # shellcheck disable=SC2086
    clip hd1080.ts -f lavfi -i testsrc2=size=1920x1080:rate=30 -f lavfi -i sine=frequency=440:sample_rate=48000 \
        -map 0:v -map 1:a $x264 -b:v 7500k -maxrate 7500k -bufsize 7500k -c:a aac -b:a 128k -muxrate 8000k
    # shellcheck disable=SC2086
    clip hd720.ts -f lavfi -i testsrc2=size=1280x720:rate=30 -f lavfi -i sine=frequency=660:sample_rate=48000 \
        -map 0:v -map 1:a $x264 -b:v 3600k -maxrate 3600k -bufsize 3600k -c:a mp2 -b:a 192k -muxrate 4000k
    # shellcheck disable=SC2086
    clip sd.ts -f lavfi -i testsrc2=size=720x480:rate=30 -f lavfi -i sine=frequency=880:sample_rate=48000 \
        -map 0:v -map 1:a $x264 -b:v 1700k -maxrate 1700k -bufsize 1700k -c:a aac -b:a 128k -muxrate 2000k
    # shellcheck disable=SC2086
    clip mpts.ts -f lavfi -i testsrc2=size=1280x720:rate=30 -f lavfi -i sine=frequency=440:sample_rate=48000 \
        -f lavfi -i smptebars=size=720x480:rate=30 -f lavfi -i sine=frequency=1000:sample_rate=48000 \
        -map 0:v -map 1:a -map 2:v -map 3:a $x264 -b:v:0 3600k -maxrate:v:0 3600k -bufsize:v:0 3600k \
        -b:v:1 1700k -maxrate:v:1 1700k -bufsize:v:1 1700k -c:a:0 aac -b:a:0 128k -c:a:1 mp2 -b:a:1 192k \
        -program title=One:st=0:st=1 -program title=Two:st=2:st=3 -muxrate 6500k
    CLIPS=(--clip "$MEDIA/hd1080.ts" --clip "$MEDIA/hd720.ts" --clip "$MEDIA/sd.ts" --clip "$MEDIA/mpts.ts")
}

# Part one: the feeds alone, checked at the generator.
feeds_alone() {
    log "part one: $FEEDS feeds for $WARM s, checked straight off the generator"
    tmux new-session -d -s gmx-scale-check "exec '$TOOL' check --from $IN --count $FEEDS --seconds $((WARM + 3)) --skip 3 --quiet --json '$RUN/check-in.json' > '$RUN/check-in.log' 2>&1"
    sleep 1
    "$TOOL" feeds "${CLIPS[@]}" --count "$FEEDS" --to "$IN" --seconds "$WARM" --out "$OUT" --format "$FORMAT" \
        --csv "$RUN/feeds.csv" --json "$RUN/feeds.json" 2> "$RUN/feeds.log" > /dev/null || die "the feeds did not run: $(tail -3 "$RUN/feeds.log")"
    wait_for 30 "the check of the feeds as sent" test -s "$RUN/check-in.json"
    tmux kill-session -t gmx-scale-check 2>/dev/null
}

start_station() {
    awk '/^\[\[sources\]\]/{exit} {print}' "$REPO/godwinmix.example.toml" | sed "s/^bind = .*/bind = \"$ADDR\"/" > "$RUN/station.toml"
    log "starting the station on $ADDR from a copy of godwinmix.example.toml with its sources and outputs left out"
    tmux new-session -d -s gmx-scale-station "GODWINMIX_PLUGINS_DIR='$RUN/plugins' exec '$BIN' --config '$RUN/station.toml' > '$RUN/station.log' 2>&1"
    wait_for 60 "the station to answer on $ADDR" curl -sf -m 2 "http://$ADDR/api/v1/shows"
    STATION_PID="$(pane_pid gmx-scale-station)"
    # The default channel opens RTMP on 1935, which another mixer on this machine may want.
    api DELETE /api/v1/channels/live > /dev/null
    api POST /api/v1/plugins "{\"source\": \"$REPO/plugins/udp\"}" > /dev/null
    wait_for 600 "the udp plugin to install" sh -c "curl -s -m 5 http://$ADDR/api/v1/plugins | grep -q '\"name\":\"udp\"'"
    api GET /api/v1/core/api > "$RUN/api.json"
    if [[ "$MODE" == auto ]]; then
        if grep -q '"show.add_many"' "$RUN/api.json"; then MODE=direct; else MODE=legacy; fi
        log "this station $( [[ $MODE == direct ]] && echo has || echo has no ) show.add_many, so the mode is $MODE"
    fi
}

# Part two: the station with a show per feed, measured.
station_run() {
    local shows=$FEEDS extra=()
    [[ "$MODE" == legacy ]] && shows=$LEGACY && extra=(--legacy --limit "$LEGACY")
    log "part two: $FEEDS feeds into $shows $MODE shows"
    tmux new-session -d -s gmx-scale-feeds "exec '$TOOL' feeds ${CLIPS[*]} --count $FEEDS --to $IN --seconds 86400 --json '$RUN/feeds-run.json' > '$RUN/feeds-run.log' 2>&1"
    FEEDS_PID="$(pane_pid gmx-scale-feeds)"
    "$TOOL" add --csv "$RUN/feeds.csv" --station "$ADDR" --json "$RUN/add.json" "${extra[@]}" > /dev/null || die "adding shows failed: see $RUN"
    log "added: $(cat "$RUN/add.json")"
    log "settling for $SETTLE s, then measuring for $MEASURE s"
    sleep "$SETTLE"
    tmux new-session -d -s gmx-scale-check "exec '$TOOL' check --from $OUT --count $shows --seconds $MEASURE --quiet --json '$RUN/check.json' > '$RUN/check.log' 2>&1"
    "$TOOL" sample --pid "$STATION_PID" --station "$ADDR" --seconds "$MEASURE" --watch "feeds=$FEEDS_PID" \
        --watch "checker=$(pane_pid gmx-scale-check)" --csv "$RUN/samples.csv" --json "$RUN/sample.json" > /dev/null
    wait_for 30 "the check of the outputs" test -s "$RUN/check.json"
    tmux send-keys -t gmx-scale-feeds C-c
    wait_for 30 "the feeds to stop" test -s "$RUN/feeds-run.json"
}

# stop_station: SIGTERM, as a service manager would, then count what outlived it.
stop_station() {
    ORPHANS=0
    [[ -n "${STATION_PID:-}" ]] || return 0
    kill -TERM "$STATION_PID" 2>/dev/null
    local until=$((SECONDS + 20))
    while kill -0 "$STATION_PID" 2>/dev/null && (( SECONDS < until )); do sleep 0.5; done
    sleep 2
    ORPHANS="$(leftovers | wc -l | tr -d ' ')"
    (( ORPHANS > 0 )) && log "$ORPHANS show or plugin processes outlived the station"
    return 0
}

report() {
    local commit version file
    commit="$(cd "$REPO" && git rev-parse --short HEAD 2>/dev/null || echo unknown)"
    version="$("$BIN" --version 2>/dev/null || echo unknown)"
    [[ -z "$TITLE" ]] && TITLE="$FEEDS feeds, $MODE, $TRANSPORT, $FORMAT"
    printf '{"title":"%s","version":"%s","commit":"%s","date":"%s","mode":"%s","feeds":%s,"seconds":%s,"clips":"hd1080.ts, hd720.ts, sd.ts, mpts.ts","command":"dev/bench/scale.sh %s","note":"%s","load":"%s","orphans":%s}\n' \
        "$TITLE" "$version" "$commit" "$(date '+%Y-%m-%d %H:%M %Z')" "$MODE ($TRANSPORT)" "$FEEDS" "$MEASURE" "$ARGS" "$NOTE" "$LOAD" "${ORPHANS:-0}" > "$RUN/run.json"
    file="$REPO/dev/bench/results/scale-$MACHINE-$STAMP-$MODE.md"
    "$TOOL" report --dir "$RUN" --out "$file" || die "the report could not be written"
    log "wrote $file"
}

[[ $BUILD == 1 ]] && build
[[ -x "$TOOL" ]] || die "$TOOL is not built. Run without --no-build."
media
feeds_alone
[[ "$MODE" != feeds ]] && start_station && station_run && stop_station
report
