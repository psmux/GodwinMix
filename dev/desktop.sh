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
    nohup "$ROOT/target/release/godwinmix" --config "$1" > "$ROOT/dev/harness/logs/mixer.log" 2>&1 &
  else
    echo "starting the test rig"
    "$ROOT/dev/harness/up.sh"
  fi
  for _ in $(seq 1 30); do curl -sf "$API" >/dev/null 2>&1 && break; sleep 1; done
  curl -sf "$API" >/dev/null 2>&1 || { echo "the mixer did not come up; see dev/harness/logs/mixer.log"; exit 1; }
fi

# The camera, the screen and the microphone, staged where the bundler picks
# them up. Cheap when nothing has changed, because cargo does the deciding, and
# it means the next `cargo tauri build` carries them whether or not anybody
# remembered the step.
"$ROOT/dev/bundle-plugins.sh" >/dev/null || echo "the device plugins did not stage; run dev/bundle-plugins.sh to see why"

# The bundle, if it has been built, else the bare binary from cargo. Run in
# the foreground so its exit status is known: "Quit and stop the mixer" in
# the app menu exits with 2 after stopping the mixer, and that is when the
# rest of the rig (mediamtx, the camera, the page server) is stopped too.
# Plain Quit, or closing the window, leaves everything running.
APP="$ROOT/tauri-app/target/release/bundle/macos/GodwinMix.app/Contents/MacOS/godwinmix-desktop"
BIN="$ROOT/tauri-app/target/release/godwinmix-desktop"
if [ -x "$APP" ]; then RUN="$APP"; elif [ -x "$BIN" ]; then RUN="$BIN"; else
  echo "no desktop build yet: cd tauri-app && cargo tauri build --bundles app"; exit 1
fi
# A bundle built before the plugins travelled with it has no camera in it, and
# from the window that looks like a bug rather than a build that is behind.
RESOURCES="$ROOT/tauri-app/target/release/bundle/macos/GodwinMix.app/Contents/Resources/plugins"
if [ "$RUN" = "$APP" ] && [ -z "$(find "$RESOURCES" -name 'gmx-camera' 2>/dev/null)" ]; then
  echo "this bundle carries no camera plugin; rebuild it: cd tauri-app && cargo tauri build --bundles app"
fi
echo "desktop app open against http://localhost:8080 (this waits until it quits)"
"$RUN" 2>> "$ROOT/dev/harness/logs/desktop.log"
STATUS=$?
if [ "$STATUS" = 2 ]; then
  echo "stopping the rig"
  "$ROOT/dev/harness/down.sh"
fi
exit 0
