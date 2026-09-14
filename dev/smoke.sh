#!/usr/bin/env bash
#
# One run of everything a person does on a fresh mixer, against a real core.
#
# Starts a core on a free port with a token, adds a test source, puts it on
# air, looks at it through all three doors (/api/v1, /rpc, /metrics) and the
# two clients (gmx ctl, gmx mcp), then takes it down and checks nothing was
# left running. Every step prints ok or fails the script.
#
# Usage: dev/smoke.sh [--keep]
#   --keep   leave the working directory and the core's log behind
#
# Needs: cargo, python3 (standard library only), curl.

set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KEEP=0
[[ "${1:-}" == "--keep" ]] && KEEP=1

WORK="$(mktemp -d "${TMPDIR:-/tmp}/gmx-smoke.XXXXXX")"
LOG="$WORK/core.log"
TOKEN="smoke-$RANDOM$RANDOM"
CORE_PID=""
FAILED=0

# --- reporting --------------------------------------------------------------

step() { printf '%-58s' "$1"; }
ok() { printf 'ok\n'; }
bad() {
    printf 'FAIL\n'
    printf '    %s\n' "$1" >&2
    FAILED=$((FAILED + 1))
}

cleanup() {
    if [[ -n "$CORE_PID" ]] && kill -0 "$CORE_PID" 2>/dev/null; then
        kill "$CORE_PID" 2>/dev/null
        wait "$CORE_PID" 2>/dev/null
    fi
    if [[ $KEEP -eq 1 ]]; then
        echo "kept: $WORK"
    else
        rm -rf "$WORK"
    fi
}
trap cleanup EXIT

# --- the core ---------------------------------------------------------------

PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
BASE="http://127.0.0.1:$PORT"
AUTH=(-H "Authorization: Bearer $TOKEN")

echo "GodwinMix smoke test on $BASE"
echo

step "build"
if ! (cd "$REPO" && cargo build --quiet) >"$WORK/build.log" 2>&1; then
    bad "cargo build failed, see $WORK/build.log"
    KEEP=1
    exit 1
fi
ok
GMX="$REPO/target/debug/gmx"

# The example config, with every source and output commented out: the point is
# a core that starts clean and is driven entirely through the API.
step "config from --example-config"
"$REPO/target/debug/godwinmix" --example-config >"$WORK/example.toml" 2>/dev/null
python3 - "$WORK/example.toml" "$WORK/godwinmix.toml" "$PORT" "$TOKEN" <<'PY'
import re, sys
src, dst, port, token = sys.argv[1:5]
text = open(src).read()
# Comment out every [[sources]] and [[outputs]] block: a smoke test drives the
# mixer through the API and must not wait on a camera that is not there.
out, skipping = [], False
for line in text.splitlines():
    stripped = line.strip()
    if stripped.startswith("[["):
        skipping = stripped in ("[[sources]]", "[[outputs]]")
    elif stripped.startswith("[") and not stripped.startswith("[["):
        skipping = False
    out.append("# " + line if skipping and not line.startswith("#") else line)
text = "\n".join(out) + "\n"
text = re.sub(r'(?m)^bind = .*$', f'bind = "127.0.0.1:{port}"', text)
text = re.sub(r'(?m)^# token = .*$', f'token = "{token}"', text)
# Short lingers, so the teardown step waits seconds rather than half a
# minute. The mechanism is what is being tested, not the shipped numbers:
# the mosaic is held up by the /rpc client and by the snapshot tracker, and
# both have to let go before it comes down.
text = re.sub(r'(?m)^linger_secs = .*$', 'linger_secs = 2', text)
text = re.sub(r'(?m)^idle_secs = .*$', 'idle_secs = 2', text)
open(dst, "w").write(text)
PY
if grep -q "^token = " "$WORK/godwinmix.toml" && grep -q "127.0.0.1:$PORT" "$WORK/godwinmix.toml"; then
    ok
else
    bad "the config was not rewritten"
    exit 1
fi

step "core starts"
(cd "$WORK" && "$REPO/target/debug/godwinmix" --config "$WORK/godwinmix.toml") >"$LOG" 2>&1 &
CORE_PID=$!
for _ in $(seq 1 100); do
    curl -fsS "$BASE/api/v1/core/info" "${AUTH[@]}" >/dev/null 2>&1 && break
    kill -0 "$CORE_PID" 2>/dev/null || break
    sleep 0.2
done
if curl -fsS "$BASE/api/v1/core/info" "${AUTH[@]}" >/dev/null 2>&1; then
    ok
else
    bad "the core never answered; see $LOG"
    KEEP=1
    tail -20 "$LOG" >&2
    exit 1
fi

