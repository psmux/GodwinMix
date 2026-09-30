#!/usr/bin/env python3
"""Simulated viewers for an HLS output, and what they cost the core.

Each viewer is a thread that behaves like a player: it reads the
multivariant playlist, takes one rung (spread over the ladder), and then
either follows it with LL-HLS blocking reloads, fetching every new part, or
polls the playlist and fetches every new segment. Meanwhile the core's CPU
and resident memory are sampled with `ps`.

    dev/hls-viewers.py 'http://127.0.0.1:8080/hls/viewers/master.m3u8?key=...' \\
        --viewers 50 --seconds 60 --pid "$(pgrep -f 'godwinmix -c')"

Standard library only, so it runs anywhere Python 3.8 does.
"""

import argparse
import re
import statistics
import subprocess
import threading
import time
import urllib.parse
import urllib.request

lock = threading.Lock()
totals = {"requests": 0, "bytes": 0, "errors": 0, "waits": [], "media": []}


def get(url, timeout=15):
    started = time.monotonic()
    with urllib.request.urlopen(url, timeout=timeout) as r:
        body = r.read()
    return body, time.monotonic() - started


def count(nbytes, took=None, media=False, error=False):
    with lock:
        totals["requests"] += 1
        totals["bytes"] += nbytes
        totals["errors"] += int(error)
        if took is not None:
            (totals["media"] if media else totals["waits"]).append(took)


def uris(text):
    """What a player fetches from a playlist: the init segment, then the parts
    when the playlist has them (LL-HLS) or the segments when it does not."""
    maps = re.findall(r'#EXT-X-MAP:URI="([^"]+)"', text)
    parts = re.findall(r'#EXT-X-PART:.*?URI="([^"]+)"', text)
    if parts:
        return maps + parts
    return maps + [l for l in text.splitlines() if l and not l.startswith("#")]


def live_edge(found):
    """A player joins near the live edge: the init, then the last few."""
    return set(found[:-3]) - {u for u in found if "init" in u}


def viewer(master, n, stop):
    try:
        text, _ = get(master)
    except Exception:
        count(0, error=True)
        return
    variants = [l for l in text.decode().splitlines() if l and not l.startswith("#")]
    rung = urllib.parse.urljoin(master, variants[n % len(variants)])
    seen = None
    hint = None
    while not stop.is_set():
        url = rung
        if hint:
            url += ("&" if "?" in url else "?") + f"_HLS_msn={hint[0]}&_HLS_part={hint[1]}"
        try:
            body, took = get(url)
            count(len(body), took)
        except Exception:
            count(0, error=True)
            time.sleep(1)
            hint = None
            continue
        text = body.decode()
        found = uris(text)
        if seen is None:
            seen = live_edge(found)
        for u in found:
            if u in seen:
                continue
            seen.add(u)
            try:
                data, took = get(urllib.parse.urljoin(rung, u))
                count(len(data), took, media=True)
            except Exception:
                count(0, error=True)
        m = re.search(r'#EXT-X-PRELOAD-HINT:TYPE=PART,URI="(\d+)\.(\d+)\.m4s', text)
        if m:
            hint = (int(m.group(1)), int(m.group(2)))
        else:
            time.sleep(1)


def cpu_seconds(pid):
    """The process's CPU time so far, from `ps -o time=` (`[[dd-]hh:]mm:ss.ss`)."""
    text = subprocess.run(["ps", "-o", "time=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
    total = 0.0
    for part in text.replace("-", ":").split(":"):
        total = total * 60 + float(part)
    return total


def sample(pid, stop, out):
    """Resident memory once a second."""
    while not stop.is_set():
        try:
            rss = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True).stdout
            out.append(int(rss) / 1024)
        except Exception:
            pass
        stop.wait(1)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("master")
    ap.add_argument("--viewers", type=int, default=50)
    ap.add_argument("--seconds", type=int, default=60)
    ap.add_argument("--pid", type=int)
    a = ap.parse_args()
    stop = threading.Event()
    samples = []
    if a.pid:
        threading.Thread(target=sample, args=(a.pid, stop, samples), daemon=True).start()
        cpu0, t0 = cpu_seconds(a.pid), time.monotonic()
    threads = [threading.Thread(target=viewer, args=(a.master, i, stop), daemon=True) for i in range(a.viewers)]
    for t in threads:
        t.start()
        time.sleep(0.02)
    time.sleep(a.seconds)
    if a.pid:
        cpu = 100 * (cpu_seconds(a.pid) - cpu0) / (time.monotonic() - t0)
    stop.set()
    for t in threads:
        t.join(timeout=20)
    media = sorted(totals["media"]) or [0]
    print(f"viewers {a.viewers} for {a.seconds} s: {totals['requests']} requests, "
          f"{totals['bytes'] / 1e6:.1f} MB, {totals['errors']} errors, "
          f"{totals['bytes'] * 8 / a.seconds / 1e6:.1f} Mbit/s out")
    print(f"media fetch ms: median {statistics.median(media) * 1000:.1f}, "
          f"p95 {media[int(len(media) * 0.95)] * 1000:.1f}, max {media[-1] * 1000:.1f}")
    if a.pid and samples:
        print(f"core cpu {cpu:.1f}% of one core over the run (from its CPU time); "
              f"rss MiB: start {samples[0]:.0f}, end {samples[-1]:.0f}, max {max(samples):.0f}")


if __name__ == "__main__":
    main()
