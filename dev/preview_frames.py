#!/usr/bin/env python3
"""Prove that an armed scene reaches a client as preview frames on /rpc.

The defect this answers: the connection held a preview subscription and never
read it, so `ext.preview` bought a compositor and no picture. Nothing but a
running core can show that the frames now arrive, so this arms a scene over the
REST API, subscribes over the WebSocket, and reads the binary frames back.

Preview frames and mosaic frames share one socket and one sixteen byte header.
The top bit of the sequence number is the stream: clear for the mosaic, set for
the preview. That is what this splits them on.

Standard library only, like every other script here.

Usage: preview_frames.py HOST PORT TOKEN [--out DIR]

Start a core it can talk to with two test patterns for sources:

    ./target/release/godwinmix --example-config > t.toml
    # replace the [[sources]] with two of type = "test/source",
    # uri = "test://smpte" and "test://ball"
    GODWINMIX_TOKEN=testtok ./target/release/godwinmix --config t.toml \\
        --bind 127.0.0.1:18111
    dev/preview_frames.py 127.0.0.1 18111 testtok --out /tmp
"""

import base64
import json
import os
import socket
import struct
import sys
import urllib.request

HOST, PORT, TOKEN = sys.argv[1], int(sys.argv[2]), sys.argv[3]
OUT = sys.argv[sys.argv.index("--out") + 1] if "--out" in sys.argv else None
BASE = f"http://{HOST}:{PORT}"
PREVIEW_STREAM = 1 << 31
# An empty preview is the compositor drawing its own backdrop, which encodes to
# about 4.3 kB at 640 wide. Two test patterns in a two box come out at about
# 7.8 kB, so the size alone tells a real picture from an empty one and there is
# a comfortable gap to put the line in.
BLACK_CEILING = 6000
FAILED = []


def check(name, ok, detail=""):
    print(f"  {name}: {'ok' if ok else 'FAIL ' + str(detail)}")
    if not ok:
        FAILED.append(name)


def call(path, body=None):
    data = None if body is None else json.dumps(body).encode()
    r = urllib.request.Request(f"{BASE}{path}", data=data)
    r.add_header("Authorization", f"Bearer {TOKEN}")
    if data is not None:
        r.add_header("Content-Type", "application/json")
    with urllib.request.urlopen(r, timeout=15) as resp:
        return json.loads(resp.read().decode())


def ws_open(path):
    s = socket.create_connection((HOST, PORT), timeout=20)
    key = base64.b64encode(os.urandom(16)).decode()
    s.send(
        (
            f"GET {path} HTTP/1.1\r\nHost: {HOST}:{PORT}\r\n"
            f"Upgrade: websocket\r\nConnection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n"
            f"Authorization: Bearer {TOKEN}\r\n\r\n"
        ).encode()
    )
    buf = b""
    while b"\r\n\r\n" not in buf:
        part = s.recv(4096)
        if not part:
            raise RuntimeError("the socket closed during the handshake")
        buf += part
    status = buf.split(b"\r\n", 1)[0].decode()
    if "101" not in status:
        raise RuntimeError(status)
    return s, buf.split(b"\r\n\r\n", 1)[1]


def ws_send_text(s, text):
    """One masked text frame, which is the only kind a client may send."""
    payload = text.encode()
    mask = os.urandom(4)
    masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
    header = bytes([0x81])
    if len(payload) < 126:
        header += bytes([0x80 | len(payload)])
    elif len(payload) < 65536:
        header += bytes([0x80 | 126]) + struct.pack(">H", len(payload))
    else:
        header += bytes([0x80 | 127]) + struct.pack(">Q", len(payload))
    s.send(header + mask + masked)


def ws_frames(s, rest, want_binary, timeout=25):
    """Read until `want_binary` binary frames have arrived. Text comes too."""
    binaries, texts, buf = [], [], rest
    s.settimeout(timeout)
    while len(binaries) < want_binary:
        while len(buf) < 2:
            buf += s.recv(65536)
        opcode, length = buf[0] & 0x0F, buf[1] & 0x7F
        offset = 2
        if length == 126:
            while len(buf) < 4:
                buf += s.recv(65536)
            length, offset = struct.unpack(">H", buf[2:4])[0], 4
        elif length == 127:
            while len(buf) < 10:
                buf += s.recv(65536)
            length, offset = struct.unpack(">Q", buf[2:10])[0], 10
        while len(buf) < offset + length:
            buf += s.recv(65536)
        payload, buf = buf[offset:offset + length], buf[offset + length:]
        if opcode == 2:
            binaries.append(payload)
        elif opcode == 1:
            texts.append(payload.decode())
        elif opcode == 8:
            raise RuntimeError("the core closed the socket")
    return binaries, texts


def split(frame):
    """One binary message as (is_preview, seq, layout, running_ms, jpeg)."""
    seq, layout, running_ms = struct.unpack("<IIQ", frame[:16])
    return bool(seq & PREVIEW_STREAM), seq & ~PREVIEW_STREAM, layout, running_ms, frame[16:]


# --- arm a scene ------------------------------------------------------------

status = call("/api/v1/core/status")
sources = [s["id"] for s in status.get("sources", [])]
if len(sources) < 2:
    print(f"this core has {len(sources)} sources; two are needed", file=sys.stderr)
    sys.exit(2)

scene = call("/api/v1/scenes/create_from", {"sources": sources[:2], "name": "Preview probe"})
armed = call("/api/v1/scenes/preview/set", {"scene": scene["id"]})
check("a scene is armed", armed["preview"]["armed"], armed)

# --- read the frames --------------------------------------------------------

sock, rest = ws_open("/rpc")
ws_send_text(
    sock,
    json.dumps(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "core.subscribe",
            "params": {"events": ["*"], "ext": {"preview": {"fps": 8, "width": 640}}},
        }
    ),
)
try:
    binaries, texts = ws_frames(sock, rest, 12)
finally:
    sock.close()

previews = [split(f) for f in binaries]
previews = [p for p in previews if p[0]]
mosaics = [f for f in binaries if not split(f)[0]]
sizes = [len(p[4]) for p in previews]
check("preview frames arrive", len(previews) > 0, f"{len(binaries)} binary frames, none of them preview")
check(
    "every preview frame is a JPEG",
    all(p[4][:2] == b"\xff\xd8" for p in previews),
    [p[4][:4].hex() for p in previews[:3]],
)
check(
    "the preview numbers itself and names no grid",
    all(p[2] == 0 for p in previews) and [p[1] for p in previews] == list(range(1, len(previews) + 1)),
    [(p[1], p[2]) for p in previews[:5]],
)
check(
    "the picture is not black",
    bool(sizes) and min(sizes) > BLACK_CEILING,
    f"sizes={sizes}",
)
# A client that asked for the preview alone is still told about the grid, which
# is where `preview_empty` says whether a picture is coming at all.
layouts = [json.loads(t) for t in texts if '"event/multiview.layout"' in t]
check(
    "the layout says the preview is not empty",
    bool(layouts) and layouts[-1]["params"].get("preview_empty") is False,
    layouts[-1]["params"] if layouts else "no layout event",
)

print(f"  {len(previews)} preview frames, {len(mosaics)} mosaic frames, sizes {sizes}")
if OUT and previews:
    biggest = max(previews, key=lambda p: len(p[4]))
    path = os.path.join(OUT, "preview-frame.jpg")
    with open(path, "wb") as f:
        f.write(biggest[4])
    print(f"  wrote {path} ({len(biggest[4])} bytes)")

sys.exit(1 if FAILED else 0)
