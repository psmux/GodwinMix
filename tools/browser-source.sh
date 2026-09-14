#!/bin/sh
# Capture a web page, picture and sound, and write it to stdout for GodwinMix.
#
# Used as an `exec:` source:
#
#   godwinmix ctl source add site \
#     "exec:/path/to/browser-source.sh https://example.com/page"
#
# A real Chromium renders the page on a virtual display while its audio goes to
# a null sink we then record. That means everything Chrome can do works,
# including DRM-protected players, WebGL and WebAudio, which is the reason for
# using a whole browser rather than a lighter renderer.
#
# Linux only: it needs Xvfb and PulseAudio. On a machine with no GPU Chromium
# falls back to software rendering on its own, so the same command works either
# way; pass --enable-gpu through CHROME_FLAGS where hardware is available.
#
# Requires: xvfb, chromium (or chrome), pulseaudio, ffmpeg.
set -eu

URL="${1:?usage: browser-source.sh <url> [width] [height] [fps]}"
W="${2:-1280}"
H="${3:-720}"
FPS="${4:-30}"
# A display number unlikely to collide when several of these run at once.
DISP="${BROWSER_SOURCE_DISPLAY:-$((90 + (($$ % 60))))}"
SINK="gmxcap$$"
PROFILE="$(mktemp -d)"

CHROME="${CHROME_BIN:-}"
if [ -z "$CHROME" ]; then
  for c in chromium chromium-browser google-chrome google-chrome-stable; do
    command -v "$c" >/dev/null 2>&1 && CHROME="$c" && break
  done
fi
[ -n "$CHROME" ] || { echo "no chromium/chrome found on PATH" >&2; exit 1; }

# Everything started here is torn down together, so a source that is removed
# does not leave a browser and an X server behind.
cleanup() {
  [ -n "${FFMPEG_PID:-}" ] && kill "$FFMPEG_PID" 2>/dev/null || true
  [ -n "${CHROME_PID:-}" ] && kill "$CHROME_PID" 2>/dev/null || true
  [ -n "${XVFB_PID:-}"   ] && kill "$XVFB_PID"   2>/dev/null || true
  pactl unload-module "${SINK_MODULE:-}" 2>/dev/null || true
  rm -rf "$PROFILE" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp/gmx-xdg-$$}"
mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"

# A sink with no hardware behind it gives the tab somewhere to play, and its
# monitor is what we record.
pulseaudio --start --exit-idle-time=-1 >/dev/null 2>&1 || true
SINK_MODULE=$(pactl load-module module-null-sink sink_name="$SINK" 2>/dev/null || echo "")

Xvfb ":$DISP" -screen 0 "${W}x${H}x24" -nolisten tcp >/dev/null 2>&1 &
XVFB_PID=$!
sleep 1

DISPLAY=":$DISP" PULSE_SINK="$SINK" "$CHROME" \
  --no-sandbox --no-first-run --disable-infobars --disable-translate \
  --kiosk --window-position=0,0 --window-size="$W,$H" \
  --autoplay-policy=no-user-gesture-required \
  --disable-features=TranslateUI \
  --user-data-dir="$PROFILE" \
  ${CHROME_FLAGS:-} \
  "$URL" >/dev/null 2>&1 &
CHROME_PID=$!
# Let the page lay out and start playing before the first frame is grabbed.
sleep "${BROWSER_SOURCE_SETTLE:-5}"

# MPEG-TS on stdout is what the mixer's exec source expects. Encoding here and
# decoding there costs a codec round trip; it is the price of the process
# boundary, and it keeps a browser crash away from the encoder.
# Deliberately not `exec`: replacing this shell would discard the trap above,
# and the browser and X server would outlive the source. Every add and remove
# would then leak a Chromium.
ffmpeg -hide_banner -loglevel error \
  -f x11grab -framerate "$FPS" -video_size "${W}x${H}" -i ":$DISP" \
  -f pulse -i "$SINK.monitor" \
  -c:v libx264 -preset "${X264_PRESET:-veryfast}" -tune zerolatency \
  -b:v "${VIDEO_BITRATE:-6000k}" -g "$((FPS * 2))" -pix_fmt yuv420p \
  -c:a aac -b:a 160k -ar 48000 -ac 2 \
  -f mpegts -flush_packets 1 - &
FFMPEG_PID=$!
wait "$FFMPEG_PID"
