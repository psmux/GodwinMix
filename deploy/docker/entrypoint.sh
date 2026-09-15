#!/bin/sh
# Entrypoint for the GodwinMix image.
#
# Three jobs: put a config in place if the operator mounted none, start the
# X server a web page source needs, and hand the token to the mixer. Then get
# out of the way. `exec` matters: the mixer must be PID 1 so a `docker stop`
# reaches it as SIGTERM and the outputs close cleanly.
set -eu

CONFIG="${GODWINMIX_CONFIG:-/etc/godwinmix/godwinmix.toml}"

# Anything that is not a flag is passed through, so the image doubles as the
# CLI: `docker run --rm godwinmix ctl --url http://mixer:8080 status`, or
# `docker run --rm godwinmix mcp ...` for an agent.
case "${1:-}" in
  ctl|mcp|probe|--probe|--example-config|--version|--help|-h)
    exec godwinmix "$@"
    ;;
esac

if [ ! -f "$CONFIG" ]; then
  # A first run with nothing mounted still starts a working mixer, which is
  # what makes the five minute quickstart five minutes. Sources and outputs
  # are added over the API afterwards and saved beside this file.
  if [ -w "$(dirname "$CONFIG")" ]; then
    cp /usr/share/godwinmix/default.toml "$CONFIG"
    echo "godwinmix: no config at $CONFIG, wrote the shipped default there" >&2
  else
    echo "godwinmix: no config at $CONFIG and its directory is not writable." >&2
    echo "  Mount one:  -v \$PWD/config:/etc/godwinmix" >&2
    echo "  A starting point is /usr/share/godwinmix/godwinmix.example.toml in this image." >&2
    exit 78
  fi
fi

# GODWINMIX_TOKEN is read by the mixer itself and overrides [control] token in
# the file, so it is simply passed through. The warning is here because a
# control port with no token on a published port is the one deployment mistake
# that costs someone their stream.
if [ -z "${GODWINMIX_TOKEN:-}" ] && ! grep -qE '^[[:space:]]*token[[:space:]]*=' "$CONFIG"; then
  echo "godwinmix: no token set. Anyone who can reach port 8080 can take sources" >&2
  echo "  and add outputs. Set GODWINMIX_TOKEN, or [control] token in the config," >&2
  echo "  before this port is reachable from anywhere but your own machine." >&2
fi

# wpesrc renders a web page against an X display; nothing is drawn on it and
# nothing reads it. Only started when the image was built with WITH_WPE=1.
if command -v Xvfb > /dev/null 2>&1 && [ ! -S "/tmp/.X11-unix/X${DISPLAY#:}" ]; then
  mkdir -p "${XDG_RUNTIME_DIR:-/tmp/xdg}"
  chmod 700 "${XDG_RUNTIME_DIR:-/tmp/xdg}"
  Xvfb "$DISPLAY" -screen 0 1920x1080x24 -nolisten tcp > /tmp/xvfb.log 2>&1 &
  i=0
  while [ ! -S "/tmp/.X11-unix/X${DISPLAY#:}" ] && [ "$i" -lt 100 ]; do
    i=$((i + 1))
    sleep 0.05
  done
  # WebKit's web process sandbox is bubblewrap, which cannot set up its
  # namespaces inside an unprivileged container. The container is the sandbox.
  # EGL rather than GLX: the mixer's GL elements create the context first, and
  # under X11 that defaults to GLX, which WPE cannot share.
  export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
  export GST_GL_PLATFORM=egl
  export GST_GL_API=gles2
fi

exec godwinmix --config "$CONFIG" "$@"
