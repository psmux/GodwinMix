#!/bin/bash
set -e
cd /work/browser; export CEF_PATH=/cefcache
cargo build --release 2>&1 | grep -E '^(error|\s+-->)' -A6 | head -20 || true
BIN=/work/browser/target/release/godwinmix-browser; OUT=/work/browser/dev/out
Xvfb :99 -screen 0 1280x720x24 -nolisten tcp >/dev/null 2>&1 &
sleep 1; export DISPLAY=:99
echo "=== audio pts vs arrival ==="
timeout 60 "$BIN" --url file:///work/browser/test/sync.html --width 640 --height 360 --fps 30 --seconds 5 > /dev/null 2> "$OUT/pts.log" || true
grep 'pts is' "$OUT/pts.log"
echo "=== H.264 page ==="
timeout 60 "$BIN" --url file:///work/browser/test/video-mp4.html --width 1280 --height 720 --fps 30 --seconds 6 > "$OUT/mp4.mkv" 2> "$OUT/mp4.log" || true
ffmpeg -hide_banner -loglevel error -y -ss 4 -i "$OUT/mp4.mkv" -frames:v 1 "$OUT/mp4_frame.png" && echo "frame saved"
ffmpeg -hide_banner -ss 4 -t 1 -i "$OUT/mp4.mkv" -vf signalstats,metadata=print -f null - 2>&1 | grep -oE 'YAVG=[0-9.]+' | head -3 | tr '\n' ' '; echo
