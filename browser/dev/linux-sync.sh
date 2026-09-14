#!/bin/bash
# A/V sync and codec check for the sidecar, in the :gst image. Renders three
# pages that flash and beep together every 2 s (WebAudio; a VP9/Opus <video>;
# an H.264/AAC <video>), captures each as raw Matroska, then measures where the
# flashes and beeps land relative to each other. Also checks SIGTERM shutdown.
set -e
cd /work/browser
export CEF_PATH=/cefcache
cargo build --release 2>&1 | grep -E '^(error|\s+-->)' -A6 | head -20 || true
BIN=/work/browser/target/release/godwinmix-browser
OUT=/work/browser/dev/out; mkdir -p "$OUT"
Xvfb :99 -screen 0 1280x720x24 -nolisten tcp >/dev/null 2>&1 &
sleep 1
export XDG_RUNTIME_DIR=/tmp/xdg; mkdir -p $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
pulseaudio --start --exit-idle-time=-1 >/dev/null 2>&1 || true
pactl load-module module-null-sink sink_name=gmx >/dev/null 2>&1 || true
export DISPLAY=:99 PULSE_SINK=gmx
echo "rpath: $(ldd $BIN | grep libcef | head -1)"

for page in sync video-webm video-mp4; do
  # No --resources-dir / --locales-dir: the defaults must work.
  timeout 60 "$BIN" --url file:///work/browser/test/$page.html --width 1280 --height 720 --fps 30 --seconds 12 \
    > "$OUT/$page.mkv" 2> "$OUT/$page.log" || echo "$page: exit $?"
  grep -E '\[browser\]|video error|play failed|CONSOLE' "$OUT/$page.log" | grep -v 'dbus' | head -8 | sed "s/^/  [$page] /"
done

echo "=== SIGTERM shutdown ==="
"$BIN" --url file:///work/browser/test/page.html --width 640 --height 360 --fps 30 > /dev/null 2> "$OUT/term.log" &
PID=$!; sleep 6
kill -TERM $PID; for i in $(seq 1 40); do kill -0 $PID 2>/dev/null || break; sleep 0.1; done
if kill -0 $PID 2>/dev/null; then echo "still running after 4 s: FAIL"; kill -9 $PID; else echo "exited within $((i*100)) ms after SIGTERM"; fi
sleep 1; echo "leftover browser processes: $(pgrep -fc godwinmix-browser || true)"
grep '\[browser\]' "$OUT/term.log" | tail -3

echo "=== measurement ==="
python3 - "$OUT" <<'PY'
import subprocess, re, sys, os
out = sys.argv[1]
def flashes(f):
    r = subprocess.run(['ffmpeg','-hide_banner','-i',f,'-vf','signalstats,metadata=print:key=lavfi.signalstats.YAVG','-f','null','-'],capture_output=True,text=True).stderr
    t=None; on=[]; prev=0
    for l in r.splitlines():
        m=re.search(r'pts_time:([\d.]+)',l)
        if m: t=float(m.group(1))
        m=re.search(r'YAVG=([\d.]+)',l)
        if m and t is not None:
            y=float(m.group(1)); 
            if y>120 and prev<=120: on.append(t)
            prev=y
    return on
def beeps(f):
    r = subprocess.run(['ffmpeg','-hide_banner','-i',f,'-vn','-af','asetnsamples=n=240,astats=metadata=1:reset=1,ametadata=print:key=lavfi.astats.Overall.RMS_level','-f','null','-'],capture_output=True,text=True).stderr
    t=None; on=[]; prev=-90
    for l in r.splitlines():
        m=re.search(r'pts_time:([\d.]+)',l)
        if m: t=float(m.group(1))
        m=re.search(r'RMS_level=(-?[\d.]+|-inf)',l)
        if m and t is not None:
            v=-120 if m.group(1)=='-inf' else float(m.group(1))
            if v>-30 and prev<=-30: on.append(t)
            prev=v
    return on
for page in ('sync','video-webm','video-mp4'):
    f=os.path.join(out,page+'.mkv')
    if not os.path.exists(f) or os.path.getsize(f)<1000: print(f"{page:11s} no capture"); continue
    fl=flashes(f); bp=beeps(f)
    pairs=[]
    for x in fl:
        near=[b for b in bp if abs(b-x)<0.8]
        if near: pairs.append(min(near,key=lambda b:abs(b-x))-x)
    print(f"{page:11s} flashes={len(fl)} beeps={len(bp)} " + (f"audio-video offset ms: mean={1000*sum(pairs)/len(pairs):+.0f} min={1000*min(pairs):+.0f} max={1000*max(pairs):+.0f} (n={len(pairs)})" if pairs else "NO PAIRS"))
PY
