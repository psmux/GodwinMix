#!/usr/bin/env python3
"""An agent's view of the cameras and the switcher, through `godwinmix mcp`.

Speaks MCP over stdio the way Claude Code does: finds the camera and ATEM
tools by asking in plain words, then tunes a camera, follows a person and
cuts the switcher with them. Run by dev/camera-live-checks.sh; prints one
line per step and exits non zero on the first failure.

Usage: camera-live-mcp.py <godwinmix binary> <base url> <token>
"""

import json
import subprocess
import sys

gmx, base, token = sys.argv[1:4]
proc = subprocess.Popen([gmx, "mcp", "--url", base, "--token", token], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
ids = iter(range(1, 1000))


def rpc(method, params=None):
    n = next(ids)
    proc.stdin.write(json.dumps({"jsonrpc": "2.0", "id": n, "method": method, "params": params or {}}) + "\n")
    proc.stdin.flush()
    while True:
        line = proc.stdout.readline()
        if not line:
            sys.exit("the MCP server closed its output")
        msg = json.loads(line)
        if msg.get("id") == n:
            if "error" in msg:
                sys.exit(f"{method}: {msg['error']}")
            return msg["result"]


def tool(name, args):
    r = rpc("tools/call", {"name": name, "arguments": args})
    if r.get("isError"):
        sys.exit(f"{name} {args}: {r['content']}")
    if r.get("structuredContent") is not None:
        return r["structuredContent"]
    text = "".join(c.get("text", "") for c in r.get("content", []))
    try:
        return json.loads(text)
    except ValueError:
        return {"text": text}


def check(ok, said, got=None):
    print(("ok   " if ok else "FAIL ") + said)
    if not ok:
        if got is not None:
            print("     got: " + json.dumps(got)[:600])
        sys.exit(1)


rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "camera-live", "version": "1"}})
proc.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
proc.stdin.flush()

found = json.dumps(tool("search_tools", {"query": "make a PTZ camera follow the speaker more smoothly"}))
check("gmx_ptz_track_tune" in found, "search_tools finds gmx_ptz_track_tune from plain words")
found = json.dumps(tool("search_tools", {"query": "cut the ATEM switcher"}))
check("gmx_atem_cut" in found, "search_tools finds gmx_atem_cut")


def call(name, args):
    r = tool("call_tool", {"name": name, "arguments": args})
    return r.get("structured_content", r) if isinstance(r, dict) else r


now = call("gmx_ptz_track_tune", {"camera": "cam-b"})
check("max_speed" in json.dumps(now), "an agent reads a camera's tuning")
tuned = call("gmx_ptz_track_tune", {"camera": "cam-b", "max_speed": 0.4, "max_accel": 1.2, "headroom": 0.15})
check(json.dumps(tuned).count("0.4") >= 1 and "max_accel" in json.dumps(tuned), "an agent makes it slower and smoother", tuned)
call("gmx_ptz_track_follow", {"camera": "cam-b", "target": "largest", "framing": "full"})
state = json.dumps(call("gmx_ptz_track_state", {"camera": "cam-b"}))
check('"mode": "following"' in state and '"headroom": 0.15' in state, "an agent follows a person and the tuning holds")
call("gmx_ptz_track_stop", {"camera": "cam-b"})
call("gmx_atem_mode", {"switcher": "atem-main", "mode": "own"})
call("gmx_atem_program", {"switcher": "atem-main", "me": 0, "input": 3})
pgm = call("gmx_atem_state", {"switcher": "atem-main"})
check(json.dumps(pgm).find('"program": 3') >= 0, "an agent puts input 3 on the switcher's programme")
proc.stdin.close()
proc.terminate()
