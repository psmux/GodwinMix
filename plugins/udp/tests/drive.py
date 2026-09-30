#!/usr/bin/env python3
"""Stand in for the core and run udp/source for a while, then report.

Starts the staged plugin binary the way the core does (the GMX_ environment,
the handshake on stdin and stderr), starts the source, and every few seconds
asks it for `stats` and reads its CPU and memory with `ps`. The MPEG-TS it
hands over on stdout goes to a file or to /dev/null.

    tests/drive.py --uri udp://@239.1.1.1:19471 --seconds 60
    tests/drive.py --uri udp://@239.1.1.1:19471 --program 2 --out got.ts

Standard library only. Prints one line per sample and a JSON summary last.
"""

import argparse
import json
import os
import subprocess
import sys
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)


def send(proc, obj):
    proc.stdin.write((json.dumps(obj) + "\n").encode())
    proc.stdin.flush()


class Answers:
    """Reads the plugin's stderr on a thread, keeping answers by id."""

    def __init__(self, proc):
        self.by_id, self.hello, self.lock = {}, None, threading.Condition()
        threading.Thread(target=self.read, args=(proc,), daemon=True).start()

    def read(self, proc):
        for line in proc.stderr:
            try:
                msg = json.loads(line)
            except ValueError:
                continue
            with self.lock:
                if msg.get("method") == "initialize":
                    self.hello = msg
                elif "id" in msg and "method" not in msg:
                    self.by_id[msg["id"]] = msg
                self.lock.notify_all()

    def wait(self, pred, timeout=10):
        end = time.time() + timeout
        with self.lock:
            while not pred() and time.time() < end:
                self.lock.wait(0.1)
            return pred()


def call(proc, answers, ident, method, params=None):
    send(proc, {"jsonrpc": "2.0", "id": ident, "method": method, "params": params or {}})
    answers.wait(lambda: ident in answers.by_id)
    return answers.by_id.get(ident, {})


def cpu_rss(pid):
    out = subprocess.run(["ps", "-o", "%cpu=,rss=", "-p", str(pid)], capture_output=True, text=True).stdout.split()
    return (float(out[0]), int(out[1]) // 1024) if len(out) == 2 else (0.0, 0)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--uri", required=True)
    ap.add_argument("--program", type=int, default=0)
    ap.add_argument("--seconds", type=int, default=60)
    ap.add_argument("--every", type=int, default=5)
    ap.add_argument("--out", default=os.devnull)
    ap.add_argument("--binary", default=os.path.join(ROOT, "bin", "gmx-udp"))
    a = ap.parse_args()

    env = dict(os.environ, GMX_PLUGIN="udp", GMX_PROVIDE="source", GMX_INSTANCE="drive", GMX_PLUGIN_ROOT=ROOT)
    out = open(a.out, "wb")
    proc = subprocess.Popen([a.binary], env=env, stdin=subprocess.PIPE, stdout=out, stderr=subprocess.PIPE)
    answers = Answers(proc)
    if not answers.wait(lambda: answers.hello is not None):
        sys.exit("the plugin did not say initialize within 10 s")
    params = {"uri": a.uri, "program": a.program}
    canvas = {"width": 1920, "height": 1080, "fps": 30}
    send(proc, {"jsonrpc": "2.0", "id": 0, "result": {
        "core": "drive.py", "version": "0", "api_level": 1, "api_compatible": 1, "canvas": canvas,
        "transport": "container", "media": "", "instance": "drive", "provide": "source", "params": params}})
    started = call(proc, answers, 1, "start", {"canvas": canvas, "transport": "container", "media": ""})
    if "error" in started:
        sys.exit(f"start was refused: {started['error']}")
    samples, ident, t0 = [], 10, time.time()
    while time.time() - t0 < a.seconds:
        time.sleep(a.every)
        ident += 1
        stats = call(proc, answers, ident, "stats").get("result", {}).get("stats") or {}
        ident += 1
        health = call(proc, answers, ident, "health").get("result", {})
        cpu, rss = cpu_rss(proc.pid)
        samples.append({"t": round(time.time() - t0), "cpu": cpu, "rss_mb": rss, **stats})
        print(f"{samples[-1]['t']:>4}s cpu {cpu:5.1f}% rss {rss} MB  {health.get('state')}: {health.get('detail')}", flush=True)
    ident += 1
    programs = call(proc, answers, ident, "programs").get("result")
    call(proc, answers, ident + 1, "stop")
    call(proc, answers, ident + 2, "shutdown", {"reason": "drive.py is done"})
    proc.wait(timeout=10)
    last = samples[-1] if samples else {}
    print(json.dumps({
        "seconds": a.seconds, "uri": a.uri,
        "cpu_percent_mean": round(sum(s["cpu"] for s in samples) / max(len(samples), 1), 2),
        "rss_mb_max": max((s["rss_mb"] for s in samples), default=0),
        "datagrams": last.get("datagrams"), "bytes_in": last.get("bytes_in"),
        "ts_packets_lost": last.get("ts_packets_lost"), "rtp_packets_lost": last.get("rtp_packets_lost"),
        "null_packets_dropped": last.get("null_packets_dropped"), "resumed": last.get("resumed"),
        "programs": programs,
    }, indent=1))


if __name__ == "__main__":
    main()
