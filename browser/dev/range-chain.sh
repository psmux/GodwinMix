#!/bin/bash
# Reproduce the mixer's source chain stage by stage and read the background
# luma of frame 30 (raw stream has Y=35 there). Whichever variant reports ~19
# contains the element that expands limited range to full.
export PATH="/opt/homebrew/bin:/usr/bin:/bin"
OUT="$(cd "$(dirname "$0")" && pwd)/out"; cd "$OUT"
CANVAS="video/x-raw,format=I420,width=1280,height=720,framerate=30/1,pixel-aspect-ratio=1/1,interlace-mode=progressive"
NORM="videorate skip-to-first=true ! videoconvert ! videoscale ! $CANVAS"
bg() { # $1 name, rest = pipeline after the source
  local name=$1; shift
  rm -f "$name.mkv"
  gst-launch-1.0 -q -e "$@" ! identity eos-after=40 ! vtenc_h264_hw realtime=true bitrate=6000 ! h264parse ! matroskamux ! filesink location="$name.mkv" >/dev/null 2>&1
  ffmpeg -hide_banner -loglevel error -y -i "$name.mkv" -vf "select=eq(n\,30)" -frames:v 1 -f rawvideo -pix_fmt yuv420p "$name.yuv" 2>/dev/null
  python3 -c "
import sys; w=1280
try:
    y=open('$name.yuv','rb').read()[:w*720]; print('%-28s bg Y=%3d  white=%3d' % ('$name', y[700*w+1200], max(y[100*w:101*w])))
except Exception as e: print('%-28s FAILED %s' % ('$name', e))"
}
[ -s remux.mkv ] || ffmpeg -hide_banner -loglevel error -y -t 3 -i stream.mkv -c copy -f matroska remux.mkv
bg V0_demux_direct        filesrc location=stream.mkv ! matroskademux ! queue
bg V1_decodebin_norm      filesrc location=stream.mkv ! decodebin ! $NORM
bg V2_remux_decodebin_norm filesrc location=remux.mkv ! decodebin ! $NORM
bg V3_remux_norm_comp     filesrc location=remux.mkv ! decodebin ! $NORM ! compositor name=c ! $CANVAS
bg V4_remux_norm_comp_slate filesrc location=remux.mkv ! decodebin ! $NORM ! compositor name=c ! $CANVAS videotestsrc pattern=black num-buffers=45 ! $CANVAS ! c.
bg V5_remux_norm_slatefirst videotestsrc pattern=black num-buffers=45 ! $CANVAS ! compositor name=c ! $CANVAS filesrc location=remux.mkv ! decodebin ! $NORM ! c.
echo "=== caps as decodebin exposes them ==="
for f in stream.mkv remux.mkv; do printf -- '%-11s ' $f; gst-discoverer-1.0 -v $f 2>/dev/null | grep -m1 -oE 'video/x-raw[^\n]*colorimetry=\(string\)[^,]+' | grep -oE 'colorimetry=[^,]+' ; done
