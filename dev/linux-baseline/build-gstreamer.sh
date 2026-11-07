#!/usr/bin/env bash
#
# Build GStreamer 1.28 and the few libraries Ubuntu 22.04 has too old, into one
# prefix, for the Linux baseline image (dev/linux-baseline/Dockerfile).
#
#   build-gstreamer.sh /opt/gst
#
# Every source is a release tarball checked against a SHA-256 written here, so
# the image is the same whoever builds it. Plugins whose library the image
# lacks are left out by meson; dev/bundle-gstreamer.sh fails afterwards on any
# the app needs, so a missing one cannot ship quietly.
#
# Built here rather than taken from the distribution:
#   * GStreamer 1.28.9, all seven modules. 22.04 has 1.20.
#   * libnice 0.1.24, because webrtcbin in 1.28 needs 0.1.23 or later.
#   * SVT-AV1 3.1.2, because svtav1enc needs 1.1 or later and 22.04 has 0.9.
#   * FFmpeg 7.1.5, static and private to gst-libav, the way
#     dev/build-linux-libav.sh did it on the old runner: decoders for any input,
#     and only the AAC encoder, the one the codec catalogue selects.

set -euo pipefail

PREFIX="${1:?usage: build-gstreamer.sh <prefix>}"
GST=1.28.9
JOBS="$(nproc)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

declare -A SHA=(
    [gstreamer-$GST.tar.xz]=6dbf744f583131a34845cc641e803843766a9ba80ce00378a9043fd6d8d5fb38
    [gst-plugins-base-$GST.tar.xz]=c1fda668b520e6aef763c0eb962c032c9fe52bce322e8b00c69b4e9e847bddc1
    [gst-plugins-good-$GST.tar.xz]=e518cd97483706a252e95e60312a22d23dec6f5e7673af6f5bde719c3a9ca7f4
    [gst-plugins-bad-$GST.tar.xz]=4a4e0b7bf84f868378c48f143a7ac3a98923b488a5b74f1a35ba46bc3c43f6b1
    [gst-plugins-ugly-$GST.tar.xz]=550a52536962186f5488e06b5c08f31ae0f18400aee32a3385156d31cb4b1189
    [gst-libav-$GST.tar.xz]=4b3df21fa70972a80891d2298cf32c14a62c3db4483d545c7ac018230399186b
    [gst-rtsp-server-$GST.tar.xz]=b87ed564d857a226a3135bd2c42eae7b40eb64031cb9cbce124722af92cc8b9d
    [ffmpeg-7.1.5.tar.xz]=de668509caf9e35e3cd162473441fdb29538c6d96ed080292b3cf9e6fc5d558f
    [libnice-0.1.24.tar.gz]=1da5ac13ed5d4e175e0d2d46ad3748c6635244f8f3eb2b8e31578ef59aa2ddce
    [SVT-AV1-v3.1.2.tar.gz]=d0d73bfea42fdcc1222272bf2b0e2319e9df5574721298090c3d28315586ecb1
)

# Fetch one tarball, check it, unpack it, and print the directory it made.
fetch() {
    local url="$1" file="${1##*/}" dir
    curl -fsSL --retry 5 --retry-all-errors -o "$WORK/$file" "$url" >&2
    echo "${SHA[$file]}  $WORK/$file" | sha256sum -c --quiet - >&2 \
        || { echo "$file does not match its pinned SHA-256" >&2; exit 1; }
    tar -xf "$WORK/$file" -C "$WORK"
    dir="$(tar -tf "$WORK/$file" | head -1 | cut -d/ -f1)"
    echo "$WORK/$dir"
}

# Everything lands in lib/, never lib/<multiarch>/, so the trimmer and the
# loader find the prefix's libraries before the system's.
meson_build() {
    local src="$1"; shift
    meson setup "$src/_build" "$src" --prefix="$PREFIX" --libdir=lib \
        --buildtype=release -Db_lto=false --wrap-mode=nodownload "$@"
    meson compile -C "$src/_build" -j "$JOBS"
    meson install -C "$src/_build"
}

