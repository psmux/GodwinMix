#!/usr/bin/env bash
# Bring the local rig up: an RTMP server, a synthetic camera, a page server and
# the mixer on a test config. Logs land in dev/harness/logs/.
#
#   dev/harness/up.sh                 release build, downloads mediamtx if needed
#   dev/harness/up.sh --debug         use the debug build instead
#   GST_DEBUG=3 dev/harness/up.sh     extra GStreamer logging into logs/gst.log
#
# `gmx harness up` will do this without a shell script. This is the version
# that exists.
set -euo pipefail

H="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$H/../.." && pwd)"
BIN="$H/bin"
LOGS="$H/logs"
PROFILE=release
[ "${1:-}" = "--debug" ] && PROFILE=debug

MIXER="$ROOT/target/$PROFILE/godwinmix"
mkdir -p "$BIN" "$LOGS" "$H/media"

# ---------------------------------------------------------------- mediamtx --
# Downloaded rather than vendored: it is 28 MB and it is not ours. bin/ is git
# ignored, so a clone stays small and the first run pays for it once.
fetch_mediamtx() {
  local os arch asset ext version url
  case "$(uname -s)" in
    Darwin)  os=darwin ;;
    Linux)   os=linux ;;
    MINGW*|MSYS*|CYGWIN*) os=windows ;;
    *) echo "up.sh: no mediamtx build known for $(uname -s). Put one in $BIN yourself." >&2; return 1 ;;
  esac
  case "$(uname -m)" in
    x86_64|amd64) arch=amd64 ;;
    # The project's arm64 assets are named arm64v8 on Linux and arm64 elsewhere.
    arm64|aarch64) [ "$os" = linux ] && arch=arm64v8 || arch=arm64 ;;
    *) echo "up.sh: no mediamtx build known for $(uname -m)." >&2; return 1 ;;
  esac
  ext=tar.gz
  [ "$os" = windows ] && ext=zip

  version="${MEDIAMTX_VERSION:-}"
  if [ -z "$version" ]; then
    version="$(curl -fsSL https://api.github.com/repos/bluenviron/mediamtx/releases/latest \
      | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)"
  fi
  [ -n "$version" ] || { echo "up.sh: could not work out the latest mediamtx version. Set MEDIAMTX_VERSION." >&2; return 1; }

  asset="mediamtx_${version}_${os}_${arch}.${ext}"
  url="https://github.com/bluenviron/mediamtx/releases/download/${version}/${asset}"
  echo "up.sh: fetching mediamtx $version for ${os}_${arch}"
  curl -fsSL "$url" -o "$BIN/$asset"
  if [ "$ext" = zip ]; then unzip -oq "$BIN/$asset" -d "$BIN"; else tar xzf "$BIN/$asset" -C "$BIN"; fi
  rm -f "$BIN/$asset"
  chmod +x "$BIN/mediamtx"*
}

MEDIAMTX="$BIN/mediamtx"
[ -x "$MEDIAMTX" ] || MEDIAMTX="$BIN/mediamtx.exe"
if [ ! -x "$MEDIAMTX" ]; then
  # An mediamtx already on PATH is used as it is: somebody who installed one
  # deliberately should not get a second copy.
  if command -v mediamtx > /dev/null 2>&1; then
    MEDIAMTX="$(command -v mediamtx)"
  else
    fetch_mediamtx
    MEDIAMTX="$BIN/mediamtx"
    [ -x "$MEDIAMTX" ] || MEDIAMTX="$BIN/mediamtx.exe"
  fi
fi

# ------------------------------------------------------------------- mixer --
if [ ! -x "$MIXER" ]; then
  echo "up.sh: no $MIXER. Build it first:"
  echo "    cargo build${PROFILE:+ --$PROFILE}" | sed 's/--debug//'
  exit 1
fi

"$H/down.sh" > /dev/null 2>&1 || true

# ------------------------------------------------------------------- start --
echo "up.sh: RTMP server on 1935"
nohup "$MEDIAMTX" "$H/mediamtx.yml" > "$LOGS/mediamtx.log" 2>&1 &

# A synthetic camera, if ffmpeg is about. Without one the rig still comes up
# and the mixer simply has a source that never goes live, which is itself a
# useful thing to test against.
if command -v ffmpeg > /dev/null 2>&1; then
  echo "up.sh: cam1, bars and a 440 Hz tone, into rtmp://127.0.0.1:1935/live/cam1"
  nohup "$H/cams.sh" > "$LOGS/cams.log" 2>&1 &
else
  echo "up.sh: no ffmpeg, so no synthetic camera. cam1 will sit in 'connecting'."
fi

# Test pages for web+ sources. Bound to every interface, not loopback: a
# sidecar running in a container reaches this machine as host.docker.internal,
# which a loopback-only server does not answer, and the page renders black.
if [ -d "$ROOT/browser/test" ]; then PAGES="$ROOT/browser/test"; else PAGES="$H/page"; fi
echo "up.sh: pages from $PAGES on 8090"
nohup python3 -m http.server 8090 --bind 0.0.0.0 --directory "$PAGES" \
  > "$LOGS/http.log" 2>&1 < /dev/null &

if [ -n "${GST_DEBUG:-}" ]; then
  export GST_DEBUG_NO_COLOR=1 GST_DEBUG_FILE="$LOGS/gst.log"
fi

sleep 2
echo "up.sh: mixer on 8080"
cd "$H"
nohup "$MIXER" --config "$H/test.toml" > "$LOGS/mixer.log" 2>&1 &

for _ in $(seq 1 30); do
  sleep 1
  curl -sf http://127.0.0.1:8080/api/status > /dev/null && break
done

if ! curl -sf http://127.0.0.1:8080/api/status > /dev/null; then
  echo "up.sh: the mixer did not answer on 8080. Last of $LOGS/mixer.log:" >&2
  tail -20 "$LOGS/mixer.log" >&2
  exit 1
fi

echo
"$MIXER" ctl --url http://127.0.0.1:8080 status
cat <<EOF

  UI            http://127.0.0.1:8080
  test pages    http://127.0.0.1:8090
  programme     rtmp://127.0.0.1:1935/live/program
  logs          $LOGS
  stop          $H/down.sh
EOF
