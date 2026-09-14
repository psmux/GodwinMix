#!/bin/bash
# Chromium needs an X display to start against; nothing is drawn on it. Wait
# for the server to be listening: starting the sidecar a few milliseconds too
# early gave "Missing X server or $DISPLAY" once in a while.
Xvfb :99 -screen 0 1920x1080x24 -nolisten tcp >/dev/null 2>&1 &
for _ in $(seq 1 100); do [ -S /tmp/.X11-unix/X99 ] && break; sleep 0.05; done
exec /opt/gmx/godwinmix-browser "$@"
