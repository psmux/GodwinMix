#!/bin/sh
# A GodwinMix plugin in shell.
#
# The contract, in full: JSON-RPC 2.0 objects one per line, the core's on
# stdin and yours on stderr, media on stdout. Anything on stderr that is not
# JSON goes to the core's log tagged with this instance, so `echo` is a
# perfectly good debugger.
set -eu

say() { printf '%s\n' "$1" >&2; }

# The plugin speaks first. The core answers with the canvas, the transport it
# picked and the params it validated against settings.json.
say '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"plugin":"{{name}}","version":"0.1.0","api":1,"transports":["container"],"provides":[]}}'
read -r READY
say '{"jsonrpc":"2.0","method":"initialized"}'

# What the core told us. Read with sed rather than a JSON parser, because this
# template has no dependencies; a real plugin uses its language's json module.
field() { printf '%s' "$READY" | sed -n "s/.*\"$1\":\([0-9]*\).*/\1/p"; }
WIDTH=$(field width);   WIDTH=${WIDTH:-1280}
HEIGHT=$(field height); HEIGHT=${HEIGHT:-720}
FPS=$(field fps);       FPS=${FPS:-30}
PATTERN=smpte

MEDIA_PID=""
start_media() {
  [ -n "$MEDIA_PID" ] && return 0
  # Whatever draws your picture goes here. It must write a container that
  # decodebin opens; streamable Matroska with raw I420 is the cheapest.
  gst-launch-1.0 -q \
    videotestsrc is-live=true pattern="$PATTERN" \
    ! "video/x-raw,format=I420,width=$WIDTH,height=$HEIGHT,framerate=$FPS/1" \
    ! matroskamux streamable=true \
    ! fdsink fd=1 &
  MEDIA_PID=$!
}

stop_media() {
  [ -z "$MEDIA_PID" ] && return 0
  kill "$MEDIA_PID" 2>/dev/null || true
  wait "$MEDIA_PID" 2>/dev/null || true
  MEDIA_PID=""
}

trap 'stop_media' EXIT INT TERM

# The main loop. Several requests may be in flight, and they may be answered in
# any order; answering in order is simplest and is what this does.
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"start"'*)
      start_media
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"latency_ms\":0}}"
      ;;
    *'"method":"stop"'*)
      stop_media
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{}}"
      ;;
    *'"method":"health"'*)
      # Answer this even while a slow start is pending: it is how the
      # supervisor tells a slow plugin from a dead one.
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"state\":\"ok\"}}"
      ;;
    *'"method":"configure"'*)
      # The pattern is chosen when the pipeline is built, so a change needs a
      # restart. Saying so is correct and costs one freeze frame; crashing is
      # not.
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"applied\":false,\"restart_required\":true,\"reason\":\"the pattern is fixed when the picture starts\"}}"
      ;;
    *'"method":"shutdown"'*)
      stop_media
      exit 0
      ;;
    *'"method":"configure_log"'*)
      # A notification, not a request. The core is telling you what level it
      # wants; there is nothing to answer.
      ;;
  esac
done
