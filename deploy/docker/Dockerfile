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

# Manifests first, then a throwaway main per crate, so the dependency build is
# cached and only the mixer itself is recompiled when the source changes.
# Every target a manifest names has to exist for `cargo build` to accept it,
# which is four library roots and the two binaries.
COPY Cargo.toml Cargo.lock ./
COPY crates/godwinmix-protocol/Cargo.toml crates/godwinmix-protocol/
COPY crates/godwinmix-core/Cargo.toml crates/godwinmix-core/
COPY crates/godwinmix-host/Cargo.toml crates/godwinmix-host/
COPY crates/godwinmix/Cargo.toml crates/godwinmix/
RUN mkdir -p crates/godwinmix-protocol/src crates/godwinmix-core/src \
        crates/godwinmix-host/src crates/godwinmix/src/bin \
    && touch crates/godwinmix-protocol/src/lib.rs \
        crates/godwinmix-core/src/lib.rs \
        crates/godwinmix-host/src/lib.rs \
        crates/godwinmix/src/lib.rs \
    && echo 'fn main() {}' > crates/godwinmix/src/main.rs \
    && echo 'fn main() {}' > crates/godwinmix/src/bin/gmx.rs \
    && cargo build --release --locked \
    && find crates -name '*.rs' -delete

# The real source, and the data the crates embed with `include_str!`:
# `codecs.toml` and the example config for the engine, `layouts/` for the
# scene module, `ui/` for the web UI the binary serves. They stay at the
# repository root, which is where every document says they live.
COPY crates ./crates
COPY codecs.toml godwinmix.example.toml ./
COPY layouts ./layouts
COPY ui ./ui
RUN find crates -name '*.rs' -exec touch {} + \
    && cargo build --release --locked --bins \
    && strip target/release/godwinmix target/release/gmx

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
# It pulls in WebKit and costs about 120 MB. --build-arg WITH_WPE=0 drops it
# and the image loses web page sources.
ARG WITH_WPE=1

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

COPY --from=builder /src/target/release/godwinmix /usr/local/bin/godwinmix
COPY --from=builder /src/target/release/gmx /usr/local/bin/gmx
COPY godwinmix.example.toml /usr/share/godwinmix/godwinmix.example.toml
COPY deploy/docker/entrypoint.sh /usr/local/bin/gmx-entrypoint

RUN chmod +x /usr/local/bin/gmx-entrypoint

# WPE's headless backend wants a runtime directory for its nested Wayland
# socket, and Mesa renders in software because there is no GPU in here.
ENV DISPLAY=:99 \
    XDG_RUNTIME_DIR=/tmp/xdg \
    LIBGL_ALWAYS_SOFTWARE=1 \
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
