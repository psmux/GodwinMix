"""A fake core: enough of RFC 6455 and of the protocol to test a client.

A real asyncio server on a real port, upgrading a real socket, so the client is
exercised over the wire rather than against a mock of its own transport. The
server writes unmasked frames, as a server must, which is what makes this a
test of the reader rather than of a matching pair of bugs.
"""

from __future__ import annotations

import asyncio
import base64
import hashlib
import json
import struct
from typing import Any, Callable, Dict, List, Optional

GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


class FakeCore:
    """Answers calls, pushes events, and remembers what it was asked."""

    def __init__(self) -> None:
        self.calls: List[Dict[str, Any]] = []
        self.answers: Dict[str, Any] = {}
        self.silent: set = set()
        self.on_subscribe: Optional[Callable[["FakeCore"], Any]] = None
        self.clients: List[asyncio.StreamWriter] = []
        self._server: Optional[asyncio.AbstractServer] = None
        self.url = ""

    async def start(self) -> "FakeCore":
        self._server = await asyncio.start_server(self._serve, "127.0.0.1", 0)
        port = self._server.sockets[0].getsockname()[1]
        self.url = f"http://127.0.0.1:{port}"
        return self

    async def stop(self) -> None:
        self.drop()
        if self._server is not None:
            self._server.close()
            await self._server.wait_closed()

    def drop(self) -> None:
        """Cut every connection, the way a core being restarted does."""
        for writer in list(self.clients):
            try:
                writer.close()
            except OSError:
                pass
        self.clients.clear()

    def answer(self, method: str, reply: Any) -> None:
        """Answer this method with this result, or `{"error": {...}}` to refuse it."""
        self.answers[method] = reply

    def silence(self, method: str) -> None:
        """Take this method and never answer it, the way a wedged core does."""
        self.silent.add(method)

    def notify(self, name: str, params: Any) -> None:
        body = json.dumps({"jsonrpc": "2.0", "method": name, "params": params})
        for writer in self.clients:
            writer.write(_frame(0x1, body.encode()))

    def frame(self, seq: int, layout: int, running_time_ms: int, jpeg: bytes) -> None:
        header = struct.pack("<IIQ", seq, layout, running_time_ms)
        for writer in self.clients:
            writer.write(_frame(0x2, header + jpeg))

    def session(self) -> None:
        """The snapshot, a delta, a layout, a picture, then the flush."""
        self.notify("event/snapshot", snapshot())
        self.notify("event/source.state", {"source": "cam2", "state": "live"})
        self.notify(
            "event/multiview.layout",
            {
                "id": 7,
                "width": 640,
                "height": 180,
                "cells": [
                    {"index": 0, "source": "cam1", "x": 0, "y": 0, "w": 320, "h": 180},
                    {"index": 1, "source": "cam2", "x": 320, "y": 0, "w": 320, "h": 180},
                ],
            },
        )
        self.frame(3, 7, 1500, b"\xff\xd8\xff\xd9")
        self.notify("event/flush", {"seq": 44})

    # ---------------------------------------------------------------- server

    async def _serve(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        head = await reader.readuntil(b"\r\n\r\n")
        key = ""
        for line in head.decode(errors="replace").split("\r\n"):
            if line.lower().startswith("sec-websocket-key:"):
                key = line.split(":", 1)[1].strip()
        accept = base64.b64encode(hashlib.sha1((key + GUID).encode()).digest()).decode()
        writer.write(
            (
                "HTTP/1.1 101 Switching Protocols\r\n"
                "Upgrade: websocket\r\nConnection: Upgrade\r\n"
                f"Sec-WebSocket-Accept: {accept}\r\n\r\n"
            ).encode()
        )
        await writer.drain()
        self.clients.append(writer)
        try:
            while True:
                opcode, payload = await _read_frame(reader)
                if opcode == 0x8:
                    break
                if opcode != 0x1:
                    continue
                await self._handle(json.loads(payload), writer)
        except (asyncio.IncompleteReadError, ConnectionResetError, ValueError):
            pass
        finally:
            if writer in self.clients:
                self.clients.remove(writer)
            writer.close()

    async def _handle(self, call: Dict[str, Any], writer: asyncio.StreamWriter) -> None:
        method = call.get("method") or ""
        self.calls.append({"method": method, "params": call.get("params") or {}, "id": call.get("id")})
        if method in self.silent:
            return
        if method in self.answers:
            reply = self.answers[method]
            if callable(reply):
                reply = reply(call.get("params") or {})
        else:
            reply = {
                "error": {
                    "code": -32601,
                    "message": f"this fake core has no {method}. Register one with core.answer().",
                }
            }
        if call.get("id") is not None:
            body: Dict[str, Any] = {"jsonrpc": "2.0", "id": call["id"]}
            if isinstance(reply, dict) and "error" in reply:
                body["error"] = reply["error"]
            else:
                body["result"] = reply if reply is not None else {}
            writer.write(_frame(0x1, json.dumps(body).encode()))
        if method == "core.subscribe" and self.on_subscribe is not None:
            self.on_subscribe(self)


def _frame(opcode: int, payload: bytes) -> bytes:
    """One server to client frame: never masked, never fragmented."""
    n = len(payload)
    if n < 126:
        header = struct.pack("!BB", 0x80 | opcode, n)
    elif n < 65536:
        header = struct.pack("!BBH", 0x80 | opcode, 126, n)
    else:
        header = struct.pack("!BBQ", 0x80 | opcode, 127, n)
    return header + payload


async def _read_frame(reader: asyncio.StreamReader):
    head = await reader.readexactly(2)
    opcode = head[0] & 0x0F
    masked = bool(head[1] & 0x80)
    length = head[1] & 0x7F
    if length == 126:
        length = struct.unpack("!H", await reader.readexactly(2))[0]
    elif length == 127:
        length = struct.unpack("!Q", await reader.readexactly(8))[0]
    mask = await reader.readexactly(4) if masked else b""
    body = await reader.readexactly(length) if length else b""
    if masked:
        body = bytes(b ^ mask[i % 4] for i, b in enumerate(body))
    return opcode, body


def snapshot(seq: int = 42) -> Dict[str, Any]:
    """The snapshot a test starts from: two sources, one of them on air."""
    return {
        "seq": seq,
        "state": {
            "program": "cam1",
            "running_time_ms": 1500,
            "uptime_secs": 12,
            "backend": {},
            "multiview": {"enabled": True, "cols": 2, "rows": 1, "width": 640, "height": 180, "fps": 4, "cells": []},
            "outputs": [
                {"id": "twitch", "uri_host": "live.twitch.tv", "state": "live", "reconnects": 0, "queue_secs": 0.2}
            ],
            "sources": [
                {"id": "cam1", "name": "Camera 1", "uri": "rtmp://a", "state": "live",
                 "has_video": True, "has_audio": True},
                {"id": "cam2", "name": "Camera 2", "uri": "rtmp://b", "state": "connecting",
                 "has_video": True, "has_audio": True},
            ],
        },
    }
