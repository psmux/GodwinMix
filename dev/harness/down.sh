#!/bin/bash
pkill -f "target/release/godwinmix --config" ; pkill -f "harness/cams.sh"; pkill -x mediamtx
pkill -f "lavfi -i smptebars"; pkill -f "http.server 8090"; pkill -f "godwinmix-browser"
# Sidecar containers outlive a mixer that was killed rather than asked to
# stop, and each one costs a core. Remove every one of them; the rig is down.
if [ -n "$DOCKER_HOST" ] || [ -S "$HOME/.colima/default/docker.sock" ]; then
  export DOCKER_HOST="${DOCKER_HOST:-unix://$HOME/.colima/default/docker.sock}"
  docker ps -q --filter name=gmx-browser- 2>/dev/null | xargs -r docker rm -f >/dev/null 2>&1
fi
true
