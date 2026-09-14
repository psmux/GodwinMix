#!/bin/bash
# Chromium needs an X display to start against; nothing is drawn on it.
mkdir -p /tmp/xdg && chmod 700 /tmp/xdg
Xvfb :99 -screen 0 1920x1080x24 -nolisten tcp > /tmp/xvfb.log 2>&1 &
for _ in $(seq 1 100); do [ -S /tmp/.X11-unix/X99 ] && break; sleep 0.05; done
# The wpesrc mixer, if it has a config. Its PATH has no sidecar on it.
if [ -f /etc/godwinmix/wpe.toml ]; then
  mkdir -p /var/lib/godwinmix/media-wpe
  # WebKit's web process sandbox is bubblewrap, which cannot set up its
  # namespaces inside an unprivileged container; without the switch WPE dies
  # in its process launcher (readPIDFromPeer). The container is the sandbox.
  # EGL, not GLX: the mixer's download elements create the GL context before
  # wpesrc does, and under X11 that defaults to GLX, which WPE cannot share
  # ("Available GStreamer GL Context is not EGL"). Pinned, wpesrc and the
  # pipeline share one EGL display on the Xvfb screen through Mesa.
  PATH=/usr/bin:/bin WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 GST_GL_PLATFORM=egl GST_GL_API=gles2 \
    /opt/gmx/wpe/godwinmix --config /etc/godwinmix/wpe.toml > /var/log/godwinmix-wpe.log 2>&1 &
fi
exec /opt/gmx/bin/godwinmix --config /etc/godwinmix/godwinmix.toml