export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig"
export LD_LIBRARY_PATH="$PREFIX/lib"
export PATH="$PREFIX/bin:$PATH"
mkdir -p "$PREFIX"

GSTSRC=https://gstreamer.freedesktop.org/src

# --- the libraries 22.04 has too old -----------------------------------------

svt="$(fetch https://gitlab.com/AOMediaCodec/SVT-AV1/-/archive/v3.1.2/SVT-AV1-v3.1.2.tar.gz)"
# SVT-AV1 picks its AVX2 and AVX-512 kernels at run time, so a baseline
# build still runs, more slowly, on a CPU without them.
cmake -S "$svt" -B "$svt/_build" -G Ninja -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$PREFIX" -DCMAKE_INSTALL_LIBDIR=lib \
    -DBUILD_SHARED_LIBS=ON -DBUILD_APPS=OFF -DBUILD_DEC=OFF -DBUILD_TESTING=OFF
cmake --build "$svt/_build" -j "$JOBS"
cmake --install "$svt/_build"

nice="$(fetch https://gitlab.freedesktop.org/libnice/libnice/-/archive/0.1.24/libnice-0.1.24.tar.gz)"
meson_build "$nice" -Dgupnp=disabled -Dgstreamer=disabled -Dexamples=disabled \
    -Dtests=disabled -Dintrospection=disabled -Dcrypto-library=openssl

ffmpeg="$(fetch https://ffmpeg.org/releases/ffmpeg-7.1.5.tar.xz)"
PRIVATE="$WORK/ffmpeg-private"
(
    cd "$ffmpeg"
    ./configure --prefix="$PRIVATE" --disable-autodetect --disable-programs \
        --disable-doc --disable-debug --enable-pic --disable-shared --enable-static \
        --enable-zlib --enable-bzlib --enable-lzma \
        --disable-encoders --enable-encoder=aac \
        --disable-filters --enable-filter=buffer,buffersink,yadif,scale,format,ssim,psnr
    make -j"$JOBS"
    make install
)

# --- GStreamer ----------------------------------------------------------------

COMMON=(-Dexamples=disabled -Dtests=disabled -Ddoc=disabled -Dintrospection=disabled
        -Dnls=disabled)

meson_build "$(fetch "$GSTSRC/gstreamer/gstreamer-$GST.tar.xz")" "${COMMON[@]}" \
    -Dbenchmarks=disabled -Dtools=enabled -Dptp-helper=disabled

meson_build "$(fetch "$GSTSRC/gst-plugins-base/gst-plugins-base-$GST.tar.xz")" "${COMMON[@]}" \
    -Dgl_api=opengl,gles2 -Dgl_platform=glx,egl -Dgl_winsys=x11,egl,wayland,gbm \
    -Dcdparanoia=disabled -Dtremor=disabled -Dlibvisual=disabled

meson_build "$(fetch "$GSTSRC/gst-plugins-good/gst-plugins-good-$GST.tar.xz")" "${COMMON[@]}" \
    -Dqt5=disabled -Dqt6=disabled -Dgtk3=disabled -Daalib=disabled -Dlibcaca=disabled \
    -Ddv=disabled -Ddv1394=disabled -Dshout2=disabled -Dtaglib=disabled -Dtwolame=disabled \
    -Dwavpack=disabled -Djack=disabled -Damrnb=disabled -Damrwbdec=disabled

meson_build "$(fetch "$GSTSRC/gst-plugins-ugly/gst-plugins-ugly-$GST.tar.xz")" "${COMMON[@]}" \
    -Dgpl=enabled -Dx264=enabled -Da52dec=enabled -Dmpeg2dec=disabled -Dsidplay=disabled \
    -Dcdio=disabled -Ddvdread=disabled

