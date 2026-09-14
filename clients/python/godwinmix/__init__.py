"""A client for the GodwinMix control protocol.

The same contract the first party UI uses: one WebSocket at `/rpc`, JSON-RPC
over it, `core.subscribe` to say what you want, a snapshot then deltas, and
`event/flush` to say when to repaint.

    import asyncio, godwinmix

    async def main():
        client = await godwinmix.connect("http://127.0.0.1:8080", token="…")
        await client.subscribe(ext={"tally": True})
        await client.settled()
        for source in client.state["sources"]:
            print(source["id"], source["state"], client.tally_of(source["id"]))
        await client.take("cam1")
        await client.close()

    asyncio.run(main())

Nothing but the standard library at runtime: `asyncio`, a WebSocket client
written out in `godwinmix._ws`, and `urllib` for pictures. The typed methods,
the types and the event list in `godwinmix._generated` come from
`protocol.json` by way of `clients/gen/generate.py`.

`godwinmix.tk` is imported separately, because a headless script has no reason
to load `tkinter`.

The major version of this package is the `api_level` it speaks. A core is safe
when its `core.info` reports `api_compatible <= API_LEVEL <= api_level`.
"""

from ._generated import API_COMPATIBLE, API_LEVEL, EVENT_NAMES, EXT_KEYS, METHODS
from .client import UI_EVENTS, Client, connect
from .errors import CODES, ConnectionClosed, RpcError
from .frames import HEADER_BYTES, Frame, cell_for, parse_frame, sheet_width_for
from .schema import Field, Form, apply_conditions, describe_form, missing, read_form
from .store import Store, empty_state
from .video import fetch_snapshot, poll_snapshots, preview_stream, read_mjpeg

__version__ = "1.0.0"

__all__ = [
    "API_COMPATIBLE",
    "API_LEVEL",
    "CODES",
    "EVENT_NAMES",
    "EXT_KEYS",
    "HEADER_BYTES",
    "METHODS",
    "UI_EVENTS",
    "Client",
    "ConnectionClosed",
    "Field",
    "Form",
    "Frame",
    "RpcError",
    "Store",
    "__version__",
    "apply_conditions",
    "cell_for",
    "connect",
    "describe_form",
    "empty_state",
    "fetch_snapshot",
    "missing",
    "parse_frame",
    "poll_snapshots",
    "preview_stream",
    "read_form",
    "read_mjpeg",
    "sheet_width_for",
    "urls",
]

from . import urls  # noqa: E402  (listed in __all__ above, imported last to avoid a cycle)
