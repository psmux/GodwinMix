"""The connection: one WebSocket, JSON-RPC over it, a store beside it."""

from __future__ import annotations

import asyncio
import json
from typing import Any, Callable, Dict, Iterable, List, Optional

from . import urls
from ._generated import GeneratedMethods
from ._ws import WebSocket, WebSocketError
from .errors import ConnectionClosed, RpcError
from .frames import parse_frame
from .store import Store

#: Every event a UI normally wants. Patterns match the part after `event/`,
#: and `*` matches one or more characters, so "source.*" covers
#: `event/source.state`. Expensive streams are not in here: ask through `ext`.
UI_EVENTS = [
    "snapshot",
    "program.*",
    "preview.*",
    "source.*",
    "output.*",
    "media.*",
    "adbreak.*",
    "alert",
    "resync",
    "flush",
]


class Client(GeneratedMethods):
    """A connection to one core.

    Made by :func:`godwinmix.connect`. Every typed method on it comes from
    `protocol.json` by way of `clients/gen/generate.py`; the connection, the
    store and the frame decoding are hand written.
    """

    def __init__(self, base: str, token: Optional[str] = None, timeout: float = 10.0):
        self.base = base
        self.token = token
        self.timeout = timeout
        self.store = Store()
        self.ws: Optional[WebSocket] = None
        self._reader: Optional[asyncio.Task] = None
        self._pending: Dict[int, asyncio.Future] = {}
        self._next_id = 1
        self._handlers: Dict[str, List[Callable[..., Any]]] = {}
        self._flushed = asyncio.Event()
        self._closed = False

    # ------------------------------------------------------------- lifecycle

    async def open(self) -> None:
        self.ws = await WebSocket.connect(urls.rpc(self.base, self.token), timeout=self.timeout)
        self.store.patch({"connected": True})
        self._reader = asyncio.ensure_future(self._read_loop())

    async def close(self) -> None:
        """Close the connection. The core's mosaic teardown waits for this."""
        self._closed = True
        if self.ws is not None:
            await self.ws.close()
        if self._reader is not None:
            self._reader.cancel()
            try:
                await self._reader
            except asyncio.CancelledError:
                pass
            except Exception:  # noqa: B902
                pass
        self._fail_pending(ConnectionClosed("the client closed the connection"))
        self.store.patch({"connected": False})

    async def __aenter__(self) -> "Client":
        return self

    async def __aexit__(self, *exc: Any) -> None:
        await self.close()

    @property
    def state(self) -> Dict[str, Any]:
        """The state as it stands. Read this at a flush, not per event."""
        return self.store.state

    # ----------------------------------------------------------------- calls

    async def call(self, method: str, params: Optional[Dict[str, Any]] = None) -> Any:
        """Send one call and wait for its answer.

        Every typed method goes through here, so every refusal has the one
        error shape. Use it directly for a method a plugin added that this
        api_level has never heard of.
        """
        if self.ws is None or self._closed:
            raise ConnectionClosed("not connected to the mixer. Call connect() first.")
        call_id = self._next_id
        self._next_id += 1
        waiting: asyncio.Future = asyncio.get_running_loop().create_future()
        self._pending[call_id] = waiting
        body = {"jsonrpc": "2.0", "id": call_id, "method": method, "params": params or {}}
        try:
            await self.ws.send_text(json.dumps(body))
        except WebSocketError as e:
            self._pending.pop(call_id, None)
            raise ConnectionClosed(str(e)) from e
        try:
            return await asyncio.wait_for(waiting, self.timeout)
        except asyncio.TimeoutError as e:
            self._pending.pop(call_id, None)
            raise RpcError(
                -32001,
                f"{method} did not answer within {self.timeout:g} seconds. The core may be busy; try again.",
                {"retryable": True},
            ) from e

    async def _call(self, method: str, params: Dict[str, Any]) -> Any:
        """What the generated methods call. Named with an underscore so that
        `call` stays the one a person types."""
        return await self.call(method, params)

    # ------------------------------------------------------------- subscribe

    async def subscribe(
        self,
        events: Optional[Iterable[str]] = None,
        ext: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Ask for events, and for the expensive streams this surface wants.

        Nothing runs on the core unless a client asks: no mosaic, no meters, no
        tally. `ext={"tally": True}` is a whole tally lamp panel's worth of
        subscription.
        """
        spec = {"events": list(events if events is not None else UI_EVENTS), "ext": ext or {}}
        return await self.call("core.subscribe", spec)

    async def settled(self, timeout: float = 10.0) -> Dict[str, Any]:
        """Wait until the core has flushed at least once, then answer the state.

        What a script wants after subscribing: the snapshot has landed and the
        first batch of deltas with it.
        """
        if self.store.state["seq"] == 0 or not self._flushed.is_set():
            try:
                await asyncio.wait_for(self._flushed.wait(), timeout)
            except asyncio.TimeoutError:
                raise RpcError(
                    -32001,
                    f"no event/flush in {timeout:g} seconds. Did this client subscribe?",
                    {"retryable": True},
                ) from None
        return self.store.state

    # ---------------------------------------------------------------- events

    def on(self, name: str, handler: Callable[..., Any]) -> Callable[[], None]:
        """Listen for "flush", "event", "frame", "alert", "close" or one event name.

        Returns a function that removes the handler. A handler may be a
        coroutine function; it is then scheduled rather than awaited, so a slow
        listener never holds up the read loop.
        """
        self._handlers.setdefault(name, []).append(handler)

        def off() -> None:
            handlers = self._handlers.get(name) or []
            if handler in handlers:
                handlers.remove(handler)

        return off

    def on_flush(self, handler: Callable[[Dict[str, Any]], Any]) -> Callable[[], None]:
        """Repaint here, not per event. Called once per batch that changed something."""
        return self.on("flush", handler)

    def _emit(self, name: str, *args: Any) -> None:
        for handler in list(self._handlers.get(name) or []):
            try:
                result = handler(*args)
                if asyncio.iscoroutine(result):
                    asyncio.ensure_future(result)
            except Exception as e:  # noqa: B902
                # A surface that throws while drawing must not take the
                # connection down with it.
                print(f"godwinmix: a handler for {name} raised {e!r}")

    # ------------------------------------------------------------- read loop

    async def _read_loop(self) -> None:
        assert self.ws is not None
        try:
            while not self._closed:
                opcode, payload = await self.ws.recv()
                if opcode == 0x2:
                    frame = parse_frame(payload)
                    if frame is not None:
                        self._emit("frame", frame)
                    continue
                self._handle_text(payload)
        except (WebSocketError, asyncio.CancelledError) as e:
            self._fail_pending(ConnectionClosed(str(e) or "the connection closed"))
            self.store.patch({"connected": False})
            self._emit("close")

    def _handle_text(self, payload: bytes) -> None:
        try:
            message = json.loads(payload)
        except ValueError:
            return
        if not isinstance(message, dict):
            return
        call_id = message.get("id")
        if call_id is not None and call_id in self._pending:
            waiting = self._pending.pop(call_id)
            if waiting.done():
                return
            error = message.get("error")
            if error:
                waiting.set_exception(
                    RpcError(error.get("code", -32603), error.get("message", ""), error.get("data") or {})
                )
            else:
                waiting.set_result(message.get("result", {}))
            return
        method = message.get("method") or ""
        if not method.startswith("event/"):
            return
        name = method[len("event/") :]
        params = message.get("params") or {}
        flushed = self.store.apply(name, params)
        self._emit("event", name, params)
        # Every event also reaches a listener under its own name, except
        # "flush": that one is the render tick and carries the state, not the
        # event's params, so a listener is not handed two different shapes.
        if name != "flush":
            self._emit(name, params)
        if flushed:
            self._flushed.set()
            if self.store.take_dirty():
                self._emit("flush", self.store.state)

    def _fail_pending(self, error: RpcError) -> None:
        for waiting in list(self._pending.values()):
            if not waiting.done():
                waiting.set_exception(error)
        self._pending.clear()

    # ----------------------------------------------------------- convenience

    async def take(self, source: Optional[str] = None) -> Dict[str, Any]:
        """Put a source on programme. None cuts to the slate."""
        return await self.program_take(source=source)

    async def sources(self) -> List[Dict[str, Any]]:
        """Every source, from the store when it is current and the core when it is not."""
        if self.store.state["sources"]:
            return self.store.state["sources"]
        return await self.source_list()

    def tally_of(self, source_id: str) -> str:
        return self.store.tally_of(source_id)

    # ------------------------------------------------------------------ urls

    def snapshot_url(self, name: str, width: Optional[int] = None) -> str:
        return urls.snapshot(self.base, name, width, self.token)

    def mjpeg_url(self, name: str, width: Optional[int] = None, fps: Optional[int] = None) -> str:
        return urls.mjpeg(self.base, name, width, fps, self.token)

    def whep_url(self, name: str) -> str:
        return urls.whep(self.base, name, self.token)


async def connect(base: str, token: Optional[str] = None, timeout: float = 10.0) -> Client:
    """Connect to a core and start reading its stream.

    ``base`` is the address of the mixer, ``http://host:8080`` or ``https://…``;
    ``ws://`` and ``wss://`` are accepted too. Nothing is subscribed yet: call
    :meth:`Client.subscribe` to say what you want.

    >>> client = await godwinmix.connect("http://127.0.0.1:8080", token)
    >>> await client.subscribe(ext={"tally": True})
    >>> await client.settled()
    >>> await client.take("cam1")
    """
    client = Client(base, token, timeout)
    await client.open()
    return client
