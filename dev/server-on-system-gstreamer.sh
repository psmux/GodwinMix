#!/usr/bin/env bash
# The server archive on the distribution's own GStreamer, with nothing else.
#
#   dev/server-on-system-gstreamer.sh godwinmix-<version>-<triple>.tar.gz [seconds]
#
# The .deb and the AppImage carry GStreamer 1.28, so their checks never touch
# the system's. The server archive uses the system's, which is 1.20 on Ubuntu
# 22.04, and 0.3.33 crash looped there at startup on a property 1.20 does not
# have. This unpacks the archive, starts the mixer headless, puts a test
# source on air, streams the programme over RTMP to an ffmpeg listening on
# this machine for `seconds` (60 by default), and fails unless /api/status
# answered, the mixer is still the same process at the end, and the
# recording holds that much H.264 video and AAC audio.
#
# Needs curl, jq and ffmpeg. Run inside a throwaway container, for example
# ubuntu:22.04 with the packages in docs/how-to/install-on-linux.md.
set -euo pipefail
ARCHIVE="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
SECS="${2:-60}"
WORK="$(mktemp -d)"
PORT=18080
TOKEN=server-check
cd "$WORK"
tar xzf "$ARCHIVE"
BIN="$(dirname "$(find "$WORK" -name godwinmix -type f | head -1)")"
echo "GStreamer here: $(gst-inspect-1.0 --version | sed -n 2p)"
"$BIN/godwinmix" --version

mkdir -p state
cat > godwinmix.toml <<TOML
[control]
bind = "127.0.0.1:$PORT"
TOML

pids=()
cleanup() {
    for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done
    wait 2>/dev/null || true
}
trap cleanup EXIT

GODWINMIX_TOKEN="$TOKEN" "$BIN/godwinmix" --config godwinmix.toml > mixer.log 2>&1 &
mixer=$!
pids+=("$mixer")

api() {
    local method=$1 path=$2 body=${3:-}
    local args=(-sS --fail-with-body -X "$method" "http://127.0.0.1:$PORT$path"
        -H "Authorization: Bearer $TOKEN")
    [ -n "$body" ] && args+=(-H 'content-type: application/json' -d "$body")
    curl "${args[@]}"
}

fail() {
    echo "FAIL: $*" >&2
    echo "--- the last of the mixer's log" >&2
    tail -60 mixer.log >&2 || true
    exit 1
}

for _ in $(seq 1 60); do
    kill -0 "$mixer" 2>/dev/null || fail "the mixer exited during startup"
    api GET /api/status > /dev/null 2>&1 && break
    sleep 1
done
api GET /api/status > status.json || fail "/api/status never answered"
echo "/api/status answered: version $(jq -r '.version // .core.version // "?"' status.json)"

ffmpeg -nostdin -loglevel error -listen 1 -i "rtmp://127.0.0.1:19350/live/program" \
    -c copy -f flv out.flv > ffmpeg.log 2>&1 &
pids+=("$!")
sleep 2

api POST /api/sources '{"id":"bars","uri":"test://smpte","name":"Bars"}' > /dev/null || fail "adding a test source"
for _ in $(seq 1 30); do
    state=$(api GET /api/status | jq -r '.sources[]? | select(.id=="bars") | .state')
    [ "$state" = "live" ] && break
    sleep 1
done
[ "$state" = "live" ] || fail "the test source never went live (last state: $state)"
api POST /api/take '{"source":"bars"}' > /dev/null || fail "take"
api POST /api/outputs '{"id":"check","uri":"rtmp://127.0.0.1:19350/live/program","policy":"own"}' > /dev/null \
    || fail "adding the RTMP output"
echo "on air, streaming for $SECS s"

end=$(( $(date +%s) + SECS ))
while [ "$(date +%s)" -lt "$end" ]; do
    kill -0 "$mixer" 2>/dev/null || fail "the mixer exited while on air"
    api GET /api/status > /dev/null || fail "/api/status stopped answering"
    sleep 5
done
kill -0 "$mixer" 2>/dev/null || fail "the mixer exited"
api DELETE /api/outputs/check > /dev/null || true
sleep 3

ffprobe -v error -show_entries stream=codec_name,codec_type:format=duration -of json out.flv > probe.json \
    || fail "ffprobe could not read the recording"
duration=$(jq -r '.format.duration | tonumber | floor' probe.json)
codecs=$(jq -r '[.streams[].codec_name] | sort | join(",")' probe.json)
echo "recorded ${duration} s, streams: $codecs"
[ "$duration" -ge $(( SECS - 5 )) ] || fail "only ${duration} s of programme in ${SECS} s"
[ "$codecs" = "aac,h264" ] || fail "expected H.264 and AAC, got $codecs"
if grep -E "panicked|force-live.*not found" mixer.log; then
    fail "the mixer's log has a panic"
fi
echo "PASS: the server archive ran for ${SECS} s on $(gst-inspect-1.0 --version | sed -n 2p)"
