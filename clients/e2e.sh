#!/usr/bin/env bash
#
# Run a command against a real core, the way dev/smoke.sh starts one.
#
#   clients/e2e.sh node clients/typescript/e2e/e2e.ts "$GMX_URL" "$GMX_TOKEN"
#
# Starts a core on a free port with a token and no sources, waits for it to
# answer, runs the command with GMX_URL and GMX_TOKEN in the environment and
# appended as $1 and $2, then takes it down. The command's exit status is this
# script's exit status.
#
# Needs: cargo, python3 (standard library only), curl.

set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/gmx-e2e.XXXXXX")"
LOG="$WORK/core.log"
TOKEN="e2e-$RANDOM$RANDOM"
CORE_PID=""

cleanup() {
    if [[ -n "$CORE_PID" ]] && kill -0 "$CORE_PID" 2>/dev/null; then
        kill "$CORE_PID" 2>/dev/null
        wait "$CORE_PID" 2>/dev/null
    fi
    [[ -n "${KEEP:-}" ]] && echo "kept: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT

PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
BASE="http://127.0.0.1:$PORT"

echo "building the core"
if ! (cd "$REPO" && cargo build --quiet) >"$WORK/build.log" 2>&1; then
    echo "cargo build failed:" >&2
    tail -20 "$WORK/build.log" >&2
    exit 1
fi

# The example config with every source and output commented out: the point is a
# core that starts clean and is driven entirely through the protocol.
"$REPO/target/debug/godwinmix" --example-config >"$WORK/example.toml" 2>/dev/null
python3 - "$WORK/example.toml" "$WORK/godwinmix.toml" "$PORT" "$TOKEN" <<'PY'
import re, sys

src, dst, port, token = sys.argv[1:5]
out, skipping = [], False
for line in open(src).read().splitlines():
    stripped = line.strip()
    if stripped.startswith("[["):
        skipping = stripped in ("[[sources]]", "[[outputs]]")
    elif stripped.startswith("[") and not stripped.startswith("[["):
        skipping = False
    out.append("# " + line if skipping and not line.startswith("#") else line)
text = "\n".join(out) + "\n"
text = re.sub(r'(?m)^bind = .*$', f'bind = "127.0.0.1:{port}"', text)
text = re.sub(r'(?m)^# token = .*$', f'token = "{token}"', text)
text = re.sub(r'(?m)^linger_secs = .*$', 'linger_secs = 2', text)
text = re.sub(r'(?m)^idle_secs = .*$', 'idle_secs = 2', text)
open(dst, "w").write(text)
PY

echo "starting a core on $BASE"
(cd "$WORK" && "$REPO/target/debug/godwinmix" --config "$WORK/godwinmix.toml") >"$LOG" 2>&1 &
CORE_PID=$!
for _ in $(seq 1 100); do
    curl -fsS "$BASE/api/v1/core/info" -H "Authorization: Bearer $TOKEN" >/dev/null 2>&1 && break
    kill -0 "$CORE_PID" 2>/dev/null || break
    sleep 0.2
done
if ! curl -fsS "$BASE/api/v1/core/info" -H "Authorization: Bearer $TOKEN" >/dev/null 2>&1; then
    echo "the core never answered; its log:" >&2
    tail -20 "$LOG" >&2
    exit 1
fi

echo
export GMX_URL="$BASE"
export GMX_TOKEN="$TOKEN"
"$@" "$BASE" "$TOKEN"
STATUS=$?

echo
if [[ $STATUS -eq 0 ]]; then
    echo "end to end run passed against a live core"
else
    echo "end to end run failed (exit $STATUS); the core's log:" >&2
    tail -20 "$LOG" >&2
fi
exit $STATUS
