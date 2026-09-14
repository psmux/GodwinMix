"""Multiview frames off the wire.

A binary frame on `/rpc` is a 16 byte header then JPEG::

    offset 0   u32  seq               little endian
    offset 4   u32  layout id         little endian
    offset 8   u64  running time ms   little endian
    offset 16  ...  JPEG bytes

The layout id matches the id in `event/multiview.layout`, which is what lets a
client cut cells out of a sheet without a race when the layout changes mid
flight. The core's writer is `api::rpc::frame_header` in the Rust source, and
the third field is milliseconds there, so it is milliseconds here.
"""

from __future__ import annotations

import struct
from typing import Any, Dict, NamedTuple, Optional

HEADER_BYTES = 16
_HEADER = struct.Struct("<IIQ")


class Frame(NamedTuple):
    """One mosaic frame, header read and JPEG still compressed."""

    seq: int
    layout: int
    running_time_ms: int
    jpeg: bytes


def parse_frame(data: bytes) -> Optional[Frame]:
    """Split one binary message into its header and its JPEG.

    Answers None for anything too short to be a frame, which is what a client
    gets from a core that sends bare JPEGs.
    """
    if len(data) <= HEADER_BYTES:
        return None
    seq, layout, running_time_ms = _HEADER.unpack_from(data, 0)
    return Frame(seq, layout, running_time_ms, bytes(data[HEADER_BYTES:]))


def cell_for(frame: Frame, layout: Optional[Dict[str, Any]], source: str) -> Optional[Dict[str, Any]]:
    """The cell rectangle for one source, in the layout this frame names.

    None when the frame belongs to a different layout, which happens for a
    frame or two after the grid changes. Cutting the old rectangle out of the
    new sheet is how a UI ends up showing the wrong camera.
    """
    if not layout or layout.get("id") != frame.layout:
        return None
    for cell in layout.get("cells") or []:
        if cell.get("source") == source:
            return cell
    return None


def sheet_width_for(tile_px: int, cols: int) -> int:
    """The mosaic width to ask the core for.

    The sheet holds `cols` cells across. A tile that is `tile_px` real pixels
    wide wants `tile_px * cols`, rounded up to a multiple of 16 because encoders
    like even macroblocks, and clamped to what the protocol allows. Asking for
    more than this is bytes nobody looks at.
    """
    sheet = max(1, tile_px) * max(1, cols)
    rounded = -(-sheet // 16) * 16
    return max(320, min(1920, rounded))
