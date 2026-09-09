# LiveboxMix as shipped: binaries only, no source. Debian trixie because it is
# the current release that packages gstreamer1.0-wpe (the wpesrc method) and
# its glibc runs the binaries CI builds on Ubuntu 24.04. GStreamer for the mixer,
# Xvfb for Chromium to start against, Mesa for a GL context without a GPU, and
# Chromium's shared library needs for the CEF sidecar.
FROM debian:trixie-slim
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav \
    gstreamer1.0-gl gstreamer1.0-x gstreamer1.0-wpe gstreamer1.0-alsa \
    xvfb xauth xfonts-base ca-certificates curl \
    fonts-liberation fonts-dejavu-core fonts-noto-core fonts-noto-color-emoji \
    libgl1-mesa-dri libegl1 libgles2 libgl1 \
    libnss3 libnspr4 libatk1.0-0t64 libatk-bridge2.0-0t64 libatspi2.0-0t64 libcups2t64 \
    libdrm2 libxkbcommon0 libxcomposite1 libxdamage1 libxfixes3 libxrandr2 libgbm1 \
    libasound2t64 libpango-1.0-0 libcairo2 libgtk-3-0t64 libxss1 libx11-xcb1 libxcb-dri3-0 libxshmfence1 \
    && rm -rf /var/lib/apt/lists/*
COPY bin/ /opt/lbx/bin/
COPY entrypoint.sh /opt/lbx/entrypoint.sh
RUN chmod +x /opt/lbx/bin/liveboxmix /opt/lbx/bin/liveboxmix-browser /opt/lbx/entrypoint.sh \
    && chown root:root /opt/lbx/bin/chrome-sandbox && chmod 4755 /opt/lbx/bin/chrome-sandbox \
    # A second copy of the mixer with no sidecar beside it: the mixer finds a
    # sidecar next to its own binary, and without one a web page renders
    # through GStreamer's wpesrc. That copy is the wpesrc mixer, on :8081.
    && mkdir -p /opt/lbx/wpe && cp /opt/lbx/bin/liveboxmix /opt/lbx/wpe/liveboxmix
# The X display the entrypoint starts. WPE's headless backend wants a runtime
# directory for its nested Wayland socket, and Mesa is told to render in
# software: there is no GPU in this container.
ENV DISPLAY=:99 XDG_RUNTIME_DIR=/tmp/xdg LIBGL_ALWAYS_SOFTWARE=1
EXPOSE 8080 8081
ENTRYPOINT ["/opt/lbx/entrypoint.sh"]
