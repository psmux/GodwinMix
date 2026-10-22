#!/bin/sh
# Programs of one multicast group into direct shows, one show per program,
# each copied to a UDP receiver: the native MPEG-TS input against the
# GStreamer one. Prints the direct host's CPU (a share of one core),
# resident memory and threads for each, read in the second half of the run.
#
#   dev/bench/mpts.sh [--programs 9] [--seconds 60] [--group 239.255.18.1]
#                     [--port 18100] [--iface lo] [--engines "native gstreamer"]
#
# The feed is one 720p30 H.264 encode at 3.5 Mbit/s with two AC-3 tracks,
# copied into every program of one multiplex by ffmpeg, looped. Needs ffmpeg
# and python3, and builds gmx-ingest in release. Everything it makes is in
# target/bench-mpts, and every process it starts is stopped at the end.

set -eu
PROGRAMS=9 SECONDS_=60 GROUP=239.255.18.1 PORT=18100 IFACE=lo ENGINES="native gstreamer"
while [ $# -gt 0 ]; do
  case "$1" in
    --programs) PROGRAMS=$2 ;; --seconds) SECONDS_=$2 ;; --group) GROUP=$2 ;;
    --port) PORT=$2 ;; --iface) IFACE=$2 ;; --engines) ENGINES=$2 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
  shift 2
done

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
OUT="$ROOT/target/bench-mpts"
mkdir -p "$OUT"
cargo build -q --release -p gmx-ingest --manifest-path "$ROOT/Cargo.toml"
BIN="$ROOT/target/release/gmx-ingest"
ADDR=$(python3 -c "import socket,fcntl,struct,sys
s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM)
print(socket.inet_ntoa(fcntl.ioctl(s.fileno(),0x8915,struct.pack('256s',sys.argv[1][:15].encode()))[20:24]))" "$IFACE" 2>/dev/null || echo 127.0.0.1)

CLIP="$OUT/clip.ts"
[ -f "$CLIP" ] || ffmpeg -loglevel error -y -f lavfi -i testsrc2=size=1280x720:rate=30 \
  -f lavfi -i sine=frequency=440:sample_rate=48000 -f lavfi -i sine=frequency=660:sample_rate=48000 \
  -map 0 -map 1 -map 2 -t 60 -c:v libx264 -preset veryfast -b:v 3500k -maxrate 3500k -bufsize 3500k -g 60 \
  -c:a ac3 -b:a 192k -ac 2 -f mpegts "$CLIP"

PIDS=""
stop() { for p in $PIDS; do kill "$p" 2>/dev/null || true; done; wait 2>/dev/null || true; }
trap stop EXIT INT TERM

# The sender: every program the same three streams, copied.
set -- -loglevel error -re -stream_loop -1 -i "$CLIP"
i=0
while [ $i -lt "$PROGRAMS" ]; do
  n=$((i + 1)); a=$((i * 3))
  set -- "$@" -map 0:0 -map 0:1 -map 0:2 -program "program_num=$n:title=Program $n:st=$a:st=$((a + 1)):st=$((a + 2))"
  i=$n
done
ffmpeg "$@" -c copy -f mpegts "udp://$GROUP:$PORT?pkt_size=1316&localaddr=$ADDR&ttl=1" &
PIDS="$PIDS $!"

# One receiver for every output, so each copy has somewhere to land.
python3 -c "import socket,select,sys
s=[socket.socket(socket.AF_INET,socket.SOCK_DGRAM) for _ in range(int(sys.argv[2]))]
[x.bind(('127.0.0.1',int(sys.argv[1])+1+i)) for i,x in enumerate(s)]
while True:
  for r in select.select(s,[],[])[0]: r.recv(65536)" "$PORT" "$PROGRAMS" &
PIDS="$PIDS $!"

for engine in $ENGINES; do
  TABLE="$OUT/table-$engine.json"
  python3 -c "import json,sys
g,p,n,iface,e=sys.argv[1],int(sys.argv[2]),int(sys.argv[3]),sys.argv[4],sys.argv[5]
rows=[{'id':'p%d'%i,'input':{'uri':'udp://@%s:%d'%(g,p),'program':i,'params':{'interface':iface,'engine':e}},
'outputs':[{'id':'out','uri':'udp://127.0.0.1:%d'%(p+i)}],'monitor':{'alarms':False}} for i in range(1,n+1)]
print(json.dumps({'direct':rows}))" "$GROUP" "$PORT" "$PROGRAMS" "$IFACE" "$engine" > "$TABLE"
  LOG="$OUT/run-$engine.log"
  "$BIN" --direct "$TABLE" --seconds "$SECONDS_" 2> "$LOG" &
  HOST=$!
  : > "$OUT/threads-$engine"
  while kill -0 $HOST 2>/dev/null; do
    awk '/^Threads/ {print $2}' "/proc/$HOST/status" >> "$OUT/threads-$engine" 2>/dev/null || ps -M -p $HOST 2>/dev/null | tail -n +2 | wc -l >> "$OUT/threads-$engine"
    sleep 5
  done
  half=$((SECONDS_ / 10))
  grep '^t=' "$LOG" | tail -n "$half" > "$OUT/tail-$engine"
  cpu=$(sed 's/.*cpu=\([0-9.]*\)%.*/\1/' "$OUT/tail-$engine" | sort -n | awk '{a[NR]=$1} END {print a[1] " to " a[NR]}')
  rss=$(sed 's/.*rss_mb=\([0-9.]*\).*/\1/' "$OUT/tail-$engine" | sort -n | awk '{a[NR]=$1} END {print a[1] " to " a[NR]}')
  thr=$(sort -n "$OUT/threads-$engine" | tail -1)
  echo "$engine: cpu $cpu % of one core, rss $rss MB, threads $thr; last: $(tail -1 "$OUT/tail-$engine")"
done