# --- the three doors --------------------------------------------------------

step "GET / serves the UI"
if curl -fsS "$BASE/" | grep -qi "<!doctype html>"; then ok; else bad "no page at /"; fi

step "GET /api/v1/core/info says api_level 1"
INFO="$(curl -fsS "$BASE/api/v1/core/info" "${AUTH[@]}")"
if python3 -c "import json,sys; d=json.load(sys.stdin); sys.exit(0 if d['api_level']==1 else 1)" <<<"$INFO"; then
    ok
else
    bad "core.info: $INFO"
fi

step "an unauthenticated call is refused"
CODE="$(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/v1/core/status")"
if [[ "$CODE" == "401" ]]; then ok; else bad "expected 401, got $CODE"; fi

# What `client/index.js` `detect()` does before it opens anything: one GET of
# core/info, where a 200 or a 401 both mean "this core has /rpc". Getting this
# wrong sends the UI down the deprecated /api and /ws path without saying so.
step "the UI's probe picks /rpc, with or without a token"
NOTOKEN="$(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/v1/core/info")"
WITHTOKEN="$(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/v1/core/info" "${AUTH[@]}")"
if [[ "$NOTOKEN" == "401" && "$WITHTOKEN" == "200" ]]; then
    ok
else
    bad "core/info answered $NOTOKEN without a token and $WITHTOKEN with one"
fi

step "POST /api/v1/sources adds test://smpte"
ADD="$(curl -fsS -X POST "$BASE/api/v1/sources" "${AUTH[@]}" \
    -H 'content-type: application/json' \
    -d '{"id":"bars","uri":"test://smpte","name":"Smoke bars"}')"
if python3 -c "
import json,sys
d = json.load(sys.stdin)
assert d.get('id') == 'bars', d
assert d.get('source', {}).get('uri') or d.get('uri'), d
" <<<"$ADD"; then
    ok
else
    bad "source.add answered: $ADD"
fi

# The pattern that started all this. It must be a refusal, not a dead mixer.
step "a pattern videotestsrc lacks is refused, not fatal"
BAD="$(curl -s -X POST "$BASE/api/v1/sources" "${AUTH[@]}" \
    -H 'content-type: application/json' \
    -d '{"id":"nope","uri":"test://bars"}')"
if grep -q "smpte" <<<"$BAD" && curl -fsS "$BASE/api/v1/core/status" "${AUTH[@]}" >/dev/null; then
    ok
else
    bad "expected an error listing the patterns, got: $BAD"
fi

step "POST /api/v1/program/take puts it on air"
TAKE="$(curl -fsS -X POST "$BASE/api/v1/program/take" "${AUTH[@]}" \
    -H 'content-type: application/json' -d '{"source":"bars"}')"
if grep -q '"bars"' <<<"$TAKE"; then ok; else bad "program.take answered: $TAKE"; fi

step "GET /api/v1/sources shows it live"
for _ in $(seq 1 50); do
    LIST="$(curl -fsS "$BASE/api/v1/sources" "${AUTH[@]}")"
    grep -q '"state":"live"' <<<"$LIST" && break
    sleep 0.2
done
if python3 -c "
import json,sys
d = json.load(sys.stdin)
rows = d['sources'] if isinstance(d, dict) else d
one = [s for s in rows if s['id'] == 'bars']
assert one, rows
assert one[0]['state'] == 'live', one[0]
" <<<"$LIST"; then
    ok
else
    bad "source.list answered: $LIST"
fi

step "/metrics carries gmx_programme_frame_interval_ms"
if curl -fsS "$BASE/metrics" | grep -q "gmx_programme_frame_interval_ms"; then
    ok
else
    bad "the histogram is not in the scrape"
fi

step "GET /api/v1/snapshot/sheet returns a JPEG"
if curl -fsS "$BASE/api/v1/snapshot/sheet?width=320" "${AUTH[@]}" -o "$WORK/sheet.jpg" \
    && [[ -s "$WORK/sheet.jpg" ]] \
    && [[ "$(head -c 2 "$WORK/sheet.jpg" | xxd -p)" == "ffd8" ]]; then
    ok
else
    bad "no JPEG came back"
fi

# --- /rpc -------------------------------------------------------------------

step "/rpc: subscribe, snapshot, flush, a mosaic frame"
if python3 "$REPO/dev/smoke_rpc.py" "127.0.0.1" "$PORT" "$TOKEN" >"$WORK/rpc.log" 2>&1; then
    ok
else
    bad "$(tail -5 "$WORK/rpc.log")"
fi

