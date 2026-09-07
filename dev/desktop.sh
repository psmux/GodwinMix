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

# The bundle, if it has been built, else the bare binary from cargo. Run in
# the foreground so its exit status is known: "Quit and stop the mixer" in
# the app menu exits with 2 after stopping the mixer, and that is when the
# rest of the rig (mediamtx, the camera, the page server) is stopped too.
# Plain Quit, or closing the window, leaves everything running.
APP="$ROOT/tauri-app/target/release/bundle/macos/LiveboxMix.app/Contents/MacOS/liveboxmix-desktop"
BIN="$ROOT/tauri-app/target/release/liveboxmix-desktop"
if [ -x "$APP" ]; then RUN="$APP"; elif [ -x "$BIN" ]; then RUN="$BIN"; else
  echo "no desktop build yet: cd tauri-app && cargo tauri build --bundles app"; exit 1
fi
echo "desktop app open against http://localhost:8080 (this waits until it quits)"
"$RUN" 2>> "$ROOT/dev/harness/logs/desktop.log"
STATUS=$?
if [ "$STATUS" = 2 ]; then
  echo "stopping the rig"
  "$ROOT/dev/harness/down.sh"
fi
exit 0
