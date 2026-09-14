#!/usr/bin/env bash
# Stop everything up.sh started. Safe to run when nothing is up.
H="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

pkill -f "target/release/godwinmix --config" 2>/dev/null
pkill -f "target/debug/godwinmix --config" 2>/dev/null
pkill -f "harness/cams.sh" 2>/dev/null
pkill -f "lavfi -i smptebars" 2>/dev/null
pkill -f "http.server 8090" 2>/dev/null
pkill -f "godwinmix-browser" 2>/dev/null
# The downloaded copy and one that was already on PATH.
pkill -f "$H/bin/mediamtx" 2>/dev/null
pkill -x mediamtx 2>/dev/null

# Sidecar containers outlive a mixer that was killed rather than asked to stop,
# and each one costs a core. Remove every one of them; the rig is down.
if [ -n "$DOCKER_HOST" ] || [ -S "$HOME/.colima/default/docker.sock" ]; then
  export DOCKER_HOST="${DOCKER_HOST:-unix://$HOME/.colima/default/docker.sock}"
  docker ps -q --filter name=gmx-browser- 2>/dev/null | xargs -r docker rm -f > /dev/null 2>&1
fi
true
