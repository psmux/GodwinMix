#!/usr/bin/env python3
"""Where do the flashes and beeps of the sync pattern land in a capture?

Prints the audio minus video offset per pair. Usage: measure-sync.py FILE [-ss S -t T]
"""
import subprocess, re, sys, os
f = sys.argv[1]; extra = sys.argv[2:]
def onsets(args, key, thr, floor):
    r = subprocess.run(['ffmpeg','-hide_banner'] + extra + ['-i', f] + args + ['-f','null','-'], capture_output=True, text=True).stderr
    t = None; on = []; prev = floor
    for l in r.splitlines():
        m = re.search(r'pts_time:([\d.]+)', l)
        if m: t = float(m.group(1))
        m = re.search(key + r'=(-?[\d.]+|-inf)', l)
        if m and t is not None:
            v = floor if m.group(1) == '-inf' else float(m.group(1))
            if v > thr and prev <= thr: on.append(t)
            prev = v
    return on
fl = onsets(['-vf','signalstats,metadata=print:key=lavfi.signalstats.YAVG'], 'YAVG', 120, 0)
bp = onsets(['-vn','-af','asetnsamples=n=240,astats=metadata=1:reset=1,ametadata=print:key=lavfi.astats.Overall.RMS_level'], 'RMS_level', -30, -120)
pairs = [min(bp, key=lambda b: abs(b - x)) - x for x in fl if any(abs(b - x) < 0.8 for b in bp)]
name = os.path.basename(f)
if pairs:
    print(f"{name}: flashes={len(fl)} beeps={len(bp)} audio-video offset ms: mean={1000*sum(pairs)/len(pairs):+.0f} min={1000*min(pairs):+.0f} max={1000*max(pairs):+.0f} (n={len(pairs)})")
else:
    print(f"{name}: flashes={len(fl)} beeps={len(bp)} NO PAIRS")
