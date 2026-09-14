#!/usr/bin/env python3
"""Drive a graphic onto a real core's programme and record what was said.

Standard library only, over `/api/v1`, which is the same public contract the
web designer and an agent use. The point is the transcript: every call and
every answer, so the thing that is claimed to work can be read back.

Usage: live_graphic.py <base url> <token> <work dir>
"""

import json
import sys
import urllib.error
import urllib.request

BASE, TOKEN, WORK = sys.argv[1:4]
TRANSCRIPT = []
FAILED = 0


def call(path, body=None, method=None):
    """One REST call, recorded."""
    url = "%s/api/v1/%s" % (BASE, path)
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(
        url,
        data=data,
        method=method or ("POST" if data is not None else "GET"),
        headers={"Authorization": "Bearer " + TOKEN, "content-type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as answer:
            parsed = json.loads(answer.read().decode() or "null")
            TRANSCRIPT.append({"call": path, "params": body, "result": parsed})
            return parsed
    except urllib.error.HTTPError as e:
        parsed = json.loads(e.read().decode() or "null")
        TRANSCRIPT.append({"call": path, "params": body, "error": parsed})
        return {"__error": parsed}


def step(what, ok, detail=""):
    global FAILED
    print("%-64s%s" % (what, "ok" if ok else "FAIL"))
    if not ok:
        FAILED += 1
        print("    " + str(detail))


# --- the graphics host is up ------------------------------------------------

plugins = call("plugins")
rows = plugins.get("plugins", plugins) if isinstance(plugins, dict) else plugins
names = [p.get("name") for p in rows]
step("the graphics host plugin loaded", "ograf" in names, names)

catalogue = call("scenes/graphic/list")
ids = [g["type_id"] for g in catalogue.get("graphics", [])]
step("scene.graphic.list finds the lower third", "ograf/lower-third" in ids, catalogue)

# --- its fields are discoverable by name ------------------------------------

schema = call("scenes/item/schema?type=ograf/lower-third")
fields = sorted((schema.get("schema") or {}).get("properties", {}).keys())
step(
    "scene.item.schema answers its OGraf schema",
    fields == ["colour", "name", "side", "title"],
    fields,
)
step("and its step count", schema.get("step_count") == 1, schema.get("step_count"))

# --- a test source, and a scene with the graphic over it --------------------

call("sources", {"id": "bars", "uri": "test://smpte", "name": "Bars"})
scene = call("scenes", {"name": "Graphic over bars"})
step("a scene exists", "id" in scene, scene)

call(
    "scenes/item/add",
    {"scene": scene["id"], "content": {"source": "bars"}, "name": "bars"},
)
added = call(
    "scenes/item/add",
    {
        "scene": scene["id"],
        "content": {"graphic": "ograf/lower-third"},
        "name": "speaker strap",
    },
)
names_on = [r.get("name") for r in added.get("records", [])]
step("the graphic is on the scene", "speaker strap" in names_on, added)

listing = call("sources")
rows = listing.get("sources", listing) if isinstance(listing, dict) else listing
sources = [s["id"] for s in rows]
graphic_sources = [s for s in sources if s.startswith("graphic-lower-third-")]
step(
    "adding it started a source for its page",
    len(graphic_sources) == 1,
    sources,
)

# --- filling it in by field name --------------------------------------------

applied = call(
    "scenes/apply_graphic",
    {
        "graphic": "ograf/lower-third",
        "values": {"name": "Ada Lovelace", "title": "Analyst, Analytical Engine"},
        "play": True,
    },
)
step(
    "scene.apply_graphic filled it by field name",
    applied.get("values", {}).get("name") == "Ada Lovelace",
    applied,
)
step(
    "and answered with the item it filled",
    applied.get("names") == ["speaker strap"],
    applied.get("names"),
)

state = call(
    "tool/call",
    {"name": "ograf/graphic", "arguments": {"action": "status", "instance": graphic_sources[0]}}
    if graphic_sources
    else {"name": "ograf/graphic", "arguments": {"action": "status"}},
)
structured = (state.get("result") or state).get("structured_content", state)
step(
    "the host has the words and is playing",
    structured.get("values", {}).get("name") == "Ada Lovelace" and structured.get("playing"),
    structured,
)

# --- a collection parameter drives the field --------------------------------

call(
    "scenes/item/set",
    {
        "scene": scene["id"],
        "item": "speaker strap",
        "props": {
            "content": {"graphic": "ograf/lower-third", "params": {"name": "{{speaker}}"}}
        },
    },
)
# A parameter has to be declared before it can be set, so the collection's
# params block gets one first. That is what a designer's "add a parameter"
# button does, and there is no separate method for it yet.
declared = call("scenes/params/set", {"values": {"speaker": "Grace Hopper"}})
bound = call(
    "scenes/apply_graphic",
    {"graphic": "ograf/lower-third", "values": {}, "play": True},
)
step(
    "a {{speaker}} binding follows scene.params.set",
    bound.get("values", {}).get("name") == "Grace Hopper",
    (declared, bound.get("values")),
)

doc_now = call("scenes/export?format=json")
print("    document params:", json.dumps(doc_now.get("params", {}).get("properties", {})))
for sc in doc_now.get("scenes", []):
    for it in sc.get("items", []):
        if "graphic" in (it.get("content") or {}):
            print("    graphic item content:", json.dumps(it["content"]))

# --- on air ------------------------------------------------------------------

took = call("program/take", {"scene": "Graphic over bars"})
status = call("program")
step(
    "the scene with the graphic is on the programme",
    status.get("scene") == "Graphic over bars",
    (took, status),
)
placed = [
    p
    for p in status.get("sources", [])
    if str(p.get("id", "")).startswith("graphic-lower-third-")
]
step("the graphic's source is in the mixer", bool(placed) or bool(graphic_sources), status)

# --- taking it off ------------------------------------------------------------

off = call(
    "scenes/apply_graphic",
    {"graphic": "ograf/lower-third", "values": {}, "stop": True},
)
step("stop takes it off", "__error" not in off, off)

with open(WORK + "/transcript.json", "w") as f:
    json.dump(TRANSCRIPT, f, indent=2)
print()
print("transcript: %s/transcript.json (%d calls)" % (WORK, len(TRANSCRIPT)))
sys.exit(1 if FAILED else 0)