# gst-plugins-bad: everything its libraries allow, and these by name, because
# a build that silently lost them is the one that must fail here. va and
# nvcodec need nothing at build time: they open the GPU driver when a mixer
# starts, and register no encoder on a machine without one.
meson_build "$(fetch "$GSTSRC/gst-plugins-bad/gst-plugins-bad-$GST.tar.xz")" "${COMMON[@]}" \
    -Dgpl=enabled -Dwebrtc=enabled -Ddtls=enabled -Dsrtp=enabled -Dsrt=enabled \
    -Drtmp=enabled -Dva=enabled -Dnvcodec=enabled -Dv4l2codecs=enabled \
    -Dopenh264=enabled -Dx265=enabled -Dsvtav1=enabled -Dde265=enabled \
    -Dvulkan=disabled -Dqt6d3d11=disabled -Dmsdk=disabled -Dqsv=disabled \
    -Dopencv=disabled -Dwpe=disabled -Dgtk3=disabled -Dmagicleap=disabled \
    -Dopenaptx=disabled -Dzxing=disabled -Dwebrtcdsp=disabled -Dladspa=disabled \
    -Dlv2=disabled -Dspandsp=disabled -Dzbar=disabled -Dteletext=disabled \
    -Dmpeg2enc=disabled -Dmplex=disabled -Dresindvd=disabled -Dflite=disabled \
    -Dmusepack=disabled -Dneon=disabled -Dopenmpt=disabled -Dopenexr=disabled \
    -Dsndfile=disabled -Dsoundtouch=disabled -Dchromaprint=disabled \
    -Dgs=disabled -Daja=disabled -Dlcevcdecoder=disabled -Dlcevcencoder=disabled \
    -Dcuda-nvmm=disabled -Danalyticsoverlay=disabled -Donnx=disabled

# gst-libav with FFmpeg linked in statically, so the bundle carries no
# FFmpeg shared libraries and none of their optional dependency stacks.
libav="$(fetch "$GSTSRC/gst-libav/gst-libav-$GST.tar.xz")"
python3 - "$libav/meson.build" <<'PY'
import sys
from pathlib import Path
path = Path(sys.argv[1])
text = path.read_text()
for name in ("libavfilter", "libavformat", "libavcodec", "libavutil"):
    old = f"dependency('{name}',"
    if text.count(old) != 1:
        raise SystemExit(f"expected one dependency declaration for {name}")
    text = text.replace(old, old + " static: true,")
path.write_text(text)
PY
PKG_CONFIG_PATH="$PRIVATE/lib/pkgconfig:$PKG_CONFIG_PATH" meson_build "$libav" "${COMMON[@]}"
if objdump -p "$PREFIX/lib/gstreamer-1.0/libgstlibav.so" \
    | grep -E 'NEEDED.*lib(avcodec|avfilter|avformat|avutil|swscale|swresample)'; then
    echo "gst-libav still imports shared FFmpeg libraries" >&2
    exit 1
fi

meson_build "$(fetch "$GSTSRC/gst-rtsp-server/gst-rtsp-server-$GST.tar.xz")" "${COMMON[@]}"

# --- what the trimmer looks for beside the prefix ------------------------------

MULTIARCH="$(gcc -print-multiarch)"
# The system's libraries, searched after the prefix's own: libx264, libvpx,
# libsrt and the rest of what the plugins link.
ln -sfn "/usr/lib/$MULTIARCH" "$PREFIX/lib/$MULTIARCH"
# TLS for souphttpsrc and curl comes from GIO modules loaded at run time.
[[ ! -d "/usr/lib/$MULTIARCH/gio" ]] || ln -sfn "/usr/lib/$MULTIARCH/gio" "$PREFIX/lib/gio"
# Screen capture on Wayland. PipeWire builds this plugin, not GStreamer, and
# the 22.04 copy loads in 1.28 because the plugin ABI has not changed.
for plugin in "/usr/lib/$MULTIARCH"/gstreamer-1.0/libgstpipewire.so; do
    [[ -f "$plugin" ]] && cp "$plugin" "$PREFIX/lib/gstreamer-1.0/"
done

echo
GST_REGISTRY="$WORK/registry.bin" GST_PLUGIN_SYSTEM_PATH="$PREFIX/lib/gstreamer-1.0" \
    "$PREFIX/bin/gst-inspect-1.0" --version
echo "plugins: $(ls "$PREFIX/lib/gstreamer-1.0" | wc -l)"
