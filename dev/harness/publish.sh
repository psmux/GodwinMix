#!/usr/bin/env bash
# Publish a test pattern to a running listener, so the acceptance line can be
# checked by hand and by the smoke script.
#
#   dev/harness/publish.sh rtmp [port] [app/key]   publish RTMP to this machine
#   dev/harness/publish.sh srt  [port]             publish SRT, caller mode
#   dev/harness/publish.sh both [rtmp port] [srt port]
#
# It runs in the foreground until interrupted, so put a `&` after it when you
# want it in the background. With no listener there, it exits non zero and says
# what was not reachable.
#
#   dev/harness/publish.sh rtmp 1935 &
#   gmx source add phone --type ingest/rtmp
#   gmx take phone
#
# The pattern is SMPTE bars with a 440 Hz tone, 320x240 at 30, encoded with
# x264enc at a low bitrate: enough to prove a path works, small enough to run
# beside the mixer on a laptop.
set -uo pipefail

mode=${1:-rtmp}
# Replace this shell with gst-launch for a single protocol, so the process id a
# caller has is the one publishing: `kill $!` then stops the publisher rather
# than leaving it orphaned behind a dead wrapper. In `both` mode there are two
# children, so the wrapper stays and traps.
RUN=exec

have() { command -v "$1" >/dev/null 2>&1; }

if ! have gst-launch-1.0; then
  echo "gst-launch-1.0 is not on PATH, so there is nothing to publish with." >&2
  echo "Install GStreamer, or publish from OBS instead: the address is the same." >&2
  exit 2
fi

missing=""
for element in videotestsrc audiotestsrc x264enc; do
  gst-inspect-1.0 "$element" >/dev/null 2>&1 || missing="$missing $element"
done
if [ -n "$missing" ]; then
  echo "this build of GStreamer is missing:$missing" >&2
  echo "skipped: nothing to publish with" >&2
  exit 2
fi

# The two halves of the pattern, shared by both protocols.
video="videotestsrc is-live=true pattern=smpte ! video/x-raw,width=320,height=240,framerate=30/1 \
 ! x264enc tune=zerolatency bitrate=800 key-int-max=30 ! h264parse"
audio="audiotestsrc is-live=true wave=sine freq=440 volume=0.2 ! audioconvert ! audioresample"

publish_rtmp() {
  local port=$1 path=$2
  if ! gst-inspect-1.0 rtmp2sink >/dev/null 2>&1; then
    echo "this build of GStreamer has no rtmp2sink; skipped" >&2
    return 2
  fi
  echo "publishing a test pattern to rtmp://127.0.0.1:$port/$path"
  # shellcheck disable=SC2086
  $RUN gst-launch-1.0 -q flvmux name=mux streamable=true \
    ! rtmp2sink "location=rtmp://127.0.0.1:$port/$path" \
    $video ! mux. \
    $audio ! "audio/x-raw,rate=44100" ! avenc_aac ! aacparse ! mux.
}

publish_srt() {
  local port=$1
  if ! gst-inspect-1.0 srtsink >/dev/null 2>&1 || ! gst-inspect-1.0 mpegtsmux >/dev/null 2>&1; then
    echo "this build of GStreamer has no srtsink or mpegtsmux; skipped" >&2
    return 2
  fi
  echo "publishing a test pattern to srt://127.0.0.1:$port (caller mode)"
  # shellcheck disable=SC2086
  $RUN gst-launch-1.0 -q mpegtsmux name=mux \
    ! srtsink "uri=srt://127.0.0.1:$port?mode=caller&latency=125" wait-for-connection=false \
    $video ! mux. \
    $audio ! avenc_aac ! aacparse ! mux.
}

case "$mode" in
  rtmp)
    publish_rtmp "${2:-1935}" "${3:-live/test}"
    ;;
  srt)
    publish_srt "${2:-9000}"
    ;;
  both)
    RUN=""
    publish_rtmp "${2:-1935}" "live/test" &
    rtmp_pid=$!
    publish_srt "${3:-9000}" &
    srt_pid=$!
    trap 'kill $rtmp_pid $srt_pid 2>/dev/null' EXIT INT TERM
    wait
    ;;
  *)
    echo "usage: $(basename "$0") rtmp|srt|both [port] [app/key]" >&2
    echo "  rtmp [port] [app/key]   default 1935 live/test" >&2
    echo "  srt  [port]             default 9000, caller mode" >&2
    echo "  both [rtmp port] [srt port]" >&2
    exit 2
    ;;
esac
