#!/bin/bash
# Point browser.sidecar at this file to run the containerised sidecar (the one
# with H.264 and AAC) from a mixer on any host with Docker. The container's
# stdout is the stream. The mixer stops a source by signalling its process
# group; that reaches this script, not the container (its stdout is a pipe to
# the Docker daemon, so it would not notice the reader going away), so the
# signal is forwarded by name and the container is gone before this exits.
NAME="lbx-browser-$$"
# TERM first, so the browser can close; then, if the container is still there
# a few seconds on, remove it by force. A Chromium that does not act on TERM
# left containers running for hours after their mixer was gone, each one
# rendering a page for nobody at a full core, until the machine could not
# keep up with the sources it actually had.
stop() {
  docker kill -s TERM "$NAME" >/dev/null 2>&1
  for _ in 1 2 3 4 5 6; do docker inspect "$NAME" >/dev/null 2>&1 || break; sleep 0.5; done
  docker rm -f "$NAME" >/dev/null 2>&1
  wait "$CHILD" 2>/dev/null
  exit 0
}
trap stop TERM INT HUP
# LBX_SIDECAR_LOG=<file> keeps a copy of the sidecar's stderr, which the mixer
# only logs at debug level. A copy, through tee, and not a redirect: the mixer
# reads this stderr when it probes a page for superimpose, and a plain
# `exec 2>>file` closed that pipe on it, so every probe through this script
# came back empty and the source quietly rendered the page whole.
if [ -n "$LBX_SIDECAR_LOG" ]; then exec 2> >(tee -a "$LBX_SIDECAR_LOG" >&2); fi
# --log-driver none: the stream is stdout, and the default json-file driver copies
# everything a container writes to disk. 41 MB/s of raw video filled a 30 GB
# disk in minutes, and the players stalled on the full disk.
docker run -i --rm --init --log-driver none --name "$NAME" ${LBX_DOCKER_ARGS:---network host} lbx-browser "$@" &
CHILD=$!
wait "$CHILD"
