#!/usr/bin/env python3
"""What an AI agent does to a mixer, without the model: MCP and `gmx tool`.

Usage:
  dev/agents_e2e.py --url http://127.0.0.1:8080 --token TOKEN [--gmx PATH]
  dev/agents_e2e.py --start [--gmx PATH]     start a throwaway core first

The calls are the ones Claude Code, opencode and pi made when they were asked
for a lower third, a ticker, a cut and "what is on air": read the state, make
a scene of the source on air, add a template graphic and a ticker, change the
words, look at the programme, and reach a tool outside the hot list through
`call_tool`. Then the same through `gmx tool`, the way pi runs them. Every
step prints ok or fails the run. Standard library only, so it runs on any CI
runner with Python 3.8 or newer. The sources and scenes it adds carry the
run's number in their names and are left behind, so point it at a test core.
"""

import argparse
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from agents_e2e_cli import cli_steps  # noqa: E402
from agents_e2e_core import Core  # noqa: E402
from agents_e2e_mcp import Mcp  # noqa: E402

RUN = f"e2e{os.getpid()}"
FAILED = []


def step(name, fn):
    print(f"{name:<62}", end="", flush=True)
    try:
        fn()
        print("ok")
    except Exception as e:  # noqa: BLE001 - every failure is reported the same way
        print("FAIL")
        print(f"    {e}", file=sys.stderr)
        FAILED.append(name)


def mcp_steps(mcp):
    state = {}

    def handshake():
        info = mcp.initialize()
        assert "GodwinMix" in info.get("instructions", ""), "the server's instructions do not name GodwinMix"
        names = [t["name"] for t in mcp.tools()]
        for want in ["agent_state", "take", "add_source", "add_scene_item", "set_scene_item",
                     "create_scene_from", "list_templates", "snapshot", "call_tool", "search_tools"]:
            assert want in names, f"{want} is not in the hot list: {names}"
        for t in mcp.tools():
            schema = t["inputSchema"]
            assert schema.get("type") == "object", f"{t['name']} has no object schema"
            # The MCP TypeScript SDK (opencode, and most clients) drops the
            # whole list when one property's schema is not an object.
            for prop, sub in schema.get("properties", {}).items():
                assert isinstance(sub, dict), f"{t['name']}.{prop} is {sub!r}, not an object schema"

    def on_air():
        doc = mcp.json("agent_state")
        state["program"] = doc.get("program")
        assert any(s["id"] == "cam1" for s in doc["sources"]), doc

    def a_scene_of_what_is_on_air():
        scene = f"Live {RUN}"
        mcp.json("create_scene_from", {"sources": ["cam1"], "name": scene})
        mcp.json("take", {"scene": scene})
        state["scene"] = scene
        doc = mcp.json("agent_state")
        assert doc.get("scene") == scene, f"agent_state does not name the scene on air: {doc}"

    def a_lower_third():
        templates = mcp.json("list_templates")
        assert any(t.get("name") == "news-lower-third" for t in templates["templates"]), templates
        mcp.json("add_source", {"id": f"lower-{RUN}", "uri": "template:news-lower-third",
                                "params": {"fields": {"name": "Ana Silva", "title": "Producer", "accent": "#1f6fd1"}}})
        mcp.json("add_scene_item", {"scene": state["scene"], "content": {"source": f"lower-{RUN}"},
                                    "name": "lower third", "visible": False,
                                    "enter": {"type": "slide", "edge": "left", "duration_ms": 300}})
        scene = mcp.json("set_scene_item", {"scene": state["scene"], "item": "lower third", "props": {"visible": True}})
        boxes = [g for g in scene["geometry"] if g.get("source") == f"lower-{RUN}"]
        assert boxes, f"the shown lower third is not drawn: {scene['geometry']}"
        box = boxes[0]
        assert (box["x"], box["y"], box["width"]) == (0.0, 0.0, float(scene["canvas"]["width"])), \
            f"a template with no transform should cover the canvas: {box}"

    def words_change_on_air():
        mcp.json("set_source", {"id": f"lower-{RUN}", "params": {"fields": {"title": "Executive Producer"}}})
        text, _ = mcp.call("call_tool", {"name": "template_fields", "arguments": {"id": f"lower-{RUN}"}})
        assert "Executive Producer" in text, text

    def a_ticker_sent_as_json_text():
        # Claude Code sends an object argument as JSON text; the server reads it back.
        mcp.json("add_source", {"id": f"ticker-{RUN}", "uri": "ticker:",
                                "params": '{"items": ["Storm closes coast road", "Council approves new bridge"]}'})
        mcp.json("add_scene_item", {"scene": state["scene"], "content": {"source": f"ticker-{RUN}"}, "name": "ticker",
                                    "transform": {"position": {"x": 0, "y": 980}, "frame": {"w": 1920, "h": 70}}})

    def a_look():
        time.sleep(2)
        _, images = mcp.call("snapshot", {"id": "program", "width": 320, "force": True})
        assert images and images[0]["mimeType"] == "image/jpeg", "snapshot gave no picture"

    def outside_the_hot_list():
        text, _ = mcp.call("search_tools", {"query": "apply a layout"})
        assert "apply_layout" in text and "call_tool" in text, text[:300]
        mcp.call("call_tool", {"tool": "mcp__godwinmix__get_scene", "args": {"scene": state["scene"]}})
        text, _ = mcp.call("add_scene_item", {"scene": "no such scene", "content": {"source": "cam1"}}, ok=False)
        assert "create_scene_from" in text, f"a missing scene should say how to make one: {text}"

    def back_to_a_camera():
        mcp.json("take", {"source": "cam-wide"})
        assert mcp.json("agent_state")["program"] == "cam-wide"

    for name, fn in [
        ("mcp: initialize and the hot list", handshake),
        ("mcp: agent_state", on_air),
        ("mcp: a scene of the source on air, taken", a_scene_of_what_is_on_air),
        ("mcp: a lower third over the whole canvas, shown", a_lower_third),
        ("mcp: its words changed on air", words_change_on_air),
        ("mcp: a ticker, its params sent as JSON text", a_ticker_sent_as_json_text),
        ("mcp: a look at the programme", a_look),
        ("mcp: search_tools, call_tool and a refusal", outside_the_hot_list),
        ("mcp: cut to the wide shot", back_to_a_camera),
    ]:
        step(name, fn)
    return state


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--url", default=os.environ.get("GODWINMIX_URL"))
    ap.add_argument("--token", default=os.environ.get("GODWINMIX_TOKEN", ""))
    ap.add_argument("--gmx", default=os.environ.get("GMX", "gmx"), help="the gmx or godwinmix executable")
    ap.add_argument("--start", action="store_true", help="start a throwaway core with two test cameras")
    args = ap.parse_args()
    core = None
    if args.start:
        core = Core(args.gmx).wait()
        args.url, args.token = core.url, core.token
        time.sleep(3)
    if not args.url:
        ap.error("give --url and --token, or --start")
    print(f"agents end to end against {args.url}\n")
    mcp = Mcp(args.gmx, args.url, args.token)
    try:
        mcp_steps(mcp)
    finally:
        mcp.close()
    cli_steps(step, args.gmx, args.url, args.token, RUN)
    if core:
        core.stop()
    print(f"\n{len(FAILED)} failed" if FAILED else "\nall passed")
    return 1 if FAILED else 0


if __name__ == "__main__":
    sys.exit(main())
