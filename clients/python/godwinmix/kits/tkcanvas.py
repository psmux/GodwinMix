"""The canvas kit's painter, for a `tkinter.Canvas`.

Every number in here comes out of :mod:`godwinmix.kits.canvas`. This file only
calls `create_rectangle`, `create_line` and `create_oval`, and turns a press, a
drag and a release into :func:`~godwinmix.kits.canvas.apply_drag` calls. Keep
the split: the moment a coordinate is worked out here rather than there, the
Tkinter designer and the web composer start putting a handle in two places.

    painter = DesignerCanvas(widget, on_drag=send, on_drop=settle)
    painter.set_scene({"width": 1920, "height": 1080}, items)
    painter.draw()

It draws over whatever the preview transport put behind it: MJPEG, a still, or
nothing at all. It never draws the picture itself.
"""

from __future__ import annotations

import tkinter as tk
from typing import Any, Callable, Dict, List, Optional

from .canvas import (
    View,
    apply_drag,
    gizmos_for,
    handles_for,
    hit_test,
    inside,
    safe_areas,
    snap_delta,
    snap_targets,
)

#: Everything this class draws carries this tag, so one delete clears it.
TAG = "gmx-overlay"

#: How big a handle is on screen, in screen pixels, and how near counts as a hit.
HANDLE = 8
GRIP = 12

COLOURS = {
    "box": "#9aa0a6",
    "select": "#3b82f6",
    "guide": "#f59e0b",
    "safe": "#6b7280",
    "handle": "#ffffff",
}


