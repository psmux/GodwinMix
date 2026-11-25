# GodwinMix, built from source in the image.
#
# Two stages. The builder carries the Rust toolchain and the GStreamer
# development headers; the runtime carries the mixer, the example config and
# the GStreamer plugin sets the pipelines actually use. Nothing else. The
# source build means the image can be produced from a clone with no CI
# artifact to fetch by hand, which is what the old recipe needed.
#
# Debian trixie on both sides so the glibc the binary was linked against is
# the glibc it runs on.
#
# Build from the repository root, because the builder copies the source tree:
#   docker build -f deploy/docker/Dockerfile -t godwinmix .

# ---------------------------------------------------------------- builder ---
FROM rust:1-trixie AS builder

# Development headers for the gstreamer crates, plus the base plugin
# development package (gstreamer-video, gstreamer-audio, gstreamer-app) and
# the bad one (gstreamer-play, used by pbutils). pkg-config finds them.
RUN apt-get update && apt-get install -y --no-install-recommends \
      libgstreamer1.0-dev \
      libgstreamer-plugins-base1.0-dev \
      libgstreamer-plugins-bad1.0-dev \
      pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src

# Copy the complete workspace, including plugin manifests and embedded assets.
# .dockerignore excludes local build output and private runtime files.
COPY . .
RUN cargo build --release --locked -p godwinmix --bins \
    && strip target/release/godwinmix target/release/gmx

