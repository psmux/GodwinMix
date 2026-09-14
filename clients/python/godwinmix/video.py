"""Preview pictures for a surface with no video stack.

Three ways in, cheapest first:

===========  ==================================================================
snapshot     one JPEG on demand, for an agent, a still, or a slow poll
MJPEG        a picture a second to thirty, no audio, ten lines of code
WHEP         WebRTC, audio included, under 500 ms, and a real media stack
===========  ==================================================================

What is here is the first two, over `urllib`, because that is what a Tkinter
panel with PIL needs and it costs no dependency. WHEP wants `aiortc`, which is
a large thing to require of every reader, so this library builds the URL
(:func:`godwinmix.urls.whep`) and leaves the negotiation to a surface that
wants it.

Both readers are blocking and belong on a thread of their own. The JPEG then
reaches the toolkit the way the toolkit wants it: `after_idle` in Tkinter, a
queue anywhere else.
"""

from __future__ import annotations

import time
import urllib.error
import urllib.request
from typing import Iterator, Optional

SOI = b"\xff\xd8"
EOI = b"\xff\xd9"


def fetch_snapshot(url: str, timeout: float = 10.0) -> bytes:
    """One JPEG from `/api/v1/snapshot/...`.

    The token is already in the URL when it came from
    :meth:`godwinmix.Client.snapshot_url`, which is what an `<img>` tag needs
    and what this reuses.
    """
    with urllib.request.urlopen(url, timeout=timeout) as response:
        return response.read()


def read_mjpeg(url: str, timeout: float = 30.0) -> Iterator[bytes]:
    """Yield JPEG frames from a `multipart/x-mixed-replace` stream.

    Blocking, and meant for a thread. The generator ends when the core closes
    the stream; closing the generator closes the connection, which is what
    tells the core to stop encoding for this client.

    >>> for jpeg in read_mjpeg(client.mjpeg_url("program")):
    ...     photo = PIL.ImageTk.PhotoImage(PIL.Image.open(io.BytesIO(jpeg)))

    The parts are found by scanning for the JPEG start and end markers rather
    than by parsing the multipart boundary. That reads every core's spelling of
    the headers, including the ones that leave out Content-Length.
    """
    request = urllib.request.Request(url, headers={"Accept": "multipart/x-mixed-replace"})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        buffer = b""
        while True:
            chunk = response.read(16384)
            if not chunk:
                return
            buffer += chunk
            while True:
                start = buffer.find(SOI)
                if start < 0:
                    # Header bytes between parts. Keep the tail in case a
                    # marker straddles two reads.
                    buffer = buffer[-1:]
                    break
                end = buffer.find(EOI, start + 2)
                if end < 0:
                    buffer = buffer[start:]
                    break
                yield buffer[start : end + 2]
                buffer = buffer[end + 2 :]


def poll_snapshots(url: str, every: float = 2.0, timeout: float = 10.0) -> Iterator[bytes]:
    """Yield a JPEG every `every` seconds, for a core with no `/mjpeg` route yet.

    The same shape as :func:`read_mjpeg`, so a surface can swap one for the
    other without changing anything else. A refusal or a network hiccup is
    skipped rather than ending the loop: the next picture is two seconds away.
    """
    while True:
        started = time.monotonic()
        try:
            yield fetch_snapshot(url, timeout)
        except (urllib.error.URLError, OSError):
            pass
        left = every - (time.monotonic() - started)
        if left > 0:
            time.sleep(left)


def preview_stream(
    client,
    name: str = "program",
    width: Optional[int] = None,
    every: float = 2.0,
) -> Iterator[bytes]:
    """JPEGs of one source or the programme, by whichever route this core has.

    Tries `/mjpeg/{name}` and falls back to polling `/api/v1/snapshot/{name}`
    when the core answers 404, which every core does today: the MJPEG routes
    are specified (05 section 2) and not built yet. A surface written against
    this gets the live stream the day the core grows it, with no edit.
    """
    try:
        stream = read_mjpeg(client.mjpeg_url(name, width=width))
        first = next(stream)
    except (urllib.error.HTTPError, urllib.error.URLError, OSError, StopIteration):
        yield from poll_snapshots(client.snapshot_url(name, width=width), every)
        return
    yield first
    yield from stream
