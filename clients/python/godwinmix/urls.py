"""Addresses, built the same way in all three client libraries.

A token goes in the query rather than a header because an `<img>` tag, a
`WebSocketPeer` and a WHEP player cannot set headers. GET only: a token in the
URL of a POST ends up in more logs than it should.
"""

from __future__ import annotations

from typing import Optional
from urllib.parse import quote, urlencode, urlsplit


def rpc(base: str, token: Optional[str] = None) -> str:
    """`ws://host/rpc?token=`, from an http, https, ws or wss address."""
    scheme = "wss" if _secure(base) else "ws"
    return f"{scheme}://{_netloc(base)}/rpc" + _query(token=token)


def snapshot(base: str, name: str, width: Optional[int] = None, token: Optional[str] = None) -> str:
    """`GET /api/v1/snapshot/{name}`: one JPEG. "sheet", "program" or a source id."""
    return f"{http_base(base)}/api/v1/snapshot/{quote(name, safe='')}" + _query(width=width, token=token)


def mjpeg(
    base: str,
    name: str,
    width: Optional[int] = None,
    fps: Optional[int] = None,
    token: Optional[str] = None,
) -> str:
    """`GET /mjpeg/{name}`: `multipart/x-mixed-replace`, one JPEG per part."""
    return f"{http_base(base)}/mjpeg/{quote(name, safe='')}" + _query(width=width, fps=fps, token=token)


def whep(base: str, name: str, token: Optional[str] = None) -> str:
    """`POST /whep/{name}`: the WebRTC offer endpoint, for audio and low latency."""
    return f"{http_base(base)}/whep/{quote(name, safe='')}" + _query(token=token)


def http_base(base: str) -> str:
    """The core's address as http or https, whatever scheme was handed in."""
    return ("https://" if _secure(base) else "http://") + _netloc(base)


def _secure(base: str) -> bool:
    return urlsplit(base if "//" in base else "//" + base).scheme in ("https", "wss")


def _netloc(base: str) -> str:
    parts = urlsplit(base if "//" in base else "//" + base)
    return parts.netloc or parts.path.strip("/")


def _query(**pairs) -> str:
    given = {k: v for k, v in pairs.items() if v is not None and v != ""}
    return "?" + urlencode(given) if given else ""
