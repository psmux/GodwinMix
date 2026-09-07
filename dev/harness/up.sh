#!/bin/bash
# Bring the rig up: RTMP server, cam1, then the mixer. Logs land in logs/.
#   GST_DEBUG="..." dev/harness/up.sh      (extra GStreamer logging goes to logs/gst.log)
export PATH="/home/dev/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin"
H="$(cd "$(dirname "$0")" && pwd)"; LB="$H/../../target/release/liveboxmix"
"$H/down.sh" >/dev/null 2>&1
nohup mediamtx "$H/mediamtx.yml" > "$H/logs/mediamtx.log" 2>&1 &
sleep 1
nohup "$H/cams.sh" > "$H/logs/cams.log" 2>&1 &
# Test pages for web+ sources (browser/test) on port 8090. Bound to every
# interface, not loopback: the sidecar runs in a container and reaches this
# machine as host.docker.internal, which a loopback-only server does not answer,
# and the page then loads nothing and renders black.
nohup python3 -m http.server 8090 --bind 0.0.0.0 --directory "$H/../../browser/test" > "$H/logs/http.log" 2>&1 < /dev/null &
sleep 2
cd "$H"
if [ -n "$GST_DEBUG" ]; then export GST_DEBUG_NO_COLOR=1 GST_DEBUG_FILE="$H/logs/gst.log"; fi
nohup "$LB" --config "$H/test.toml" > "$H/logs/mixer.log" 2>&1 &
for i in $(seq 1 20); do sleep 1; curl -sf http://127.0.0.1:8080/api/status >/dev/null && break; done
"$LB" ctl --url http://127.0.0.1:8080 status