step "the mosaic goes after the linger"
DOWN=0
for _ in $(seq 1 40); do
    SUBS="$(curl -fsS "$BASE/metrics" | awk '/^gmx_multiview_subscribers/ {print $2}')"
    if [[ "$SUBS" == "0" ]]; then
        DOWN=1
        break
    fi
    sleep 0.25
done
if [[ $DOWN -eq 1 ]]; then ok; else bad "gmx_multiview_subscribers stuck at ${SUBS:-unknown}"; fi

step "and the log says the mosaic was torn down"
if grep -qi "multiview\|mosaic" "$LOG"; then ok; else bad "nothing in the log about the mosaic"; fi

# --- presets ----------------------------------------------------------------
#
# The volunteer's install, on a directory that has nothing in it: the plan
# first, then the real thing, then a core started from what it wrote.

PWORK="$WORK/preset"
mkdir -p "$PWORK"

step "gmx preset list names the six official presets"
LISTED="$("$GMX" preset list --json 2>"$WORK/preset.log" | python3 -c 'import json,sys; print(",".join(sorted(r["name"] for r in json.load(sys.stdin) if r["official"])))')"
if [[ "$LISTED" == "broadcast,church,classroom,default,esports,headless-agent" ]]; then
    ok
else
    bad "listed ${LISTED:-nothing}"
fi

step "preset apply --dry-run names the camera plugin"
"$GMX" preset apply church --dry-run --config "$PWORK/godwinmix.toml" >"$WORK/dry.log" 2>&1
if grep -q "MISSING  camera" "$WORK/dry.log" && [[ ! -f "$PWORK/godwinmix.toml" ]]; then
    ok
else
    bad "$(tail -3 "$WORK/dry.log")"
fi

step "and nothing else in the plan is wrong"
if grep -qiE "error|does not (load|apply|resolve|validate)" "$WORK/dry.log"; then
    bad "$(grep -iE -m 2 'error|does not' "$WORK/dry.log")"
else
    ok
fi

step "preset apply writes config, scenes and [ui]"
"$GMX" preset apply church --config "$PWORK/godwinmix.toml" >"$WORK/apply.log" 2>&1
if [[ -f "$PWORK/godwinmix.toml" ]] && [[ -f "$PWORK/godwinmix.scenes.json" ]] \
    && grep -q '^\[ui\]' "$PWORK/godwinmix.runtime.toml"; then
    ok
else
    bad "$(tail -3 "$WORK/apply.log")"
fi

step "the preset's own comments came with its config"
if grep -q "# The church preset" "$PWORK/godwinmix.toml"; then ok; else bad "the comments were lost"; fi

step "applying it twice does not double the sources"
BEFORE="$(grep -c '^\[\[sources\]\]' "$PWORK/godwinmix.toml")"
"$GMX" preset apply church --config "$PWORK/godwinmix.toml" >>"$WORK/apply.log" 2>&1
AFTER="$(grep -c '^\[\[sources\]\]' "$PWORK/godwinmix.toml")"
if [[ "$BEFORE" == "$AFTER" ]]; then ok; else bad "$BEFORE sources became $AFTER"; fi

step "a core starts from what the preset wrote"
PPORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
python3 - "$PWORK/godwinmix.toml" "$PPORT" <<'PYEOF'
import re, sys
path, port = sys.argv[1], sys.argv[2]
text = open(path).read()
open(path, "w").write(re.sub(r'bind = "[^"]*"', f'bind = "127.0.0.1:{port}"', text))
PYEOF
"$REPO/target/debug/godwinmix" --config "$PWORK/godwinmix.toml" >"$WORK/preset-core.log" 2>&1 &
PRESET_PID=$!
PUP=0
for _ in $(seq 1 80); do
    if curl -fsS "http://127.0.0.1:$PPORT/api/v1/core/info" >"$WORK/pinfo.json" 2>/dev/null; then PUP=1; break; fi
    sleep 0.25
done
if [[ $PUP -eq 1 ]]; then ok; else bad "the core did not come up: $(tail -3 "$WORK/preset-core.log")"; fi

step "core.info carries the preset's theme and gallery"
UI="$(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])).get("ui") or {}; print(d.get("preset"), d.get("theme"), d.get("gallery"), len(d.get("layout") or {}))' "$WORK/pinfo.json" 2>/dev/null)"
if [[ "$UI" == "church calm icon 4" ]]; then ok; else bad "core.info ui is '${UI:-absent}'"; fi

step "the preset's theme is served over HTTP"
if curl -fsS "http://127.0.0.1:$PPORT/presets/church/theme.css" | grep -q -- "--live"; then
    ok
else
    bad "no stylesheet at /presets/church/theme.css"
fi

