#!/usr/bin/env python3
"""One frame off /mjpeg, ten frames off /pcm/program, and a WHEP offer.

Called by dev/smoke.sh. Standard library only, like every other step. Prints one
line per check and exits non zero if any of them failed, so the shell script
only has to look at the exit code.

Usage: smoke_streams.py HOST PORT TOKEN
"""

import base64
import os
import socket
import struct
import sys
import urllib.error
import urllib.request

HOST, PORT, TOKEN = sys.argv[1], int(sys.argv[2]), sys.argv[3]
BASE = f"http://{HOST}:{PORT}"
FAILED = []


def check(name, ok, detail=""):
    print(f"  {name}: {'ok' if ok else 'FAIL ' + str(detail)}")
    if not ok:
        FAILED.append(name)


def authed(path):
    r = urllib.request.Request(f"{BASE}{path}")
    r.add_header("Authorization", f"Bearer {TOKEN}")
    return r


def one_mjpeg_frame(path):
    """Read until the first complete JPEG, then hang up."""
    resp = urllib.request.urlopen(authed(path), timeout=15)
    ctype = resp.headers.get("content-type", "")
    if "multipart/x-mixed-replace" not in ctype:
        resp.close()
        raise RuntimeError(f"content-type is {ctype!r}")
    buf = b""
    while len(buf) < 4_000_000:
        chunk = resp.read(4096)
        if not chunk:
            break
        buf += chunk
        if b"\xff\xd8" in buf and b"\xff\xd9" in buf[buf.index(b"\xff\xd8"):]:
            break
    resp.close()
    start = buf.find(b"\xff\xd8")
    if start < 0:
        raise RuntimeError(f"no JPEG in {len(buf)} bytes")
    return buf[start:]


def ws_open(path):
    s = socket.create_connection((HOST, PORT), timeout=15)
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


def ws_binary(s, rest, count):
    """The next `count` binary frames, unmasked as a server sends them."""
    frames, buf = [], rest
    s.settimeout(15)
    while len(frames) < count:
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
            frames.append(payload)
    return frames


# --- MJPEG ------------------------------------------------------------------

for path in ("/mjpeg/sheet", "/mjpeg/program"):
    try:
        jpeg = one_mjpeg_frame(path)
        check(f"MJPEG {path}", jpeg[:2] == b"\xff\xd8", f"{jpeg[:4].hex()}")
    except Exception as e:  # noqa: BLE001 - a smoke test reports whatever went wrong
        check(f"MJPEG {path}", False, e)

try:
    urllib.request.urlopen(f"{BASE}/mjpeg/sheet", timeout=10)
    check("MJPEG without a token is refused", False, "it let us in")
except urllib.error.HTTPError as e:
    check("MJPEG without a token is refused", e.code == 401, f"HTTP {e.code}")
except Exception as e:  # noqa: BLE001
    check("MJPEG without a token is refused", False, e)

# --- PCM --------------------------------------------------------------------

try:
    sock, rest = ws_open("/pcm/program")
    frames = ws_binary(sock, rest, 10)
    sock.close()
    heads = [struct.unpack("<IIQ", f[:16]) for f in frames]
    seqs = [h[0] for h in heads]
    times = [h[2] for h in heads]
    sizes = {len(f) for f in frames}
    # 16 byte header plus 480 samples of stereo F32LE.
    right_size = sizes == {16 + 3840}
    in_order = seqs == list(range(seqs[0], seqs[0] + 10))
    monotonic = all(b > a for a, b in zip(times, times[1:]))
    check("PCM /pcm/program: ten frames", right_size and in_order and monotonic,
          f"sizes={sizes} seq0={seqs[0]} monotonic={monotonic}")
except Exception as e:  # noqa: BLE001
    check("PCM /pcm/program: ten frames", False, e)

# --- WHEP -------------------------------------------------------------------
#
# Either an SDP answer where whepserversink is installed, or a 501 naming the
# package where it is not. Both are a pass; a 500 or a hang is not.

try:
    info = urllib.request.urlopen(authed("/api/v1/core/info"), timeout=10).read().decode()
    has_whep = '"whep"' in info
except Exception:  # noqa: BLE001
    has_whep = False

try:
    r = urllib.request.Request(f"{BASE}/whep/program", data=b"v=0\r\n", method="POST")
    r.add_header("Authorization", f"Bearer {TOKEN}")
    r.add_header("Content-Type", "application/sdp")
    resp = urllib.request.urlopen(r, timeout=10)
    body = resp.read().decode()
    check("WHEP POST /whep/program", resp.status in (200, 201), f"HTTP {resp.status}")
except urllib.error.HTTPError as e:
    body = e.read().decode()
    named = "mjpeg" in body.lower() or "install" in body.lower()
    check(
        "WHEP POST /whep/program says what to do"
        + (" (element present)" if has_whep else " (element absent)"),
        e.code == 501 and named,
        f"HTTP {e.code}: {body[:140]}",
    )
except Exception as e:  # noqa: BLE001
    check("WHEP POST /whep/program", False, e)

# --- nothing left running ---------------------------------------------------

try:
    body = urllib.request.urlopen(authed("/metrics"), timeout=10).read().decode()
    lines = [l for l in body.splitlines() if l.startswith("gmx_stream_clients")]
    open_now = [l for l in lines if not l.endswith(" 0")]
    check("every stream closed after the checks", not open_now, open_now)
    check("the encoder is not running with no output",
          "gmx_encoder_running 0" in body,
          [l for l in body.splitlines() if "gmx_encoder_running" in l])
except Exception as e:  # noqa: BLE001
    check("metrics after the checks", False, e)

sys.exit(1 if FAILED else 0)