# The person cutout's two models and ONNX Runtime for this architecture,
# pinned and checked by the same script the desktop installers use. The
# runtime image puts them where the mixer looks from /usr/local/bin.
RUN apt-get update && apt-get install -y --no-install-recommends python3 curl \
    && rm -rf /var/lib/apt/lists/* \
    && bash dev/fetch-models.sh --models /cutout/models --runtime /cutout/runtime

# The browser runs outside the mixer. Debian's WPE fallback can crash inside
# the mixer process, so the default image ships the existing CEF sidecar.
# The CEF is GodwinMix's own build with H.264 and AAC, the one the desktop
# packages carry (dev/cef-codecs.sh, checked by SHA-256 after the download).
# The official CEF plays neither, so a page with an H.264 video in it, or a
# superimposed page whose clip is AAC, stayed silent or black in the image.
# CEF_CODECS=off builds against the official CEF.
FROM builder AS browser-builder
ARG WITH_CEF=1
ARG CEF_CODECS=on
ENV CEF_PATH=/cef
RUN mkdir -p /browser-runtime \
    && if [ "$WITH_CEF" = "1" ]; then \
         apt-get update && apt-get install -y --no-install-recommends \
           cmake ninja-build libx11-dev libgtk-3-dev && \
         REPO=/src CEF_CODECS="$CEF_CODECS" bash -euo pipefail -c ' \
           source dev/cef-codecs.sh; \
           [ "$CEF_CODECS" = off ] || use_cef_codecs; \
           cargo build --manifest-path browser/Cargo.toml --release --locked --bin godwinmix-browser; \
           [ "$CEF_CODECS" = off ] || check_cef_codecs' && \
         cp browser/target/release/godwinmix-browser /browser-runtime/ && \
         find browser/target/release -maxdepth 1 -type f \
           \( -name '*.so*' -o -name '*.pak' -o -name '*.dat' -o -name '*.bin' \
              -o -name '*icd.json' -o -name 'chrome-sandbox' \) \
           -exec cp {} /browser-runtime/ \; && \
         cp -r browser/target/release/locales /browser-runtime/ && \
         find /cef -name LICENSE.txt -exec cp {} /browser-runtime/CEF-LICENSE.txt \; && \
         find /cef -name CREDITS.html -exec cp {} /browser-runtime/CEF-CREDITS.html \; ; \
       fi

# ---------------------------------------------------------------- runtime ---
FROM debian:trixie-slim AS runtime

# x264enc is the codec catalogue's preferred software H.264 encoder, and on
# Debian it ships inside gstreamer1.0-plugins-ugly. Build with
# --build-arg WITH_X264=0 if that package's licensing is a problem where the
# image runs; the programme is then encoded by openh264enc from
# gstreamer1.0-plugins-bad, or by a hardware encoder (VA, NVENC).
ARG WITH_X264=1
# gst-libav is a fallback only: the catalogue takes openh264, libde265,
# voaacenc and faad from gstreamer1.0-plugins-bad, mpg123 from good and
# a52dec from ugly first. Build with --build-arg WITH_LIBAV=0 for an image
# with no FFmpeg in it; E-AC-3, interlaced H.264 and ProRes clips then do
# not decode.
ARG WITH_LIBAV=1
# gstreamer1.0-wpe renders a web page through wpesrc, which is how a `web+`
# source works when no browser sidecar is installed next to the binary.
# It is an optional fallback for images built without CEF.
ARG WITH_WPE=0
ARG WITH_CEF=1

ENV DEBIAN_FRONTEND=noninteractive

# core: the registry and the element base classes.
# base: videoconvert, videoscale, audioconvert, audioresample, compositor
#       inputs, the parsers, playbin.
# good: rtmp2src and rtmp2sink, flvmux, matroska, the proxy elements, jpegenc
#       for the multiview mosaic, rtp, souphttpsrc for HLS and DASH.
# bad:  srtsrc and srtsink, the DASH and HLS demuxers, videoparsers, the
#       programme's AAC encoder (voaacenc), the software H.264, HEVC and AAC
#       decoders (openh264dec, libde265dec, faad), and the GL elements the
#       compositor uses when there is a context. fdkaacenc would be better
#       than voaacenc, but libfdk-aac is in Debian's non-free and is not here.
# libav: the fallback, below, for what nothing else decodes.
# tools: gst-inspect-1.0 and gst-launch-1.0, for `docker exec` when a pipeline
#       misbehaves. Half a megabyte and worth it on a headless box.
# x:    on Debian this is where pango lives, and with it textoverlay, which
#       draws every text source, lower third and ticker. Without it each of
#       those failed in the image with "No such element or plugin
#       'textoverlay'".
RUN apt-get update && apt-get install -y --no-install-recommends \
      gstreamer1.0-plugins-base \
      gstreamer1.0-plugins-good \
      gstreamer1.0-plugins-bad \
      gstreamer1.0-tools \
      gstreamer1.0-x \
      ca-certificates \
      curl \
    && if [ "$WITH_CEF" = "1" ]; then \
         apt-get install -y --no-install-recommends \
           libnss3 libnspr4 libatk1.0-0t64 libatk-bridge2.0-0t64 \
           libdrm2 libxkbcommon0 libxcomposite1 libxdamage1 libxfixes3 \
           libxrandr2 libgbm1 libasound2t64 libcups2t64 libgtk-3-0t64 \
           xvfb xauth xfonts-base libgl1-mesa-dri libegl1 libgles2 \
           fonts-dejavu-core fonts-liberation ; \
       fi \
    && if [ "$WITH_WPE" = "1" ]; then \
         apt-get install -y --no-install-recommends \
           gstreamer1.0-wpe gstreamer1.0-gl gstreamer1.0-x \
           xvfb xauth xfonts-base \
           libgl1-mesa-dri libegl1 libgles2 \
           fonts-dejavu-core fonts-liberation ; \
       fi \
    # Only the x264 and a52dec elements out of the ugly set, not the DVD and
    # RealMedia demuxers that come with them. a52dec decodes AC-3 without
    # libav. dpkg still believes the package is whole, which is fine in an
    # image that is never upgraded in place.
    && if [ "$WITH_X264" = "1" ]; then \
         apt-get install -y --no-install-recommends gstreamer1.0-plugins-ugly && \
         dpkg -L gstreamer1.0-plugins-ugly | grep '\.so$' | grep -v 'libgstx264\.so\|libgsta52dec\.so' | xargs -r rm -f ; \
       fi \
    && if [ "$WITH_LIBAV" = "1" ]; then \
         apt-get install -y --no-install-recommends gstreamer1.0-libav ; \
       fi \
    && rm -rf /var/lib/apt/lists/*

# A user with no login and no home of its own to write to. The mixer needs to
# write two places and nothing else: the config directory, which holds the
# runtime lists and, under home/, the secret store with channel keys and
# certificates, and the media directory.
RUN useradd --system --uid 10001 --user-group --no-create-home --shell /usr/sbin/nologin godwinmix \
    && mkdir -p /etc/godwinmix /var/lib/godwinmix/media /var/lib/godwinmix/run \
    && chown -R godwinmix:godwinmix /etc/godwinmix /var/lib/godwinmix

COPY --from=browser-builder /browser-runtime/ /opt/godwinmix/browser/
RUN if [ -f /opt/godwinmix/browser/godwinmix-browser ]; then \
      ln -s /opt/godwinmix/browser/godwinmix-browser /usr/local/bin/godwinmix-browser ; \
    fi

COPY --from=builder /src/target/release/godwinmix /usr/local/bin/godwinmix
COPY --from=builder /src/target/release/gmx /usr/local/bin/gmx
COPY --from=builder /cutout/models/ /usr/local/share/godwinmix/models/
COPY --from=builder /cutout/runtime/ /usr/local/lib/godwinmix/
COPY godwinmix.example.toml /usr/share/godwinmix/godwinmix.example.toml
COPY deploy/docker/default.toml /usr/share/godwinmix/default.toml
COPY deploy/docker/entrypoint.sh /usr/local/bin/gmx-entrypoint

RUN chmod +x /usr/local/bin/gmx-entrypoint

# WPE's headless backend wants a runtime directory for its nested Wayland
# socket, and Mesa renders in software because there is no GPU in here.
ENV DISPLAY=:99 \
    XDG_RUNTIME_DIR=/tmp/xdg \
    LIBGL_ALWAYS_SOFTWARE=1 \
    XDG_CACHE_HOME=/var/lib/godwinmix/cache \
    GODWINMIX_HOME=/etc/godwinmix/home \
    GODWINMIX_CONFIG=/etc/godwinmix/godwinmix.toml

# The control server and the UI. One port, as the mixer has always been.
EXPOSE 8080

USER godwinmix
WORKDIR /var/lib/godwinmix

# No -f: an unauthenticated /api/status answers 200, and with a token set it
# answers 401, which is still proof that the process is up and listening.
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD curl -s -o /dev/null http://127.0.0.1:8080/api/status || exit 1

ENTRYPOINT ["/usr/local/bin/gmx-entrypoint"]
