#!/usr/bin/env python3
"""One /rpc session, with nothing but the standard library.

Opens the WebSocket by hand, subscribes with ext.multiview, and checks the
four things 05 section 2 promises: event/snapshot first, a layout, a flush at
the end of the batch, and binary frames carrying the 16 byte header with the
layout id the client was sent.

Usage: smoke_rpc.py <host> <port> <token>
"""

import base64
import json
import os
import socket
import struct
import sys

HEADER_BYTES = 16


def handshake(sock, host, port, token):
    key = base64.b64encode(os.urandom(16)).decode()
    request = (
        f"GET /rpc?token={token} HTTP/1.1\r\n"
        f"Host: {host}:{port}\r\n"
        "Upgrade: websocket\r\n"
        "Connection: Upgrade\r\n"
        f"Sec-WebSocket-Key: {key}\r\n"
        "Sec-WebSocket-Version: 13\r\n\r\n"
    )
    sock.sendall(request.encode())
    buf = b""
    while b"\r\n\r\n" not in buf:
        chunk = sock.recv(4096)
        if not chunk:
            raise SystemExit("the core closed the connection during the handshake")
        buf += chunk
    head, rest = buf.split(b"\r\n\r\n", 1)
    if b"101" not in head.split(b"\r\n")[0]:
        raise SystemExit("no upgrade: " + head.decode(errors="replace").splitlines()[0])
    return rest


def send_text(sock, payload):
    data = payload.encode()
    mask = os.urandom(4)
    masked = bytes(b ^ mask[i % 4] for i, b in enumerate(data))
    n = len(data)
    if n < 126:
        header = struct.pack("!BB", 0x81, 0x80 | n)
    elif n < 65536:
        header = struct.pack("!BBH", 0x81, 0x80 | 126, n)
    else:
        header = struct.pack("!BBQ", 0x81, 0x80 | 127, n)
    sock.sendall(header + mask + masked)


class Reader:
    """Enough of RFC 6455 to read unmasked server frames."""

    def __init__(self, sock, buf=b""):
        self.sock = sock
        self.buf = buf

    def take(self, n):
        while len(self.buf) < n:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise SystemExit("the core closed the connection")
            self.buf += chunk
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def frame(self):
        b0, b1 = self.take(2)
        opcode = b0 & 0x0F
        length = b1 & 0x7F
        if length == 126:
            length = struct.unpack("!H", self.take(2))[0]
        elif length == 127:
            length = struct.unpack("!Q", self.take(8))[0]
        if b1 & 0x80:
            mask = self.take(4)
            body = bytes(b ^ mask[i % 4] for i, b in enumerate(self.take(length)))
        else:
            body = self.take(length)
        return opcode, body


def main():
    host, port, token = sys.argv[1], int(sys.argv[2]), sys.argv[3]
    sock = socket.create_connection((host, port), timeout=20)
    sock.settimeout(20)
    reader = Reader(sock, handshake(sock, host, port, token))

    send_text(
        sock,
        json.dumps(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "core.subscribe",
                "params": {"ext": {"multiview": {"fps": 4, "width": 640}, "tally": True}},
            }
        ),
    )

    saw = {"result": False, "snapshot": False, "flush": False}
    layouts = set()
    frames = []
    # Enough messages for the subscribe answer, the snapshot, a layout, a
    # flush and several mosaic frames at 4 fps.
    for _ in range(200):
        opcode, body = reader.frame()
        if opcode == 0x8:
            raise SystemExit("the core closed the socket early")
        if opcode == 0x2:
            if len(body) < HEADER_BYTES:
                raise SystemExit(f"a binary frame was only {len(body)} bytes")
            seq, layout, running_ms = struct.unpack("<IIQ", body[:HEADER_BYTES])
            jpeg = body[HEADER_BYTES:]
            if jpeg[:2] != b"\xff\xd8":
                raise SystemExit("the payload after the header is not a JPEG")
            frames.append((seq, layout, running_ms, len(jpeg)))
            if len(frames) >= 2:
                break
            continue
        if opcode != 0x1:
            continue
        message = json.loads(body)
        if message.get("id") == 1:
            result = message.get("result") or {}
            if "seq" not in result:
                raise SystemExit(f"core.subscribe answered {message}")
            if result.get("ignored_ext"):
                raise SystemExit(f"the core ignored ext keys: {result['ignored_ext']}")
            saw["result"] = True
            continue
        method = message.get("method", "")
        if method == "event/snapshot":
            saw["snapshot"] = True
        elif method == "event/multiview.layout":
            layouts.add(message["params"]["id"])
        elif method == "event/flush":
            saw["flush"] = True

    for name in ("result", "snapshot", "flush"):
        if not saw[name]:
            raise SystemExit(f"never saw {name}")
    if not frames:
        raise SystemExit("no mosaic frame arrived, so the pipeline was never built")
    if not layouts:
        raise SystemExit("no event/multiview.layout, so a frame's layout id means nothing")
    for seq, layout, running_ms, size in frames:
        if layout not in layouts:
            raise SystemExit(f"frame {seq} carries layout {layout}, none of {sorted(layouts)}")
    if frames[1][0] <= frames[0][0]:
        raise SystemExit("the frame counter did not advance")

    # Go away, which is what the mosaic's teardown is waiting for.
    sock.sendall(struct.pack("!BB", 0x88, 0x80) + os.urandom(4))
    sock.close()
    print(f"snapshot, flush, layouts {sorted(layouts)}, {len(frames)} frames: {frames}")


if __name__ == "__main__":
    main()
