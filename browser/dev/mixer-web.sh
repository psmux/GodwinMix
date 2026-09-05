#!/bin/bash
# End to end on the mixer: add a page by URL (web+http), which runs the CEF
# sidecar, take it to air, record the programme, and measure what went out:
# frame rate, A/V offset of the page's flash and beep pattern, and levels.
#   browser/dev/mixer-web.sh [page] [ctl-url]      page: sync | video-webm | page
set -e
export PATH="/home/dev/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin"
PAGE="${1:-sync}"; URL="${2:-http://127.0.0.1:8080}"
D="$(cd "$(dirname "$0")" && pwd)"; LB="$D/../../target/release/liveboxmix"; OUT="$D/out"
"$LB" ctl --url "$URL" source remove web1 >/dev/null 2>&1 || true
"$LB" ctl --url "$URL" source add web1 "web+http://$WEB_HOST/$PAGE.html" --name "Web page"
sleep ${PRE_TAKE:-6}
"$LB" ctl --url "$URL" source list
CAP="$OUT/program_web_$PAGE.flv"; rm -f "$CAP"
ffmpeg -hide_banner -loglevel error -rw_timeout 30000000 -i rtmp://127.0.0.1:1935/live/program -c copy -f flv "$CAP" >/dev/null 2>&1 &
REC=$!; sleep 3
"$LB" ctl --url "$URL" take web1
sleep 14
"$LB" ctl --url "$URL" take cam1
sleep 3
kill -INT $REC 2>/dev/null; sleep 2; kill -9 $REC 2>/dev/null || true
echo "--- frame rate while the page was on air (3 s .. 17 s of the capture)"
ffmpeg -hide_banner -ss 3 -t 14 -i "$CAP" -vf "showinfo" -f null - 2>&1 | grep -c "pts_time" | awk '{printf "  %d frames in 14 s = %.1f fps\n", $1, $1/14}'
echo "--- A/V offset on air"
python3 "$D/measure-sync.py" "$CAP" -ss 4 -t 12
echo "--- levels: darkest and brightest luma on air (limited range is 16..235; the sync page is 16 black / 235 white)"
ffmpeg -hide_banner -ss 5 -t 8 -i "$CAP" -vf signalstats,metadata=print -f null - 2>&1 | grep -oE 'Y(MIN|MAX)=[0-9.]+' | sort | uniq -c | sort -rn | head -4 | awk '{printf "  %s x%d\n",$2,$1}'
"$LB" ctl --url "$URL" source remove web1 >/dev/null 2>&1 || true
