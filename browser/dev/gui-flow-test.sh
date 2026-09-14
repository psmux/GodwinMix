#!/bin/bash
# The operator's flow, driven through the API exactly as the UI drives it:
# paste plain URLs, add, click tiles. The programme is recorded throughout.
export PATH="/home/dev/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin"; export DOCKER_HOST=unix://$HOME/.colima/default/docker.sock
cd "$(dirname "$0")/../.."; LB=target/release/godwinmix; OUT=browser/dev/out; API=http://127.0.0.1:8080
have() { $LB ctl source list | awk '{print $1}' | grep -qx "$1"; }
add() { have "$2" && { echo "  $2 already there"; return; }; printf "  add %-16s -> HTTP %s\n" "$2" "$(curl -s -o /dev/null -w '%{http_code}' -X POST $API/api/sources -H 'content-type: application/json' -d "$1")"; }
echo "=== add sites (no prefix, no id) ==="
add '{"uri":"http://host.docker.internal:8090/video-mp4.html","kind":"web","name":"H264 test page"}' h264-test-page
add '{"uri":"https://www.youtube.com/watch?v=aqz-KE-bpKQ","kind":"web","name":"YouTube"}' youtube
sleep 15; echo "=== sources ==="; $LB ctl source list
echo "=== record the programme while switching every 10 s ==="
CAP=$OUT/gui_flow.flv; rm -f $CAP
ffmpeg -hide_banner -loglevel error -rw_timeout 30000000 -i rtmp://127.0.0.1:1935/live/program -c copy -f flv $CAP >/dev/null 2>&1 & REC=$!
T0=$(date +%s)
for s in cam1 h264-test-page youtube h264-test-page cam1; do
  $LB ctl take $s >/dev/null && printf "  t=%2ds take %s\n" $(( $(date +%s)-T0 )) $s; sleep 10
done
kill -INT $REC; sleep 2
echo "=== output health ==="; $LB ctl status | grep output
echo "=== continuity: frames per second across the whole recording ==="
ffmpeg -hide_banner -i $CAP -vf "signalstats,metadata=print:key=lavfi.signalstats.YAVG" -f null - 2>&1 | grep -oE 'pts_time:[0-9.]+' | cut -d: -f2 | awk '{s=int($1); n[s]++} END{min=999;max=0; for(i in n){if(n[i]<min&&i>0&&i<int(NR/30)-1)min=n[i]; if(n[i]>max)max=n[i]} printf "  %d frames, per-second min=%d max=%d (excluding first and last second)\n", NR, min, max}'
echo "=== sound per segment (mean volume; the H264 page is silent between its beeps) ==="
for seg in "1 8 cam1" "12 8 h264-page" "22 8 youtube" "32 8 h264-page" "42 6 cam1"; do set -- $seg
  v=$(ffmpeg -hide_banner -ss $1 -t $2 -i $CAP -vn -af volumedetect -f null - 2>&1 | grep -oE "max_volume: -?[0-9.]+ dB" | head -1)
  m=$(ffmpeg -hide_banner -ss $1 -t $2 -i $CAP -vf "signalstats,metadata=print:key=lavfi.signalstats.YDIF" -f null - 2>&1 | grep -oE 'YDIF=[0-9.]+' | awk -F= '{n++; if($2>0.5)c++} END{printf "%d/%d frames moving", c, n}')
  printf "  %-11s %-26s %s\n" "$3" "$v" "$m"
done
echo "=== sync on the H264 page segments ==="; python3 browser/dev/measure-sync.py $CAP -ss 11 -t 10; python3 browser/dev/measure-sync.py $CAP -ss 31 -t 10
echo "=== containers match web sources? ==="; echo "  web sources: $($LB ctl source list | grep -c 'web+')  containers: $(docker ps -q --filter ancestor=gmx-browser | wc -l | tr -d ' ')"
echo "  dropouts logged during the run: $(tail -400 dev/harness/logs/sidecar.log | grep -c re-anchored)"
ffmpeg -hide_banner -loglevel error -y -i $CAP -vf "select='eq(n\,150)+eq(n\,480)+eq(n\,780)',tile=3x1,scale=1536:-1" -frames:v 1 $OUT/gui_flow_tiles.png && echo "  frames from the three sites: $OUT/gui_flow_tiles.png"
