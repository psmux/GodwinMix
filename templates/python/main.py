#!/usr/bin/env python3
"""{{description}}

A GodwinMix source plugin. It draws colour bars at the canvas caps and writes
them to stdout as raw I420 frames in a streamable Matroska stream. Control is
JSON-RPC 2.0, one object per line, on stdin and stderr.

Change draw() and schemas/source.json. Nothing else here needs touching.
"""
import json
import struct
import sys
import threading
import time

API = 1
TIMECODE_SCALE_NS = 1_000_000
CLUSTER_MS = 2_000
UNKNOWN = b"\x01\xff\xff\xff\xff\xff\xff\xff"   # an EBML size that never ends
# Y, U, V for the eight standard bars, white through to black.
BARS = [(235, 128, 128), (210, 16, 146), (170, 166, 16), (145, 54, 34),
        (106, 202, 222), (81, 90, 240), (41, 240, 110), (16, 128, 128)]

MEDIA = sys.stdout.buffer
state = {"canvas": None, "params": {}, "stop": threading.Event(), "thread": None,
         "frames": 0, "cluster_ms": None, "header": False}


# --- what you change --------------------------------------------------------

def draw(canvas, params, pts_ns):
    """Return one I420 frame of exactly the right size for the canvas.

    An I420 frame is a Y plane of width*height bytes, then a U plane and a V
    plane of ceil(width/2)*ceil(height/2) each. Y is brightness, 16 is black
    and 235 is white; U and V are colour, 128 each is grey.

    This draws eight colour bars and ignores the time. Your plugin will not.
    """
    width, height = canvas["width"], canvas["height"]
    cw, ch = (width + 1) // 2, (height + 1) // 2
    count = max(1, min(8, int(params.get("bars", 8))))

    def bar(x):
        return BARS[min(x * count // width, count - 1)]

    return (bytes(bar(x)[0] for x in range(width)) * height
            + bytes(bar(x * 2)[1] for x in range(cw)) * ch
            + bytes(bar(x * 2)[2] for x in range(cw)) * ch)


# --- the control channel ----------------------------------------------------

def send(obj):
    """One JSON object on one line of stderr. stdout is media and only media."""
    sys.stderr.write(json.dumps(obj, separators=(",", ":")) + "\n")
    sys.stderr.flush()


def log(level, message):
    send({"jsonrpc": "2.0", "method": "log",
          "params": {"level": level, "message": message}})


def reply(rid, result):
    if rid is not None:
        send({"jsonrpc": "2.0", "id": rid, "result": result})


def fail(rid, code, message, data):
    send({"jsonrpc": "2.0", "id": rid,
          "error": {"code": code, "message": message, "data": data}})


# --- the container transport: streamable Matroska on stdout -----------------

def vint(value):
    """A data size, as an EBML variable length integer, in the shortest form."""
    for length in range(1, 9):
        if value < (1 << (7 * length)) - 1:
            return (value | (1 << (7 * length))).to_bytes(length, "big")
    raise ValueError("a size no Matroska stream ever has")


def uint(value):
    return value.to_bytes(8, "big").lstrip(b"\x00") or b"\x00"


def elem(element_id, payload):
    return element_id + vint(len(payload)) + payload


def write_header(canvas):
    """EBML header, an open ended Segment, Info, and one raw video track."""
    ebml = (elem(b"\x42\x82", b"matroska\x00") + elem(b"\x42\x87", uint(4))
            + elem(b"\x42\x85", uint(2)))
    info = (elem(b"\x2a\xd7\xb1", uint(TIMECODE_SCALE_NS))
            + elem(b"\x4d\x80", b"{{name}}\x00") + elem(b"\x57\x41", b"{{name}}\x00"))
    video = (elem(b"\xb0", uint(canvas["width"])) + elem(b"\xba", uint(canvas["height"]))
             + elem(b"\x9a", uint(2)) + elem(b"\x2e\xb5\x24", b"I420"))
    track = (elem(b"\xd7", uint(1)) + elem(b"\x73\xc5", uint(1)) + elem(b"\x83", uint(1))
             + elem(b"\x86", b"V_UNCOMPRESSED\x00")
             + elem(b"\x23\xe3\x83", uint(1_000_000_000 // canvas["fps"]))
             + elem(b"\xe0", video))
    MEDIA.write(elem(b"\x1a\x45\xdf\xa3", ebml) + b"\x18\x53\x80\x67" + UNKNOWN
                + elem(b"\x15\x49\xa9\x66", info)
                + elem(b"\x16\x54\xae\x6b", elem(b"\xae", track)))
    MEDIA.flush()
    state["header"] = True


def write_frame(pts_ns, data):
    """One SimpleBlock, opening a Cluster when the last one is full."""
    ms = pts_ns // TIMECODE_SCALE_NS
    base = state["cluster_ms"]
    if base is None or ms - base >= CLUSTER_MS:
        MEDIA.write(b"\x1f\x43\xb6\x75" + UNKNOWN + elem(b"\xe7", uint(ms)))
        base = state["cluster_ms"] = ms
    block = b"\x81" + struct.pack(">hB", ms - base, 0x80) + data
    MEDIA.write(b"\xa3" + vint(len(block)) + block)
    MEDIA.flush()


def produce():
    """Frames at canvas fps, PTS on our own clock starting at zero.

    The deadline comes from the frame count, not from adding a sleep each time,
    so one slow draw does not push every later frame back.
    """
    canvas = state["canvas"]
    step_ns = 1_000_000_000 // canvas["fps"]
    started, index = time.monotonic(), 0
    expected = canvas["width"] * canvas["height"] * 3 // 2
    while not state["stop"].is_set():
        pts = index * step_ns
        wait = started + pts / 1e9 - time.monotonic()
        if wait > 0 and state["stop"].wait(wait):
            break
        frame = draw(canvas, state["params"], pts)
        if len(frame) != expected:
            log("error", "draw() returned %d bytes, the canvas needs %d"
                % (len(frame), expected))
            return
        try:
            write_frame(pts, frame)
        except (BrokenPipeError, ValueError):
            log("error", "the media pipe closed")
            return
        state["frames"] += 1
        index += 1


# --- the methods the core calls ---------------------------------------------

def on_start(rid, params):
    transport = params.get("transport", "container")
    if transport != "container":
        return fail(rid, -32602,
                    "this plugin only speaks the container transport. Declare "
                    "transports = [\"container\"] in gmx-plugin.toml, which is the default.",
                    {"transport": transport, "retryable": False})
    state["canvas"] = params.get("canvas") or state["canvas"]
    if not state["header"]:
        write_header(state["canvas"])
    state["stop"].clear()
    state["thread"] = threading.Thread(target=produce, daemon=True)
    state["thread"].start()
    reply(rid, {"latency_ms": 0})


def on_stop(rid):
    state["stop"].set()
    if state["thread"] is not None:
        state["thread"].join(timeout=2)
    state["thread"] = None
    reply(rid, {})


def dispatch(message):
    """Answer one call. Returns False when the process should exit."""
    rid, method = message.get("id"), message.get("method")
    params = message.get("params") or {}
    if method == "start":
        on_start(rid, params)
    elif method == "stop":
        on_stop(rid)
    elif method == "configure":
        # The full validated object, not a diff. draw() reads it next frame.
        state["params"] = params.get("params", params)
        reply(rid, {"applied": True})
    elif method == "health":
        thread = state["thread"]
        ok = thread is None or thread.is_alive()
        reply(rid, {"state": "ok" if ok else "failing",
                    "detail": "%d frames sent" % state["frames"], "latency_ms": 0})
    elif method == "shutdown":
        on_stop(None)
        reply(rid, {})
        return False
    elif method in ("keyframe", "initialized"):
        reply(rid, {})
    else:
        fail(rid, -32601,
             "this plugin has no method '%s'. It implements start, stop, configure, "
             "health and shutdown." % method,
             {"method": method, "retryable": False})
    return True


def main():
    provides = [{"kind": "source", "id": "source", "transports": ["container"],
                 "media": {"video": "raw", "audio": "none", "alpha": False, "thumb": True},
                 "capabilities": ["restart-in-place", "health"], "latency_ms": 0,
                 "settings": "schemas/source.json", "skill": "skills/source/SKILL.md"}]
    send({"jsonrpc": "2.0", "id": 0, "method": "initialize",
          "params": {"plugin": "{{name}}", "version": "0.1.0", "api": API,
                     "transports": ["container"], "provides": provides}})
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except ValueError:
            log("info", "ignored a line that was not JSON: %s" % line[:200])
            continue
        if message.get("id") == 0 and "result" in message:
            ready = message["result"]
            canvas = state["canvas"] = ready["canvas"]
            state["params"] = ready.get("params") or {}
            log("info", "{{name}} at %dx%d@%d as '%s'"
                % (canvas["width"], canvas["height"], canvas["fps"],
                   ready.get("instance", "?")))
            send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
        elif "method" in message and not dispatch(message):
            break
    state["stop"].set()


if __name__ == "__main__":
    main()
