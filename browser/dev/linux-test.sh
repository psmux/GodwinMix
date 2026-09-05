#!/bin/sh
# Build and run milestone 1 inside the Linux dev image.
set -e
cd /work/browser
export CEF_PATH=/cefcache
cargo build --release 2>&1 | grep -E '^(error|warning: unused|\s+-->|\s+Finished)' -A6 | head -40
BIN=/work/browser/target/release/liveboxmix-browser
# Chromium needs libcef.so on the loader path and its resources findable.
LIBDIR=$(dirname "$(find /cefcache /work/browser/target -name libcef.so 2>/dev/null | head -1)")
RES=$(dirname "$(find /cefcache /work/browser/target -name icudtl.dat 2>/dev/null | head -1)")
echo "libcef in: $LIBDIR"; echo "resources in: $RES"
export LD_LIBRARY_PATH="$LIBDIR:$RES"
# Outputs go under the bind mount so they can be inspected from the host.
OUT=/work/browser/dev/out
rm -rf "$OUT" && mkdir -p "$OUT"
# Xvfb started directly: xvfb-run needs xauth, which the image does not carry.
Xvfb :99 -screen 0 1280x720x24 -nolisten tcp >/dev/null 2>&1 &
sleep 1
# Give Chromium a real (silent) output device. The audio handler taps the
# output stream, and with no device at all there may be no stream to tap.
export XDG_RUNTIME_DIR=/tmp/xdg; mkdir -p $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
pulseaudio --start --exit-idle-time=-1 >/dev/null 2>&1 || true
pactl load-module module-null-sink sink_name=lbx >/dev/null 2>&1 || true
# D-Bus errors are expected in a container with no session bus and are noise.
DISPLAY=:99 PULSE_SINK=lbx timeout 90 "$BIN" \
  --url file:///work/browser/test/page.html --width 1280 --height 720 --fps 30 --frames 150 \
  --out-dir "$OUT" --resources-dir "$RES" --locales-dir "$RES/locales" > "$OUT/run.log" 2>&1
echo "exit=$?"
grep '\[browser\]' "$OUT/run.log" | head -20
grep -vE 'dbus|\[browser\]' "$OUT/run.log" | grep -E 'ERROR|FATAL' | grep -v dbus | head -6
echo "--- results ---"
echo "frames dumped: $(ls "$OUT"/*.ppm 2>/dev/null | wc -l)"
echo "audio bytes  : $(stat -c%s "$OUT/audio.f32" 2>/dev/null || echo 0)"
F=$(ls "$OUT"/*.ppm 2>/dev/null | tail -1)
if [ -n "$F" ]; then
  echo "last frame   : $F"
  ffmpeg -hide_banner -loglevel info -i "$F" -vf signalstats,metadata=print:key=lavfi.signalstats.YAVG -f null - 2>&1 | grep -o 'YAVG=[0-9.]*' | head -1
  ffmpeg -hide_banner -loglevel error -i "$F" -y "$OUT/last.png" && echo "png: $OUT/last.png"
fi
if [ -s "$OUT/audio.f32" ]; then
  ffmpeg -hide_banner -loglevel info -f f32le -ar 48000 -ac 2 -i "$OUT/audio.f32" \
    -af aspectralstats=measure=centroid,ametadata=print:key=lavfi.aspectralstats.1.centroid -f null - 2>&1 \
    | grep -o 'centroid=[0-9.]*' | awk -F= '{s+=$2;n++} END{printf "audio centroid: mean=%.0f Hz over %d windows (page tone is 660)\n", s/n, n}'
fi
