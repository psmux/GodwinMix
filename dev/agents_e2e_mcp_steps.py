"""The MCP half of dev/agents_e2e.py: the calls Claude Code and opencode made
for a lower third, a ticker, a cut and "what is on air"."""

import time


def mcp_steps(step, mcp, RUN):
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
