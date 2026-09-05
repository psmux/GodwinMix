#!/bin/bash
# Build the sidecar against Arch's CEF (proprietary codecs on) and prove the
# H.264/AAC <video> plays, with sync intact. Run inside dev/Dockerfile.arch.
#
# Two layouts have to meet. The cef crate wants a CEF binary distribution
# (headers, libcef_dll sources, CMakeLists, runtime files flattened in one
# directory) and downloads the official one. Arch installs its own build of the
# same version under /usr/lib/cef. So: let the crate fetch and build against
# the official distribution once, then lay Arch's runtime files over it, both
# in the distribution (for rebuilds) and next to the binary (for this run).
set -e
cd /work/browser
export CEF_PATH=/cefarch
cargo build --release 2>&1 | grep -E '^(error|\s+-->)' -A6 | head -20 || true
DIST="$CEF_PATH/151.3.24/cef_linux_x86_64"
[ -d "$DIST" ] || { echo "no distribution at $DIST"; ls -R "$CEF_PATH" | head; exit 1; }
REL=/work/browser/target/release
ARCH=/usr/lib/cef
for d in "$DIST" "$REL"; do
  cp -f "$ARCH"/libcef.so "$ARCH"/*.pak "$ARCH"/icudtl.dat "$ARCH"/v8_context_snapshot.bin "$d"/
  for f in libEGL.so libGLESv2.so libvk_swiftshader.so libvulkan.so.1 vk_swiftshader_icd.json chrome-sandbox; do
    [ -e "$ARCH/$f" ] && cp -f "$ARCH/$f" "$d"/ || true
  done
  rm -rf "$d/locales"; cp -r "$ARCH/locales" "$d/locales"
done
echo "libcef now: $(ls -la $REL/libcef.so | awk '{print $5}') bytes; h264 symbols: $(strings $REL/libcef.so | grep -c 'H264Decoder\|ff_h264' || true)"

BIN=$REL/liveboxmix-browser; OUT=/work/browser/dev/out-arch; mkdir -p "$OUT"
Xvfb :99 -screen 0 1280x720x24 -nolisten tcp >/dev/null 2>&1 &
sleep 1; export DISPLAY=:99
for page in video-mp4 sync; do
  timeout 90 "$BIN" --url file:///work/browser/test/$page.html --width 1280 --height 720 --fps 30 --seconds 12 \
    > "$OUT/$page.mkv" 2> "$OUT/$page.log" || echo "$page: exit $?"
  grep -E 'done:|cef initialize failed|api hash' "$OUT/$page.log" | head -3 | sed "s/^/  [$page] /"
  python3 /work/browser/dev/measure-sync.py "$OUT/$page.mkv"
done
ffmpeg -hide_banner -loglevel error -y -ss 4 -i "$OUT/video-mp4.mkv" -frames:v 1 "$OUT/mp4_frame.png" && echo "frame: $OUT/mp4_frame.png"
