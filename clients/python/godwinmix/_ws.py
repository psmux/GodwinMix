"""Enough of RFC 6455 to talk to the core, with nothing but the standard library.

A WebSocket client is about two hundred lines: a handshake, masked frames out,
unmasked frames in, and a pong for every ping. Pulling in `websockets` or
`aiohttp` for that would make this package impossible to drop onto a Raspberry
Pi with no index, and a Tkinter panel is exactly the kind of thing that gets
dropped onto a Raspberry Pi with no index.

Not implemented, on purpose: extensions (no permessage-deflate is offered, so
none is negotiated), subprotocols, and fragmented messages larger than memory.
A control message is never fragmented, which the RFC guarantees.
"""

from __future__ import annotations

import asyncio
import base64
import hashlib
import os
import ssl as ssl_module
import struct
from typing import Dict, Optional, Tuple
from urllib.parse import urlsplit

GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"

OP_CONT = 0x0
OP_TEXT = 0x1
OP_BINARY = 0x2
OP_CLOSE = 0x8
OP_PING = 0x9
OP_PONG = 0xA

# 8 MiB. A mosaic frame at 1920 wide is tens of kilobytes; anything near this
# is a core that has gone wrong, and reading it would be the client going wrong
# with it.
MAX_MESSAGE = 8 * 1024 * 1024


class WebSocketError(OSError):
    """The socket, the handshake or the framing. Not a refusal from the core."""


class WebSocket:
    """One open connection. Not safe to read from two tasks at once."""

    def __init__(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter):
        self._reader = reader
        self._writer = writer
        self.closed = False

    @classmethod
    async def connect(
        cls,
        url: str,
        headers: Optional[Dict[str, str]] = None,
        timeout: float = 10.0,
    ) -> "WebSocket":
        """Open `ws://` or `wss://` and complete the handshake."""
        parts = urlsplit(url)
        secure = parts.scheme == "wss"
        port = parts.port or (443 if secure else 80)
        host = parts.hostname or "127.0.0.1"
        context = ssl_module.create_default_context() if secure else None
        path = parts.path or "/"
        if parts.query:
            path += "?" + parts.query

        try:
            reader, writer = await asyncio.wait_for(
                asyncio.open_connection(host, port, ssl=context), timeout
            )
        except asyncio.TimeoutError as e:
            raise WebSocketError(f"{host}:{port} did not answer within {timeout:g} seconds") from e
        except OSError as e:
            raise WebSocketError(f"cannot reach {host}:{port}: {e}") from e

        key = base64.b64encode(os.urandom(16)).decode()
        lines = [
            f"GET {path} HTTP/1.1",
            f"Host: {parts.netloc}",
            "Upgrade: websocket",
            "Connection: Upgrade",
            f"Sec-WebSocket-Key: {key}",
            "Sec-WebSocket-Version: 13",
        ]
        for name, value in (headers or {}).items():
            lines.append(f"{name}: {value}")
        writer.write(("\r\n".join(lines) + "\r\n\r\n").encode())
        await writer.drain()

        try:
            head = await asyncio.wait_for(reader.readuntil(b"\r\n\r\n"), timeout)
        except (asyncio.IncompleteReadError, asyncio.TimeoutError) as e:
            writer.close()
            raise WebSocketError("the core closed the connection during the handshake") from e

        status = head.split(b"\r\n", 1)[0].decode(errors="replace")
        if " 101" not in status:
            writer.close()
            raise WebSocketError(f"no upgrade: {status.strip()}")
        expected = base64.b64encode(hashlib.sha1((key + GUID).encode()).digest()).decode()
        if expected.lower().encode() not in head.lower():
            writer.close()
            raise WebSocketError("the server's Sec-WebSocket-Accept did not match")
        return cls(reader, writer)

    # ------------------------------------------------------------------ write

    async def send_text(self, text: str) -> None:
        await self._send(OP_TEXT, text.encode())

    async def send_binary(self, payload: bytes) -> None:
        await self._send(OP_BINARY, payload)

    async def _send(self, opcode: int, payload: bytes) -> None:
        if self.closed:
            raise WebSocketError("the connection is closed")
        n = len(payload)
        if n < 126:
            header = struct.pack("!BB", 0x80 | opcode, 0x80 | n)
        elif n < 65536:
            header = struct.pack("!BBH", 0x80 | opcode, 0x80 | 126, n)
        else:
            header = struct.pack("!BBQ", 0x80 | opcode, 0x80 | 127, n)
        # Every frame from a client is masked. The RFC is not asking.
        mask = os.urandom(4)
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        try:
            self._writer.write(header + mask + masked)
            await self._writer.drain()
        except OSError as e:
            self.closed = True
            raise WebSocketError(f"the connection dropped while sending: {e}") from e

    # ------------------------------------------------------------------- read

    async def recv(self) -> Tuple[int, bytes]:
        """The next text or binary message, as (opcode, payload).

        Pings are answered here and never handed up. A close frame raises, so
        the caller's read loop ends the same way a dropped socket ends it.
        """
        payload = b""
        message_op = None
        while True:
            fin, opcode, data = await self._frame()
            if opcode == OP_CLOSE:
                self.closed = True
                raise WebSocketError("the core closed the connection")
            if opcode == OP_PING:
                await self._send(OP_PONG, data)
                continue
            if opcode == OP_PONG:
                continue
            if opcode == OP_CONT:
                if message_op is None:
                    raise WebSocketError("a continuation frame with nothing to continue")
            else:
                message_op = opcode
            payload += data
            if len(payload) > MAX_MESSAGE:
                raise WebSocketError(f"a message over {MAX_MESSAGE} bytes; giving up")
            if fin:
                return message_op or OP_TEXT, payload

    async def _frame(self) -> Tuple[bool, int, bytes]:
        head = await self._read(2)
        fin = bool(head[0] & 0x80)
        opcode = head[0] & 0x0F
        masked = bool(head[1] & 0x80)
        length = head[1] & 0x7F
        if length == 126:
            length = struct.unpack("!H", await self._read(2))[0]
        elif length == 127:
            length = struct.unpack("!Q", await self._read(8))[0]
        if masked:
            mask = await self._read(4)
            body = await self._read(length)
            body = bytes(b ^ mask[i % 4] for i, b in enumerate(body))
        else:
            body = await self._read(length)
        return fin, opcode, body

    async def _read(self, n: int) -> bytes:
        if n == 0:
            return b""
        try:
            return await self._reader.readexactly(n)
        except (asyncio.IncompleteReadError, ConnectionResetError) as e:
            self.closed = True
            raise WebSocketError("the connection to the mixer dropped") from e

    # ------------------------------------------------------------------ close

    async def close(self) -> None:
        if self.closed:
            return
        self.closed = True
        try:
            await self._send_close()
        except (WebSocketError, OSError):
            pass
        try:
            self._writer.close()
            await self._writer.wait_closed()
        except (OSError, asyncio.CancelledError):
            pass

    async def _send_close(self) -> None:
        # 1000: a normal, deliberate close. The core's multiview teardown is
        # waiting for exactly this.
        self.closed = False
        try:
            await self._send(OP_CLOSE, struct.pack("!H", 1000))
        finally:
            self.closed = True
