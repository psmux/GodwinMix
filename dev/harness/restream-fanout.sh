#!/usr/bin/env bash
# One publisher fanned out to three local RTMP receivers by the restreamer in
# the ingest plugin, the way a channel sends one stream to YouTube, Facebook
# and Twitch at once. The middle receiver is killed part way through and
# started again. At the end it prints the fan out process's CPU and memory and
# checks every receiver's recording against the publisher's own copy.
#
#   dev/harness/restream-fanout.sh             two minutes, ports 19352 to 19355
#   SECS=30 dev/harness/restream-fanout.sh     a shorter run
#
# Needs ffmpeg and ffprobe. The receivers are `ffmpeg -listen 1`, so nothing
# of ours is on the receiving end. Everything lands in $OUT (a temp directory
# unless given).
set -euo pipefail
cd "$(dirname "$0")/../.."

SECS=${SECS:-120}
IN=${IN:-19355}
PORTS=(19352 19353 19354)
KILL_AT=${KILL_AT:-50}
BACK_AT=${BACK_AT:-60}
OUT=${OUT:-$(mktemp -d)}
echo "restream-fanout: writing to $OUT"

BIN=$(cargo test --release -p gmx-ingest --no-run 2>&1 | grep -o 'target/release/deps/gmx_ingest-[a-f0-9]*' | head -1)
[ -x "$BIN" ] || { echo "could not build the gmx-ingest test binary" >&2; exit 1; }

receive() { # port, file
  ffmpeg -hide_banner -loglevel error -listen 1 -i "rtmp://127.0.0.1:$1/live/key" -c copy -f flv "$2" &
}
declare -a RECV
for p in "${PORTS[@]}"; do receive "$p" "$OUT/recv-$p-1.flv"; RECV+=($!); done
trap 'kill ${RECV[@]} $FAN $PUB 2>/dev/null || true' EXIT
sleep 1

urls=$(printf 'rtmp://127.0.0.1:%s/live/key,' "${PORTS[@]}"); urls=${urls%,}
GMX_FANOUT_IN=$IN GMX_FANOUT_OUT=$urls GMX_FANOUT_SECS=$((SECS + 5)) \
  "$BIN" restream::fanout::fanout --ignored --exact --nocapture 2>"$OUT/fanout.log" >/dev/null &
FAN=$!
sleep 1

ffmpeg -hide_banner -loglevel error -re \
  -f lavfi -i testsrc2=size=1920x1080:rate=30 -f lavfi -i sine=frequency=440:sample_rate=48000 \
  -map 0:v -map 1:a -c:v libx264 -preset veryfast -tune zerolatency -pix_fmt yuv420p \
  -b:v 6M -maxrate 6M -bufsize 6M -g 60 -c:a aac -b:a 128k -t "$SECS" \
  -f tee "[f=flv]rtmp://127.0.0.1:$IN/live/main|[f=flv]$OUT/source.flv" &
PUB=$!

for ((t = 0; t < SECS; t += 5)); do
  sleep 5
  echo "$((t + 5)) $(ps -o %cpu=,rss=,time= -p $FAN)" >>"$OUT/usage.txt"
  if [ $((t + 5)) -eq "$KILL_AT" ]; then kill -9 "${RECV[1]}"; echo "killed the receiver on ${PORTS[1]} at ${KILL_AT}s"; fi
  if [ $((t + 5)) -eq "$BACK_AT" ]; then receive "${PORTS[1]}" "$OUT/recv-${PORTS[1]}-2.flv"; RECV[1]=$!; echo "started it again at ${BACK_AT}s"; fi
done
wait "$PUB" || true
# The publisher leaving ends each destination, which closes its stream, and a
# receiver that is told the stream is over writes the rest of its file and
# exits. Interrupting it instead would lose what it still held.
for _ in 1 2 3 4 5 6 7 8 9 10; do
  kill -0 "${RECV[@]}" 2>/dev/null || break
  sleep 1
done
kill -INT "${RECV[@]}" 2>/dev/null || true
sleep 1

echo "--- the fan out process, sampled every five seconds: %CPU and RSS in KiB"
awk '{c+=$2; if ($3>m) m=$3; n++} END {printf "mean CPU %.1f%%, peak RSS %.1f MiB over %d samples\n", c/n, m/1024, n}' "$OUT/usage.txt"
echo "CPU time used in all: $(tail -n 1 "$OUT/usage.txt" | awk '{print $4}') (minutes:seconds) over ${SECS}s"
echo "--- the restreamer's own last report"
tail -n 3 "$OUT/fanout.log"
echo "--- the recordings"
python3 dev/harness/restream_check.py "$OUT" "${PORTS[@]}"
