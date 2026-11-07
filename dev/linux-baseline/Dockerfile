# The machine every Linux release artefact is built on, for x86_64 and arm64.
#
# Ubuntu 22.04 because its glibc, 2.35, is the oldest one in wide use that
# still has the webview the desktop app needs (WebKitGTK 4.1). Whatever is
# built here runs on Ubuntu 22.04 and later, Debian 12 and later, Raspberry Pi
# OS bookworm, Linux Mint 21 and later, and Fedora 36 and later. Built on a
# newer image, the 0.3.30 mixer needed glibc 2.43 and started on almost none
# of them.
#
# The distribution's GStreamer is 1.20, four years behind what the app is
# tested with, so 1.28 is built from the release tarballs into /opt/gst by
# build-gstreamer.sh. The Rust is compiled against that prefix and the desktop
# bundles a trimmed copy of it. A plain gcc here targets baseline x86-64, so
# nothing in the image needs a 2013 or newer CPU.
#
#   docker build -t gmx-linux-baseline dev/linux-baseline
#   docker run --rm -v "$PWD:/work" -w /work gmx-linux-baseline cargo build --release
#
# The release workflow builds this once per architecture and keeps it in
# GHCR under a tag made from these files, so it is rebuilt only when they
# change.

FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive LANG=C.UTF-8

# Build tools, the libraries the GStreamer plugins link, and what Tauri and
# the browser renderer need. Plugins whose library is missing here are simply
# not built; bundle-gstreamer.sh then fails on any the app requires.
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential pkg-config ca-certificates curl git unzip xz-utils file \
        python3 python3-pip bison flex nasm cmake ninja-build patchelf sudo \
        libglib2.0-dev libgirepository1.0-dev libssl-dev zlib1g-dev libbz2-dev liblzma-dev \
        liborc-0.4-dev libx11-dev libxext-dev libxv-dev libxfixes-dev libxdamage-dev \
        libxtst-dev libxi-dev libx11-xcb-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev \
        libwayland-dev wayland-protocols libegl-dev libgles-dev libgl-dev libgbm-dev libdrm-dev \
        libgudev-1.0-dev libudev-dev libasound2-dev libpulse-dev libjack-jackd2-dev \
        libv4l-dev libva-dev libvulkan-dev glslang-tools \
        libpng-dev libjpeg-turbo8-dev libwebp-dev librsvg2-dev libgdk-pixbuf-2.0-dev libcairo2-dev \
        libpango1.0-dev libogg-dev libvorbis-dev libopus-dev libflac-dev libmpg123-dev \
        libmp3lame-dev libvo-aacenc-dev libfaad-dev liba52-0.7.4-dev libspeex-dev \
        libvpx-dev libx264-dev libx265-dev libde265-dev libopenh264-dev libdav1d-dev \
        libsrt-gnutls-dev libsrtp2-dev librtmp-dev libsoup-3.0-dev libcurl4-gnutls-dev \
        libxml2-dev libsqlite3-dev libjson-glib-dev libnice-dev \
        libgnutls28-dev libgcrypt20-dev \
        gstreamer1.0-pipewire \
        libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libfuse2 \
    && rm -rf /var/lib/apt/lists/*

# GStreamer 1.28 asks for a newer meson than 22.04 has.
RUN pip3 install --no-cache-dir 'meson==1.9.1'

COPY build-gstreamer.sh /usr/local/bin/build-gstreamer.sh
RUN /usr/local/bin/build-gstreamer.sh /opt/gst

# The 1.28 prefix first, for pkg-config and for running anything built here.
ENV PKG_CONFIG_PATH=/opt/gst/lib/pkgconfig \
    PATH=/opt/gst/bin:/root/.cargo/bin:$PATH \
    LD_LIBRARY_PATH=/opt/gst/lib \
    GST_PLUGIN_SYSTEM_PATH=/opt/gst/lib/gstreamer-1.0 \
    GST_PLUGIN_SCANNER=/opt/gst/libexec/gstreamer-1.0/gst-plugin-scanner

RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
    && /root/.cargo/bin/rustup component add llvm-tools
