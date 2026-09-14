#!/usr/bin/env python3
"""Replay a recorded transcript against the plugin, with no core running.

Bytes in, bytes out. It spawns main.py, writes the `core` lines to its stdin in
order, and checks that each `plugin` line turns up on stderr, as a subset. Lines
the transcript does not mention (log notifications, media reports) are skipped
rather than failed, so adding a log line does not break the test.

`gmx plugin test --offline` will do this and more once gmx is installed. This
script is here so the template's check runs on a bare machine today.

    python3 tests/replay.py tests/transcript.jsonl
"""
import json
import os
import subprocess
import sys
import threading

TIMEOUT_S = 10


def matches(expected, actual):
    """Does `actual` contain everything `expected` asks for? "*" matches any."""
    if expected == "*":
        return True
    if isinstance(expected, dict):
        return isinstance(actual, dict) and all(
            key in actual and matches(value, actual[key])
            for key, value in expected.items())
    if isinstance(expected, list):
        return (isinstance(actual, list) and len(expected) == len(actual)
                and all(matches(e, a) for e, a in zip(expected, actual)))
    return expected == actual


def read_steps(path):
    steps = []
    for number, raw in enumerate(open(path), 1):
        line = raw.strip()
        if not line or line.startswith("#") or line.startswith("//"):
            continue
        step = json.loads(line)
        if len(step) != 1 or not ({"core", "plugin"} & set(step)):
            raise SystemExit("line %d: a step has one key, 'core' or 'plugin'." % number)
        steps.append((number, step))
    return steps


def main():
    here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    transcript = sys.argv[1] if len(sys.argv) > 1 else os.path.join(here, "tests/transcript.jsonl")
    steps = read_steps(transcript)

    plugin = subprocess.Popen(
        [sys.executable, os.path.join(here, "main.py")],
        stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
        cwd=here, env=dict(os.environ, GMX_PLUGIN="{{name}}", GMX_PROVIDE="source",
                           GMX_INSTANCE="test", GMX_API_LEVEL="1", GMX_PLUGIN_ROOT=here))

    # stderr is read on its own thread so a plugin that says nothing cannot
    # wedge the writer, and so the timeout is real.
    lines, done = [], threading.Event()

    def drain():
        for raw in plugin.stderr:
            lines.append(raw.decode("utf-8", "replace").strip())
        done.set()

    threading.Thread(target=drain, daemon=True).start()

    read_to = 0
    failures = []
    for number, step in steps:
        if "core" in step:
            try:
                plugin.stdin.write((json.dumps(step["core"]) + "\n").encode())
                plugin.stdin.flush()
            except (BrokenPipeError, ValueError):
                failures.append("line %d: the plugin closed stdin early" % number)
                break
            continue
        expected = step["plugin"]
        deadline = threading.Event()
        threading.Timer(TIMEOUT_S, deadline.set).start()
        found = False
        while not found and not deadline.is_set():
            if read_to < len(lines):
                raw = lines[read_to]
                read_to += 1
                if not raw:
                    continue
                try:
                    actual = json.loads(raw)
                except ValueError:
                    continue      # a non JSON line is a log line, not an error
                if matches(expected, actual):
                    found = True
                continue
            if done.is_set() and read_to >= len(lines):
                break
            deadline.wait(0.02)
        if not found:
            failures.append("line %d: never saw a line matching %s"
                            % (number, json.dumps(expected)))
            break

    try:
        plugin.stdin.close()
    except (BrokenPipeError, ValueError):
        pass
    try:
        plugin.wait(timeout=8)
    except subprocess.TimeoutExpired:
        plugin.kill()
        failures.append("the plugin did not exit within 8 seconds of shutdown")

    if failures:
        print("FAIL: offline transcript")
        for f in failures:
            print("  " + f)
        print("  what the plugin actually said:")
        for raw in lines[:40]:
            print("    " + raw[:200])
        return 1
    print("ok: transcript replayed, %d steps, plugin exited %s"
          % (len(steps), plugin.returncode))
    return 0


if __name__ == "__main__":
    sys.exit(main())
