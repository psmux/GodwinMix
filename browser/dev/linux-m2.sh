#!/bin/sh
# Milestone 2 in the Linux dev image: the sidecar streams raw Matroska on stdout
# exactly as an `exec:` source would consume it. Run with the :gst image.
set -e
cd /work/browser
export CEF_PATH=/cefcache
cargo build --release 2>&1 | grep -E '^(error|warning: unused|\s+-->|\s+Finished)' -A6 | head -40
BIN=/work/browser/target/release/liveboxmix-browser
export LD_LIBRARY_PATH=/work/browser/target/release
RES=/work/browser/target/release

OUT=/work/browser/dev/out
rm -rf "$OUT" && mkdir -p "$OUT"
Xvfb :99 -screen 0 1280x720x24 -nolisten tcp >/dev/null 2>&1 &
sleep 1
export XDG_RUNTIME_DIR=/tmp/xdg; mkdir -p $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
pulseaudio --start --exit-idle-time=-1 >/dev/null 2>&1 || true
pactl load-module module-null-sink sink_name=lbx >/dev/null 2>&1 || true

# stdout is the stream; stderr is the log. That separation is the contract.
DISPLAY=:99 PULSE_SINK=lbx timeout 40 "$BIN" \
  --url file:///work/browser/test/page.html --width 1280 --height 720 --fps 30 --seconds 8 \
  --resources-dir "$RES" --locales-dir "$RES/locales" > "$OUT/stream.mkv" 2> "$OUT/run.log"
echo "exit=$?"
grep '\[browser\]' "$OUT/run.log" | head -20
grep -vE 'dbus|\[browser\]' "$OUT/run.log" | grep -E 'ERROR|FATAL|panick' | head -6

echo "--- stream ---"
echo "bytes: $(stat -c%s "$OUT/stream.mkv" 2>/dev/null || echo 0)"
ffprobe -hide_banner -loglevel error \
  -show_entries stream=codec_type,codec_name,pix_fmt,width,height,r_frame_rate,sample_fmt,sample_rate,channels \
  -of default=noprint_wrappers=1 "$OUT/stream.mkv" 2>&1 | head -12
echo "--- decoded video frames (expect about 8 s x 30 fps) ---"
ffprobe -hide_banner -loglevel error -select_streams v -count_frames \
  -show_entries stream=nb_read_frames -of default=noprint_wrappers=1:nokey=1 "$OUT/stream.mkv" 2>&1 | head -1
echo "--- picture of the last frame + audio identity ---"
ffmpeg -hide_banner -loglevel error -y -sseof -0.1 -i "$OUT/stream.mkv" -frames:v 1 "$OUT/m2_last.png" && echo "png: $OUT/m2_last.png"
ffmpeg -hide_banner -loglevel info -i "$OUT/stream.mkv" -vn \
  -af aspectralstats=measure=centroid,ametadata=print:key=lavfi.aspectralstats.1.centroid -f null - 2>&1 \
  | grep -o 'centroid=[0-9.]*' | awk -F= '{s+=$2;n++} END{if(n) printf "audio centroid: mean=%.0f Hz over %d windows (page tone is 660)\n", s/n, n; else print "no audio measured"}'
