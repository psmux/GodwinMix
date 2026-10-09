"""`gmx tool` the way pi runs it, for dev/agents_e2e.py.

pi has no MCP: it reads the skills and runs `godwinmix tool NAME 'JSON'` in
its shell. These are the same calls the MCP half makes, through that door.
"""

import json
import os
import re
import subprocess
import time


def retry_after(p):
    """The wait a retryable refusal asks for, in milliseconds, or None."""
    if p.returncode == 0:
        return None
    out = p.stdout + p.stderr
    if '"retryable":true' not in out:
        return None
    m = re.search(r'"retry_after_ms":(\d+)', out)
    return int(m.group(1)) if m else None


def cli_steps(step, gmx, url, token, run):
    env = dict(os.environ, GODWINMIX_URL=url, GODWINMIX_TOKEN=token)

    def tool(name, args=None, ok=True):
        cmd = [gmx, "tool", name] + ([json.dumps(args)] if args is not None else [])
        p = subprocess.run(cmd, env=env, capture_output=True, text=True, encoding="utf-8", timeout=60)
        wait = retry_after(p) if ok else None
        if wait is not None:
            # The flash guard refuses a take that lands within 360 ms of a cut
            # that changed the brightness sharply, and the MCP half has just
            # cut to the wide shot. The refusal says how long to wait; an agent
            # that does what it says gets the take, so the run does the same.
            time.sleep(wait / 1000 + 0.05)
            p = subprocess.run(cmd, env=env, capture_output=True, text=True, encoding="utf-8", timeout=60)
        if (p.returncode == 0) != ok:
            raise AssertionError(f"gmx tool {name} exited {p.returncode}: {p.stdout[-400:]} {p.stderr[-400:]}")
        return p

    def listed():
        out = tool("list").stdout
        for want in ["agent_state", "add_source", "call_tool", "search_tools"]:
            assert want in out, f"gmx tool list has no {want}"

    def state():
        doc = json.loads(tool("agent_state").stdout)
        assert any(s["id"] == "cam1" for s in doc["sources"]), doc

    def a_graphic():
        tool("add_source", {"id": f"strap-{run}", "uri": "template:headline-strap",
                            "params": {"fields": {"topic": "WEATHER", "headline": "Storm warning tonight"}}})
        scene = f"Cli {run}"
        tool("create_scene_from", {"sources": ["cam1"], "name": scene})
        tool("add_scene_item", {"scene": scene, "content": {"source": f"strap-{run}"}, "name": "strap"})
        tool("take", {"scene": scene})
        doc = json.loads(tool("agent_state").stdout)
        assert doc.get("scene") == scene, doc

    def through_call_tool():
        out = tool("call_tool", {"name": "list_scenes", "arguments": {}}).stdout
        assert f"Cli {run}" in out, out[:400]

    def a_refusal_exits_one_and_stays_quiet():
        p = tool("take", {"source": "no-such-camera"}, ok=False)
        assert "cam1" in p.stdout, f"the refusal should name the ids that exist: {p.stdout}"
        assert '"level"' not in p.stderr, f"gmx tool printed log lines on stderr: {p.stderr[:300]}"

    for name, fn in [
        ("gmx tool: list", listed),
        ("gmx tool: agent_state", state),
        ("gmx tool: a headline strap on a new scene, taken", a_graphic),
        ("gmx tool: call_tool", through_call_tool),
        ("gmx tool: a refusal exits 1, with no log noise", a_refusal_exits_one_and_stays_quiet),
    ]:
        step(name, fn)
