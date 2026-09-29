#!/usr/bin/env python3
"""Check what restream-fanout.sh recorded.

For every receiver: how many video frames and keyframes arrived, the largest
gap between two frames, and whether each recording starts on a keyframe. A
receiver that was never killed must have every frame the publisher sent, with
no gap longer than a frame and a half. The one that was killed has two
recordings; the second must start on a keyframe.

    restream_check.py <dir> <port> [<port> ...]
"""

import json
import os
import subprocess
import sys


def frames(path):
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
         "packet=dts_time,flags", "-of", "json", path],
        capture_output=True, text=True, check=True).stdout
    return [(float(p["dts_time"]), "K" in p["flags"]) for p in json.loads(out)["packets"]]


def describe(name, f):
    gaps = [b[0] - a[0] for a, b in zip(f, f[1:])]
    worst = max(gaps) if gaps else 0.0
    keys = sum(1 for _, k in f if k)
    first = "keyframe" if f and f[0][1] else "NOT a keyframe"
    print(f"{name}: {len(f)} frames, {keys} keyframes, largest gap {worst * 1000:.0f} ms, "
          f"starts on {first}")
    return worst


def main():
    folder, ports = sys.argv[1], sys.argv[2:]
    source = frames(os.path.join(folder, "source.flv"))
    describe("publisher", source)
    ok = True
    for port in ports:
        second = os.path.join(folder, f"recv-{port}-2.flv")
        f = frames(os.path.join(folder, f"recv-{port}-1.flv"))
        worst = describe(f"receiver {port}", f)
        if os.path.exists(second):
            g = frames(second)
            describe(f"receiver {port} after it came back", g)
            ok &= bool(g) and g[0][1]
            continue
        # Every frame, and none more than a frame and a half after the last.
        whole = len(f) == len(source) and worst < 0.050 and f[0][1]
        print(f"  {'every frame arrived, no GOP lost' if whole else 'FRAMES WERE LOST'}"
              f" ({len(source) - len(f)} short of the publisher)")
        ok &= whole
    print("PASS" if ok else "FAIL")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
