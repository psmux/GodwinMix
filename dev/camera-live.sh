#!/usr/bin/env bash
#
# Camera control against a real core: the ptz and atem plugins installed the
# way a person installs them, started by the core, and driven through the
# core's tool.call, the same path the camera window and an agent take.
#
# What it runs:
#   * a core in a folder of its own, with two sources: cam-a a test pattern,
#     cam-b a photograph of two people
#   * the fake ATEM from crates/godwinmix-atem (examples/fake_atem.rs)
#   * the ptz plugin with two fake cameras on those sources, and the atem
#     plugin pointed at the fake switcher with both sources mapped
#
# Then it checks: cameras listed, the joystick moves and stops by itself,
# presets with a thumbnail, YOLOX finds the two people in cam-b and follows
# one, the switcher connects with its inputs, programme and preview, DSK and
# fade to black, follow mode (a GodwinMix take cuts the switcher) and lead
# mode (a switcher cut moves GodwinMix).
#
# Usage: dev/camera-live.sh [--keep]
#
# Needs: cargo, python3 (standard library only), curl, and the models for
# the YOLOX step (fetched on first use into the work folder).

set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KEEP=0
[[ "${1:-}" == "--keep" ]] && KEEP=1
WORK="$(mktemp -d "${TMPDIR:-/tmp}/gmx-camera.XXXXXX")"
TOKEN="cam-$RANDOM$RANDOM"
PIDS=()
FAILED=0

step() { printf '%-62s' "$1"; }
ok() { printf 'ok%s\n' "${1:+  ($1)}"; }
bad() { printf 'FAIL\n'; printf '    %s\n' "$1" >&2; FAILED=$((FAILED + 1)); KEEP=1; }

cleanup() {
    for pid in ${PIDS[@]+"${PIDS[@]}"}; do kill "$pid" 2>/dev/null; done
    for pid in ${PIDS[@]+"${PIDS[@]}"}; do
        for _ in 1 2 3 4 5 6 7 8 9 10; do kill -0 "$pid" 2>/dev/null || break; sleep 0.3; done
        kill -9 "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
    done
    if [[ $KEEP -eq 1 ]]; then echo "kept: $WORK"; else rm -rf "$WORK"; fi
}
trap cleanup EXIT

