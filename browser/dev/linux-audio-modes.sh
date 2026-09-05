#!/bin/bash
# Does Chromium's audio tap still deliver with --disable-audio-output and no
# PulseAudio at all? And what does the H.264 <video> say about itself?
set -e
cd /work/browser; export CEF_PATH=/cefcache
cargo build --release 2>&1 | grep -E '^(error|\s+-->)' -A6 | head -20 || true
BIN=/work/browser/target/release/liveboxmix-browser; OUT=/work/browser/dev/out
Xvfb :99 -screen 0 1280x720x24 -nolisten tcp >/dev/null 2>&1 &
sleep 1; export DISPLAY=:99
unset PULSE_SERVER PULSE_SINK; pkill pulseaudio || true
echo "=== no pulse, --disable-audio-output ==="
LBX_BROWSER_SWITCHES="disable-audio-output" timeout 60 "$BIN" --url file:///work/browser/test/sync.html --width 1280 --height 720 --fps 30 --seconds 10 > "$OUT/nopulse.mkv" 2> "$OUT/nopulse.log" || true
grep '\[browser\]' "$OUT/nopulse.log" | grep -E 'audio|done' | head -4
echo "=== no pulse, default switches ==="
timeout 60 "$BIN" --url file:///work/browser/test/sync.html --width 640 --height 360 --fps 30 --seconds 6 > /dev/null 2> "$OUT/nopulse2.log" || true
grep '\[browser\]' "$OUT/nopulse2.log" | grep -E 'audio|done' | head -3
echo "=== H.264 video element, console ==="
LBX_BROWSER_SWITCHES="enable-logging=stderr,v=0" timeout 60 "$BIN" --url file:///work/browser/test/video-mp4.html --width 640 --height 360 --fps 30 --seconds 6 > /dev/null 2> "$OUT/mp4.log" || true
grep -iE 'CONSOLE|video error|play failed|codec|not supported' "$OUT/mp4.log" | head -5
echo "=== measurement of the no-pulse capture ==="
python3 - "$OUT/nopulse.mkv" <<'PY'
import subprocess, re, sys
f=sys.argv[1]
def onsets(args, key, thr, floor):
    r=subprocess.run(['ffmpeg','-hide_banner','-i',f]+args+['-f','null','-'],capture_output=True,text=True).stderr
    t=None; on=[]; prev=floor
    for l in r.splitlines():
        m=re.search(r'pts_time:([\d.]+)',l)
        if m: t=float(m.group(1))
        m=re.search(key+r'=(-?[\d.]+|-inf)',l)
        if m and t is not None:
            v=floor if m.group(1)=='-inf' else float(m.group(1))
            if v>thr and prev<=thr: on.append(t)
            prev=v
    return on
fl=onsets(['-vf','signalstats,metadata=print:key=lavfi.signalstats.YAVG'],'YAVG',120,0)
bp=onsets(['-vn','-af','asetnsamples=n=240,astats=metadata=1:reset=1,ametadata=print:key=lavfi.astats.Overall.RMS_level'],'RMS_level',-30,-120)
pairs=[min(bp,key=lambda b:abs(b-x))-x for x in fl if any(abs(b-x)<0.8 for b in bp)]
print(f"flashes={len(fl)} beeps={len(bp)} " + (f"audio-video offset ms: mean={1000*sum(pairs)/len(pairs):+.0f} min={1000*min(pairs):+.0f} max={1000*max(pairs):+.0f}" if pairs else "NO PAIRS"))
PY
