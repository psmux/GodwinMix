#!/bin/bash
# Point browser.sidecar at this file to run the containerised sidecar (the one
# with H.264 and AAC) from a mixer on any host with Docker. The container's
# stdout is the stream. The mixer stops a source by signalling its process
# group; that reaches this script, not the container (its stdout is a pipe to
# the Docker daemon, so it would not notice the reader going away), so the
# signal is forwarded by name and the container is gone before this exits.
NAME="lbx-browser-$$"
stop() { docker kill -s TERM "$NAME" >/dev/null 2>&1; wait "$CHILD" 2>/dev/null; exit 0; }
trap stop TERM INT HUP
# LBX_SIDECAR_LOG=<file> keeps the sidecar's stderr, which the mixer only logs at debug level.
if [ -n "$LBX_SIDECAR_LOG" ]; then exec 2>>"$LBX_SIDECAR_LOG"; fi
# --log-driver none: the stream is stdout, and the default json-file driver copies
# everything a container writes to disk. 41 MB/s of raw video filled a 30 GB
# disk in minutes, and the players stalled on the full disk.
docker run -i --rm --init --log-driver none --name "$NAME" ${LBX_DOCKER_ARGS:---network host} lbx-browser "$@" &
CHILD=$!
wait "$CHILD"
