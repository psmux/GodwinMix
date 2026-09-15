#!/bin/zsh
# Feed a captured sidecar stream into a running mixer as an exec: source, take
# it to program, and measure what actually went out. Runs on the host against
# the mixer's control port; the stream file comes from dev/linux-m2.sh.
#
#   dev/mixer-integration.sh [stream.mkv] [ctl-url]
set -e
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin"
STREAM="$(cd "$(dirname "${1:-$(dirname "$0")/out/stream.mkv}")" && pwd)/$(basename "${1:-stream.mkv}")"
URL="${2:-http://127.0.0.1:8080}"
LB="$(dirname "$0")/../../target/release/godwinmix"
OUT="$(dirname "$0")/out"
[ -s "$STREAM" ] || { echo "no stream at $STREAM"; exit 1; }

echo "=== add the sidecar stream as an exec source (looped in real time, so it behaves like a live page) ==="
# Not `cat` in a loop: concatenating a second Matroska header mid-stream reads
# as end-of-stream to the demuxer, the source goes dead, and the compositor
# holds the last frame while the audiomixer gets nothing. Re-muxing with
# continuous timestamps and real-time pacing is what a live page looks like.
"$LB" ctl --url "$URL" source remove cefpage >/dev/null 2>&1 || true
"$LB" ctl --url "$URL" source add cefpage \
  "exec:ffmpeg -hide_banner -loglevel error -re -stream_loop -1 -i $STREAM -c copy -f matroska -" --name "CEF page"
sleep 8
"$LB" ctl --url "$URL" source list

echo "=== record program while taking it to air and back ==="
CAP="$OUT/program_cap.flv"
rm -f "$CAP"
ffmpeg -hide_banner -loglevel error -rw_timeout 30000000 -i rtmp://127.0.0.1:1935/live/program -c copy -f flv "$CAP" >/dev/null 2>&1 &
REC=$!
sleep 4
"$LB" ctl --url "$URL" take cefpage
sleep 8
"$LB" ctl --url "$URL" take cam1
sleep 5
kill -INT $REC 2>/dev/null; sleep 2; kill -9 $REC 2>/dev/null || true

ffmpeg -hide_banner -loglevel info -i "$CAP" \
  -vf "signalstats,metadata=print:key=lavfi.signalstats.YAVG" \
  -af "aspectralstats=measure=centroid,ametadata=print:key=lavfi.aspectralstats.1.centroid" \
  -f null - > "$OUT/program_an.log" 2>&1
python3 - "$OUT/program_an.log" <<'PY'
import re, sys
pts, vid, aud = [], [], []
cur = None
for l in open(sys.argv[1], 'rb').read().decode('utf8', 'replace').splitlines():
    m = re.search(r'pts_time:([\d.]+)', l)
    if m: cur = float(m.group(1))
    m = re.search(r'signalstats\.YAVG=([\d.]+)', l)
    if m and cur is not None: pts.append(cur); vid.append((cur, float(m.group(1))))
    m = re.search(r'aspectralstats\.1\.centroid=([\d.]+)', l)
    if m and cur is not None: aud.append((cur, float(m.group(1))))
if not pts:
    print("no frames captured"); sys.exit(1)
counts = [len([1 for p in pts if s <= p < s + 1]) for s in range(int(pts[-1]))]
gaps = sorted(((pts[i+1]-pts[i], pts[i]) for i in range(len(pts)-1)), reverse=True)
def alab(f):
    if f < 120: return 'SILENCE'
    if 300 < f < 560: return 'cam1 440Hz'
    if 560 < f < 780: return 'PAGE 660Hz'
    return f'{f:.0f}Hz (transition)'
def vlab(y):
    if y < 20: return 'black'
    if 30 < y < 55: return 'PAGE (luma~41)'
    if 88 < y < 105: return 'cam1 bars'
    return f'luma {y:.0f} (transition)'
print(f"  fps per second : min={min(counts)} max={max(counts)}")
print(f"  largest gap    : {gaps[0][0]*1000:.0f} ms")
print(f"  {'sec':>4}  {'video':<24} audio")
last = None
for s in range(int(pts[-1])):
    v = [f for t, f in aud if s <= t < s + 1]
    y = [f for t, f in vid if s <= t < s + 1]
    if not v or not y: continue
    l = (vlab(sum(y)/len(y)), alab(sum(v)/len(v)))
    if l != last: print(f"  {s:>4}  {l[0]:<24} {l[1]}"); last = l
print("  => CONTINUOUS" if min(counts) >= 29 else "  => GAP")
PY
"$LB" ctl --url "$URL" source remove cefpage >/dev/null 2>&1 || true