step "ctl status: the preset's sources and outputs"
GODWINMIX_URL="http://127.0.0.1:$PPORT" "$GMX" ctl status >"$WORK/pstatus.log" 2>&1
if grep -q "cam-wide" "$WORK/pstatus.log" && grep -q "youtube" "$WORK/pstatus.log"; then
    ok
else
    bad "$(tr '\n' '; ' <"$WORK/pstatus.log")"
fi

step "preset.apply over the API returns the plan"
APPLIED="$(curl -fsS -X POST "http://127.0.0.1:$PPORT/api/v1/preset/apply" \
    -H 'content-type: application/json' -d '{"name":"church","dry_run":true}' \
    | python3 -c 'import json,sys; r=json.load(sys.stdin); print(r["dry_run"], len(r["plan"]["steps"]))' 2>/dev/null)"
if [[ "$APPLIED" == "True 3" ]]; then ok; else bad "preset.apply answered '${APPLIED:-nothing}'"; fi

step "preset save writes one the loader reads back"
"$GMX" preset save my-church --config "$PWORK/godwinmix.toml" --out "$PWORK/mine" >"$WORK/save.log" 2>&1
if "$GMX" preset show "$PWORK/mine" --config "$PWORK/second.toml" >"$WORK/show.log" 2>&1 \
    && grep -q "YOUR-STREAM-KEY" "$PWORK/mine/config/godwinmix.toml" \
    && ! grep -q "token = \"" "$PWORK/mine/config/godwinmix.toml"; then
    ok
else
    bad "$(tail -3 "$WORK/save.log") $(tail -3 "$WORK/show.log")"
fi

step "gmx build refuses to bundle a copyleft codec entry"
"$GMX" build --preset church --name "SmokeMix" --out "$PWORK/build" --no-binary >"$WORK/build2.log" 2>&1
if grep -q "copyleft" "$WORK/build2.log" \
    && [[ -f "$PWORK/build/tauri.conf.json" ]] \
    && ! grep -q "GPL-2.0" "$PWORK/build/codecs.toml"; then
    ok
else
    bad "$(tail -3 "$WORK/build2.log")"
fi

kill "$PRESET_PID" 2>/dev/null
wait "$PRESET_PID" 2>/dev/null

# --- the clients ------------------------------------------------------------

step "gmx ctl status"
GODWINMIX_URL="$BASE" GODWINMIX_TOKEN="$TOKEN" "$GMX" ctl status >"$WORK/ctl.log" 2>&1
if grep -q "bars" "$WORK/ctl.log"; then
    ok
else
    bad "ctl status did not list the source: $(tr '\n' '; ' <"$WORK/ctl.log")"
fi

step "gmx mcp lists 12 tools on standard"
COUNT="$(python3 "$REPO/dev/smoke_mcp.py" "$GMX" "$BASE" "$TOKEN" standard 2>>"$WORK/mcp.log")"
if [[ "$COUNT" == "12" ]]; then ok; else bad "standard listed ${COUNT:-nothing}, wanted 12"; fi

step "gmx mcp lists 5 tools on minimal"
COUNT="$(python3 "$REPO/dev/smoke_mcp.py" "$GMX" "$BASE" "$TOKEN" minimal 2>>"$WORK/mcp.log")"
if [[ "$COUNT" == "5" ]]; then ok; else bad "minimal listed ${COUNT:-nothing}, wanted 5"; fi

# --- shutdown ---------------------------------------------------------------

step "core.shutdown stops the process"
curl -fsS -X POST "$BASE/api/v1/core/shutdown" "${AUTH[@]}" -d '{}' \
    -H 'content-type: application/json' >/dev/null 2>&1
GONE=0
for _ in $(seq 1 60); do
    kill -0 "$CORE_PID" 2>/dev/null || { GONE=1; break; }
    sleep 0.25
done
if [[ $GONE -eq 1 ]]; then ok; else bad "the core is still running after core.shutdown"; fi
wait "$CORE_PID" 2>/dev/null
CORE_PID=""

step "the log has no panic and no unclosed delimiter"
if grep -qiE "panicked at|unclosed delimiter|has no property" "$LOG"; then
    bad "$(grep -iE -m 3 'panicked at|unclosed delimiter|has no property' "$LOG")"
else
    ok
fi

step "nothing was left running"
LEFT="$(pgrep -f "godwinmix-browser|godwinmix --config $WORK" 2>/dev/null | wc -l | tr -d ' ')"
if [[ "$LEFT" == "0" ]]; then ok; else bad "$LEFT stray process(es)"; fi

echo
if [[ $FAILED -eq 0 ]]; then
    echo "all steps ok"
    exit 0
fi
echo "$FAILED step(s) failed"
KEEP=1
exit 1
