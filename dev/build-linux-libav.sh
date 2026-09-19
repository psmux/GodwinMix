#!/usr/bin/env bash
# Build gst-libav with private static FFmpeg libraries, avoiding the distro's
# optional speech, rendering and scientific dependency stacks in an AppImage.
set -euo pipefail
[[ $(uname -s) == Linux ]] || { echo 'This builder requires Linux.' >&2; exit 1; }
OUT=${1:?usage: build-linux-libav.sh output-prefix}
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"
# apt authenticates source hashes using the distribution's signed indexes.
# Enable deb-src in the runner before calling this script.
apt-get source ffmpeg gst-libav1.0
FFMPEG=$(find "$WORK" -maxdepth 1 -type d -name 'ffmpeg-*' | head -n 1)
LIBAV=$(find "$WORK" -maxdepth 1 -type d -name 'gst-libav1.0-*' | head -n 1)
[[ -n "$FFMPEG" && -n "$LIBAV" ]] || { echo 'Source packages were not extracted.' >&2; exit 1; }
# The Debian patch tracks shared FFmpeg files for registry invalidation and
# expects a macro supplied by debian/rules. This plugin embeds FFmpeg instead,
# so remove only that packaging patch while retaining all security patches.
DEPENDENCY_PATCH="$LIBAV/debian/patches/00_plugin-dependencies.patch"
if [[ -f "$DEPENDENCY_PATCH" ]]; then
    patch -d "$LIBAV" -p1 -R < "$DEPENDENCY_PATCH"
fi
PRIVATE="$WORK/private"
cd "$FFMPEG"
# Keep native codecs, demuxers and filters. Only external optional libraries
# and tools disappear. GStreamer provides the platform hardware codecs.
./configure --prefix="$PRIVATE" --disable-autodetect --disable-programs \
    --disable-doc --disable-debug --enable-pic --disable-shared --enable-static \
    --enable-zlib --enable-bzlib --enable-lzma
make -j"$(nproc)"
make install
export PKG_CONFIG_PATH="$PRIVATE/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
meson setup "$WORK/plugin-build" "$LIBAV" --prefix="$OUT" --libdir=lib \
    --buildtype=release -Ddefault_library=shared -Dprefer_static=true \
    -Dtests=disabled -Ddoc=disabled --wrap-mode=nofallback
meson compile -C "$WORK/plugin-build"
meson install -C "$WORK/plugin-build"
# Fail rather than accidentally restore the large shared dependency graph.
if objdump -p "$OUT/lib/gstreamer-1.0/libgstlibav.so" | \
    grep -E 'NEEDED.*lib(avcodec|avfilter|avformat|avutil|swscale|swresample)'; then
    echo 'gst-libav still imports shared FFmpeg libraries; inspect the build configuration.' >&2
    exit 1
fi
# A source prefix composed of system plugin links and our private libav.
# gst_trim copies only the selected plugins and their actual dependency graph.
MULTIARCH=$(gcc -print-multiarch)
mkdir -p "$OUT/bin" "$OUT/libexec/gstreamer-1.0"
ln -s /usr/lib/"$MULTIARCH" "$OUT/lib/$MULTIARCH"
for plugin in /usr/lib/"$MULTIARCH"/gstreamer-1.0/*.so; do
    [[ $(basename "$plugin") == libgstlibav.so ]] && continue
    ln -s "$plugin" "$OUT/lib/gstreamer-1.0/$(basename "$plugin")"
done
for binary in gst-inspect-1.0 gst-launch-1.0 gst-device-monitor-1.0; do
    [[ -f /usr/bin/$binary ]] && ln -s /usr/bin/"$binary" "$OUT/bin/$binary"
done
SCANNER=/usr/lib/"$MULTIARCH"/gstreamer1.0/gstreamer-1.0/gst-plugin-scanner
ln -s "$SCANNER" "$OUT/libexec/gstreamer-1.0/gst-plugin-scanner"
# TLS modules are loaded by GIO at runtime rather than by ELF imports.
[[ ! -d /usr/lib/"$MULTIARCH"/gio ]] || ln -s /usr/lib/"$MULTIARCH"/gio "$OUT/lib/gio"
GST_REGISTRY="$WORK/registry.bin" GST_PLUGIN_PATH="$OUT/lib/gstreamer-1.0" \
    gst-inspect-1.0 libav > "$WORK/inspect.txt"
head -n 18 "$WORK/inspect.txt"