free_tcp() { python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }
free_udp() { python3 -c 'import socket;s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }

PORT="$(free_tcp)"
ATEM_PORT="$(free_udp)"
BASE="http://127.0.0.1:$PORT"
export GODWINMIX_HOME="$WORK/home" GODWINMIX_PLUGINS_DIR="$WORK/plugins"
mkdir -p "$GODWINMIX_HOME" "$GODWINMIX_PLUGINS_DIR"

# One tool call; prints the structured answer as JSON, or {"error": ...}.
tool() {
    local args="${2:-}"; [[ -z "$args" ]] && args='{}'
    curl -sS -X POST "$BASE/api/v1/tool/call" -H "Authorization: Bearer $TOKEN" \
        -H 'Content-Type: application/json' -d "{\"name\":\"$1\",\"arguments\":$args}" |
        python3 -c 'import json,sys
r=json.load(sys.stdin)
r=r.get("result",r)
print(json.dumps(r.get("structured_content", r)))'
}
# Read one value out of JSON on stdin with a Python expression over `d`.
jq_() { python3 -c "import json,sys; d=json.load(sys.stdin); print($1)"; }
api() { curl -fsS "$BASE/api/v1/$1" -H "Authorization: Bearer $TOKEN"; }
take() {
    curl -sS -X POST "$BASE/api/v1/program/take" -H "Authorization: Bearer $TOKEN" \
        -H 'Content-Type: application/json' -d "{\"source\":\"$1\"}" >/dev/null
}
program() { api agent/state | jq_ 'd["program"]'; }
# Wait up to $1 tenths of a second for the command in $2 to print $3.
until_is() {
    local n=$1 cmd=$2 want=$3 got=""
    for _ in $(seq 1 "$n"); do got="$(eval "$cmd" 2>/dev/null)"; [[ "$got" == "$want" ]] && return 0; sleep 0.1; done
    echo "$got"; return 1
}

echo "GodwinMix camera control live test on $BASE"
echo

step "build the core, both plugins and the fake ATEM"
{
    (cd "$REPO" && cargo build --quiet -p godwinmix && cargo build --quiet -p godwinmix-atem --example fake_atem) &&
        (cd "$REPO/plugins/ptz" && ./build) && (cd "$REPO/plugins/atem" && ./build)
} >"$WORK/build.log" 2>&1 && ok || { bad "see $WORK/build.log"; exit 1; }

GMX="$REPO/target/debug/godwinmix"

step "the fake ATEM on udp $ATEM_PORT"
"$REPO/target/debug/examples/fake_atem" "$ATEM_PORT" >"$WORK/atem.log" 2>&1 &
PIDS+=($!)
sleep 0.5
kill -0 "${PIDS[${#PIDS[@]}-1]}" 2>/dev/null && ok || { bad "$(cat "$WORK/atem.log")"; exit 1; }

step "config"
"$GMX" --example-config >"$WORK/example.toml" 2>/dev/null
python3 - "$WORK" "$PORT" "$TOKEN" "$ATEM_PORT" <<'PY'
import re, sys
work, port, token, atem = sys.argv[1:5]
out, skipping = [], False
for line in open(f"{work}/example.toml").read().splitlines():
    s = line.strip()
    if s.startswith("[["):
        skipping = s in ("[[sources]]", "[[outputs]]")
    elif s.startswith("["):
        skipping = False
    out.append("# " + line if skipping and not line.startswith("#") else line)
text = "\n".join(out) + "\n"
text = re.sub(r'(?m)^bind = .*$', f'bind = "127.0.0.1:{port}"', text)
text = re.sub(r'(?m)^# token = .*$', f'token = "{token}"', text)
text = re.sub(r'(?m)^min_hold_ms = .*$', 'min_hold_ms = 300', text)
text += f'''
[plugins.ptz]
detector = "yolox"
models_dir = "{work}/models"
data_dir = "{work}/ptz"
cameras = [
  {{ id = "cam-a", protocol = "fake", source = "cam-a" }},
  {{ id = "cam-b", protocol = "fake", source = "cam-b" }},
]

[plugins.atem]
switchers = [
  {{ id = "atem-main", address = "127.0.0.1:{atem}", name = "Main switcher", mode = "own", map = {{ "cam-a" = 1, "cam-b" = 2 }} }},
]
'''
open(f"{work}/godwinmix.toml", "w").write(text)
PY
grep -q "^token = " "$WORK/godwinmix.toml" && ok || { bad "the config was not rewritten"; exit 1; }

step "core starts"
(cd "$WORK" && exec "$GMX" --config "$WORK/godwinmix.toml") >"$WORK/core.log" 2>&1 &
PIDS+=($!)
for _ in $(seq 1 150); do api core/info >/dev/null 2>&1 && break; sleep 0.2; done
api core/info >/dev/null 2>&1 && ok || { bad "no answer; see $WORK/core.log"; exit 1; }

step "install the plugins, live"
export GODWINMIX_URL="$BASE" GODWINMIX_TOKEN="$TOKEN"
{ "$GMX" plugin add "$REPO/plugins/ptz" && "$GMX" plugin add "$REPO/plugins/atem"; } >"$WORK/install.log" 2>&1 &&
    ok || { bad "$(tail -5 "$WORK/install.log")"; exit 1; }

step "two sources live"
PEOPLE="$REPO/crates/godwinmix-track/tests/images/two-people.jpg"
for spec in "cam-a test://smpte" "cam-b $PEOPLE"; do
    set -- $spec
    curl -fsS -X POST "$BASE/api/v1/sources" -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
        -d "{\"id\":\"$1\",\"uri\":\"$2\",\"name\":\"$1\"}" >/dev/null 2>&1
done
live() { api sources | jq_ 'sum(1 for s in d if s["state"]=="live")'; }
got="$(until_is 200 live 2)" && ok || bad "live sources: $got"

source "$REPO/dev/camera-live-checks.sh"

# HOLD=<file>: leave everything running while that file exists, so a browser
# can drive the camera window against it.
if [[ -n "${HOLD:-}" ]]; then
    printf '%s %s\n' "$BASE" "$TOKEN" >"$HOLD"
    echo "holding on $BASE until $HOLD is removed"
    while [[ -e "$HOLD" ]]; do sleep 1; done
fi

echo
if [[ $FAILED -eq 0 ]]; then echo "all camera checks passed"; else echo "$FAILED check(s) failed"; fi
exit $FAILED
