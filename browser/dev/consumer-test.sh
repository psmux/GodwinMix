#!/bin/bash
# Does the way the mixer reads the containerised sidecar's stdout starve it?
# Streams the H.264 page through the Docker wrapper into different readers
# and counts audio re-anchors (a re-anchor means Chromium lost audio time).
export PATH="/opt/homebrew/bin:/usr/bin:/bin"; export DOCKER_HOST=unix://$HOME/.colima/default/docker.sock
cd "$(dirname "$0")/../.."
S=(browser/dev/sidecar-docker.sh --url http://host.docker.internal:8090/video-mp4.html --width 1280 --height 720 --fps 30 --seconds 12)
run() { name=$1; shift
  ( sleep 50; pkill -f "gst-launch-1.0 fdsrc"; pkill -f "sidecar-docker.sh" ) & W=$!
  "${S[@]}" 2> browser/dev/out/c_$name.log | "$@" > /dev/null 2>&1; rc=$?
  kill $W 2>/dev/null; wait $W 2>/dev/null
  printf '%-22s reader exit=%s re-anchors=%s %s\n' "$name" "$rc" "$(grep -c re-anchored browser/dev/out/c_$name.log)" "$(grep -oE 'paints=[0-9]+' browser/dev/out/c_$name.log)"
}
run cat            cat
run fdsrc_fakesink gst-launch-1.0 fdsrc fd=0 ! fakesink sync=false
run fdsrc_decodebin gst-launch-1.0 fdsrc fd=0 ! decodebin name=d d. ! queue ! fakesink sync=false d. ! queue ! fakesink sync=false
run fdsrc_1M_queue gst-launch-1.0 fdsrc fd=0 blocksize=1048576 ! queue max-size-bytes=300000000 max-size-buffers=0 max-size-time=0 ! decodebin name=d d. ! queue ! fakesink sync=false d. ! queue ! fakesink sync=false
docker ps -q --filter ancestor=gmx-browser | xargs -r docker kill >/dev/null 2>&1; true
