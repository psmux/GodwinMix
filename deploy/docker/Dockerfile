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

# The browser runs outside the mixer. Debian's WPE fallback can crash inside
# the mixer process, so the default image ships the existing CEF sidecar.
FROM builder AS browser-builder
ARG WITH_CEF=1
ENV CEF_PATH=/cef
RUN mkdir -p /browser-runtime \
    && if [ "$WITH_CEF" = "1" ]; then \
         apt-get update && apt-get install -y --no-install-recommends \
           cmake ninja-build libx11-dev libgtk-3-dev && \
         cargo build --manifest-path browser/Cargo.toml --release --locked --bin godwinmix-browser && \
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

# x264enc is the only software H.264 encoder the mixer's probe knows
# (crates/godwinmix-core/src/probe.rs), and on Debian it ships inside gstreamer1.0-plugins-ugly.
# Build with --build-arg WITH_X264=0 if that package's licensing is a problem
# where the image runs; the mixer then needs a hardware encoder (VA, NVENC)
# and refuses to start without one.
ARG WITH_X264=1
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
# bad:  srtsrc and srtsink, the DASH and HLS demuxers, videoparsers, voaacenc,
#       and the GL elements the compositor uses when there is a context.
# libav: avdec_h264 and avdec_aac for decode, avenc_aac for the programme's
#       audio when fdkaacenc is absent (it is: libfdk-aac is not in Debian).
# tools: gst-inspect-1.0 and gst-launch-1.0, for `docker exec` when a pipeline
#       misbehaves. Half a megabyte and worth it on a headless box.
RUN apt-get update && apt-get install -y --no-install-recommends \
      gstreamer1.0-plugins-base \
      gstreamer1.0-plugins-good \
      gstreamer1.0-plugins-bad \
      gstreamer1.0-libav \
      gstreamer1.0-tools \
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
    # Only the x264 element out of the ugly set, not the DVD and RealMedia
    # demuxers that come with it. dpkg still believes the package is whole,
    # which is fine in an image that is never upgraded in place.
    && if [ "$WITH_X264" = "1" ]; then \
         apt-get install -y --no-install-recommends gstreamer1.0-plugins-ugly && \
         dpkg -L gstreamer1.0-plugins-ugly | grep '\.so$' | grep -v 'libgstx264\.so' | xargs -r rm -f ; \
       fi \
    && rm -rf /var/lib/apt/lists/*

# A user with no login and no home of its own to write to. The mixer needs to
# write two places and nothing else: the runtime source list beside its config,
# and the media directory.
RUN useradd --system --uid 10001 --user-group --no-create-home --shell /usr/sbin/nologin godwinmix \
    && mkdir -p /etc/godwinmix /var/lib/godwinmix/media /var/lib/godwinmix/run \
    && chown -R godwinmix:godwinmix /etc/godwinmix /var/lib/godwinmix

COPY --from=browser-builder /browser-runtime/ /opt/godwinmix/browser/
RUN if [ -f /opt/godwinmix/browser/godwinmix-browser ]; then \
      ln -s /opt/godwinmix/browser/godwinmix-browser /usr/local/bin/godwinmix-browser ; \
    fi

COPY --from=builder /src/target/release/godwinmix /usr/local/bin/godwinmix
COPY --from=builder /src/target/release/gmx /usr/local/bin/gmx
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
