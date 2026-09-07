#!/bin/bash
# Open the desktop app, starting the mixer first if it is not already running.
#
#   dev/desktop.sh              mixer on the test rig in dev/harness (mediamtx,
#                               a synthetic camera, the mixer on 127.0.0.1:8080)
#   dev/desktop.sh mine.toml    mixer on your own config instead of the rig
#
# The app is a window pointed at http://localhost:8080. It has nothing to show
# until a mixer answers there, which is why this script waits for one.
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
API="http://127.0.0.1:8080/api/status"

if ! curl -sf "$API" >/dev/null 2>&1; then
  if [ -n "$1" ]; then
    echo "starting the mixer on $1"
    mkdir -p "$ROOT/dev/harness/logs"
    nohup "$ROOT/target/release/liveboxmix" --config "$1" > "$ROOT/dev/harness/logs/mixer.log" 2>&1 &
  else
    echo "starting the test rig"
    "$ROOT/dev/harness/up.sh"
  fi
  for _ in $(seq 1 30); do curl -sf "$API" >/dev/null 2>&1 && break; sleep 1; done
  curl -sf "$API" >/dev/null 2>&1 || { echo "the mixer did not come up; see dev/harness/logs/mixer.log"; exit 1; }
fi

# The bundle, if it has been built, else the bare binary from cargo.
APP="$ROOT/tauri-app/target/release/bundle/macos/LiveboxMix.app"
BIN="$ROOT/tauri-app/target/release/liveboxmix-desktop"
if [ -d "$APP" ]; then
  open "$APP"
elif [ -x "$BIN" ]; then
  nohup "$BIN" > "$ROOT/dev/harness/logs/desktop.log" 2>&1 &
else
  echo "no desktop build yet: cd tauri-app && cargo tauri build --bundles app"; exit 1
fi
echo "desktop app opened against http://localhost:8080"