class DesignerCanvas:
    """Item outlines, the selection, its handles, snap guides and the safe areas.

    `on_drag(item, props, box)` is called for every move of the pointer: send
    the props with a sequence number from :class:`~godwinmix.kits.protocol.Prediction`.
    `on_drop(item)` is called once when the button comes back up, which is where
    a caller closes its undo group. `on_select(item)` is called when the
    selection changes, item being None for a click on nothing.
    """

    def __init__(
        self,
        widget: tk.Canvas,
        on_drag: Optional[Callable[[str, Dict[str, Any], Optional[Dict[str, float]]], None]] = None,
        on_drop: Optional[Callable[[Optional[str]], None]] = None,
        on_select: Optional[Callable[[Optional[str]], None]] = None,
    ):
        self.widget = widget
        self.on_drag = on_drag
        self.on_drop = on_drop
        self.on_select = on_select
        self.view = View({"width": 1920, "height": 1080}, {"width": 1, "height": 1})
        #: Bottom of the stack first, each one {"id", "box", "transform", "label", "designer"}.
        self.items: List[Dict[str, Any]] = []
        self.selected: Optional[str] = None
        self.handles: List[Dict[str, Any]] = []
        self.guides: List[Dict[str, Any]] = []
        self.show_safe = True
        self.snapping = True
        self._drag: Optional[Dict[str, Any]] = None

        widget.bind("<Configure>", lambda _e: self.draw())
        widget.bind("<ButtonPress-1>", self._press)
        widget.bind("<B1-Motion>", self._motion)
        widget.bind("<ButtonRelease-1>", self._release)

    # ------------------------------------------------------------------ state

    def set_scene(self, canvas: Dict[str, Any], items: List[Dict[str, Any]]) -> None:
        """The document as it stands. Call it after a view or a patch lands."""
        self.view.canvas = canvas or self.view.canvas
        self.items = list(items or [])
        if self.selected and not self._item(self.selected):
            self.selected = None
        self._drag = None

    def select(self, item_id: Optional[str]) -> None:
        if item_id == self.selected:
            return
        self.selected = item_id
        if self.on_select:
            self.on_select(item_id)

    def _item(self, item_id: Optional[str]) -> Optional[Dict[str, Any]]:
        for entry in self.items:
            if entry["id"] == item_id:
                return entry
        return None

    # ---------------------------------------------------------------- drawing

    def draw(self) -> None:
        """One pass over the whole overlay."""
        widget = self.widget
        widget.delete(TAG)
        surface = {"width": max(1, widget.winfo_width()), "height": max(1, widget.winfo_height())}
        self.view.set(self.view.canvas, surface)

        if self.show_safe:
            self._draw_safe()
        for entry in self.items:
            self._draw_box(entry, entry["id"] == self.selected)
        for guide in self.guides:
            self._draw_guide(guide)
        self._draw_handles()

    def _draw_safe(self) -> None:
        areas = safe_areas(self.view.canvas)
        for name in ("action", "title"):
            box = self.view.box_to_surface(areas[name])
            self.widget.create_rectangle(
                box["x"],
                box["y"],
                box["x"] + box["width"],
                box["y"] + box["height"],
                outline=COLOURS["safe"],
                dash=(6, 5),
                tags=TAG,
            )

    def _draw_box(self, entry: Dict[str, Any], selected: bool) -> None:
        box = self.view.box_to_surface(entry["box"])
        self.widget.create_rectangle(
            box["x"],
            box["y"],
            box["x"] + box["width"],
            box["y"] + box["height"],
            outline=COLOURS["select"] if selected else COLOURS["box"],
            width=2 if selected else 1,
            tags=TAG,
        )

    def _draw_guide(self, guide: Dict[str, Any]) -> None:
        low, high = guide["span"]
        if guide["axis"] == "v":
            x0, y0 = self.view.to_surface(guide["at"], low)
            x1, y1 = self.view.to_surface(guide["at"], high)
        else:
            x0, y0 = self.view.to_surface(low, guide["at"])
            x1, y1 = self.view.to_surface(high, guide["at"])
        self.widget.create_line(x0, y0, x1, y1, fill=COLOURS["guide"], dash=(3, 3), tags=TAG)

    def _draw_handles(self) -> None:
        """The handles of the selected item, as the plugin's manifest declared them."""
        entry = self._item(self.selected)
        if entry is None:
            self.handles = []
            return
        self.handles = handles_for(entry["box"], gizmos_for(entry.get("designer")))
        half = HANDLE / 2
        for handle in self.handles:
            if handle["kind"] == "cage":
                continue
            x, y = self.view.to_surface(handle["x"], handle["y"])
            if handle["kind"] in ("rotate", "dial"):
                self.widget.create_oval(
                    x - half,
                    y - half,
                    x + half,
                    y + half,
                    fill=COLOURS["handle"],
                    outline=COLOURS["select"],
                    tags=TAG,
                )
            else:
                self.widget.create_rectangle(
                    x - half,
                    y - half,
                    x + half,
                    y + half,
                    fill=COLOURS["handle"],
                    outline=COLOURS["select"],
                    tags=TAG,
                )
        self._draw_rotate_stem(entry)

    def _draw_rotate_stem(self, entry: Dict[str, Any]) -> None:
        """The line from the top edge up to the rotate handle, so it reads as one."""
        for handle in self.handles:
            if handle["kind"] != "rotate":
                continue
            box = entry["box"]
            top_x = box["x"] + box["width"] / 2
            x0, y0 = self.view.to_surface(top_x, box["y"])
            x1, y1 = self.view.to_surface(handle["x"], handle["y"])
            self.widget.create_line(x0, y0, x1, y1, fill=COLOURS["select"], tags=TAG)

    # ---------------------------------------------------------------- pointer

    def _press(self, event: tk.Event) -> None:
        x, y = self.view.to_canvas(event.x, event.y)
        handle = hit_test(self.handles, x, y, self.view.length_from_surface(GRIP))
        entry = self._item(self.selected) if handle else self._topmost(x, y)
        if entry is None:
            self.select(None)
            self.handles = []
            self.draw()
            return
        if handle is None:
            self.select(entry["id"])
            self.draw()
            # No handle under the pointer means the body of the item: the cage,
            # which is the handle that moves it.
            handle = handles_for(entry["box"], gizmos_for(entry.get("designer")))[0]
        self._drag = {
            "item": entry["id"],
            "handle": handle,
            "start": {"box": dict(entry["box"]), "transform": entry.get("transform") or {}},
            "from": (x, y),
            "targets": self._targets(entry["id"]),
        }

    def _topmost(self, x: float, y: float) -> Optional[Dict[str, Any]]:
        """The item under a point. The list is bottom first, so the last wins."""
        found = None
        for entry in self.items:
            if inside(entry["box"], x, y):
                found = entry
        return found

    def _targets(self, item_id: str) -> Dict[str, List[Dict[str, Any]]]:
        boxes = [e["box"] for e in self.items if e["id"] != item_id]
        return snap_targets({"canvas": self.view.canvas, "boxes": boxes})

    def _motion(self, event: tk.Event) -> None:
        if not self._drag:
            return
        x, y = self.view.to_canvas(event.x, event.y)
        start_x, start_y = self._drag["from"]
        dx = x - start_x
        dy = y - start_y
        mods = self._mods(event, x, y)
        result = apply_drag(self._drag["handle"], self._drag["start"], dx, dy, mods)
        result = self._snapped(result, dx, dy, mods)
        entry = self._item(self._drag["item"])
        if entry is not None and result["box"]:
            entry["box"] = result["box"]
        self.draw()
        if self.on_drag:
            self.on_drag(self._drag["item"], result["props"], result["box"])

    def _snapped(self, result: Dict[str, Any], dx: float, dy: float, mods: Dict[str, Any]) -> Dict[str, Any]:
        """Line the moved box up, then redo the drag with the nudge folded in.

        Only a move snaps. A scale that snapped would fight the aspect modifier,
        and a dial has no box to line up with anything.
        """
        if not result["box"] or self._drag["handle"].get("action") != "move":
            self.guides = []
            return result
        off = (not self.snapping) or bool(mods.get("invert"))
        delta = snap_delta(result["box"], self._drag["targets"], {"invert": off})
        self.guides = delta["guides"]
        if delta["dx"] or delta["dy"]:
            return apply_drag(self._drag["handle"], self._drag["start"], dx + delta["dx"], dy + delta["dy"], mods)
        return result

    def _mods(self, event: tk.Event, x: float, y: float) -> Dict[str, Any]:
        """What the keyboard is saying, plus where the pointer is for a rotation.

        The state bits are not the same on every platform: bit 0 is Shift
        everywhere, bit 2 is Control, and bit 3 is Alt on X11 and Option on a
        Mac. Anything finer than that belongs to the application, not the kit.
        """
        state = int(getattr(event, "state", 0) or 0)
        return {
            "aspect": bool(state & 0x0001),
            "centre": bool(state & 0x0008),
            "invert": bool(state & 0x0004),
            "pointer": {"x": x, "y": y},
        }

    def _release(self, _event: tk.Event) -> None:
        item = self._drag["item"] if self._drag else None
        self._drag = None
        self.guides = []
        self.draw()
        if item and self.on_drop:
            self.on_drop(item)
