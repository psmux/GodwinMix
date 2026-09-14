"""The state store: a snapshot, then deltas, read at flush.

The core sends `event/snapshot`, then deltas, then `event/flush`. A surface
reads from here and never re-reads the wire, and it repaints at flush and not
per event, which is why a batch of twenty changes is one repaint.

The state is plain dicts, the same shapes the protocol describes, because that
is what a Tkinter callback, a Jinja template and a print statement all want.
The typed names for those shapes are in `godwinmix._generated`.
"""

from __future__ import annotations

from typing import Any, Dict, List, Optional

MAX_ALERTS = 50


def empty_state() -> Dict[str, Any]:
    """The shape the store starts in, so a surface never guards for missing keys."""
    return {
        "program": None,
        # The preview scene, once a core sends `event/preview.changed`.
        "preview": None,
        "sources": [],
        "outputs": [],
        "multiview": {"enabled": False, "width": 0, "height": 0, "cols": 0, "rows": 0, "cells": [], "fps": 0},
        "uptime_secs": 0,
        "running_time_ms": 0,
        "backend": None,
        "ad": None,
        "tally": {},
        "meters": {"program": [], "sources": {}},
        "alerts": [],
        "layout": None,
        "seq": 0,
        "connected": False,
    }


class Store:
    """Everything a surface draws from, and the fold that keeps it current."""

    def __init__(self) -> None:
        self.state: Dict[str, Any] = empty_state()
        self._dirty = False

    # ------------------------------------------------------------ the fold

    def apply(self, name: str, params: Dict[str, Any]) -> bool:
        """Fold one event in. True when this event ended a batch."""
        state = self.state
        if name == "snapshot":
            self.snapshot(params.get("state") or {}, params.get("seq"))
        elif name == "flush":
            state["seq"] = params.get("seq", state["seq"])
            self._dirty = True
            return True
        elif name == "program.took":
            state["program"] = params.get("source") or params.get("scene")
            self._dirty = True
        elif name == "preview.changed":
            state["preview"] = params.get("scene")
            self._dirty = True
        elif name == "source.state":
            self.patch_source(params.get("source"), {"state": params.get("state"), "detail": params.get("detail")})
        elif name == "output.state":
            self.patch_output(
                params.get("output"), {"state": params.get("state"), "reconnects": params.get("reconnects")}
            )
        elif name == "adbreak.changed":
            state["ad"] = params.get("ad")
            self._dirty = True
        elif name == "meters":
            self.set_meters(params.get("program"), params.get("sources"))
        elif name == "tally":
            state["tally"] = params.get("sources") or {}
            self._dirty = True
        elif name == "multiview.layout":
            state["layout"] = params
            self._dirty = True
        elif name == "alert":
            self.add_alert(params)
        return False

    def snapshot(self, status: Dict[str, Any], seq: Optional[int] = None) -> None:
        """Replace the status document wholesale. Used by `event/snapshot`."""
        keep = {key: self.state[key] for key in ("meters", "alerts", "tally")}
        fresh = empty_state()
        fresh.update(keep)
        fresh.update(status)
        fresh["seq"] = self.state["seq"] if seq is None else seq
        fresh["connected"] = True
        self.state = fresh
        self._dirty = True

    def patch(self, fields: Dict[str, Any]) -> None:
        """Shallow merge at the top level."""
        self.state.update(fields)
        self._dirty = True

    def patch_source(self, source_id: Optional[str], fields: Dict[str, Any]) -> bool:
        """Replace one source in place, matched by id. True if it was found."""
        return self._patch_row("sources", source_id, fields)

    def patch_output(self, output_id: Optional[str], fields: Dict[str, Any]) -> bool:
        return self._patch_row("outputs", output_id, fields)

    def _patch_row(self, key: str, row_id: Optional[str], fields: Dict[str, Any]) -> bool:
        if not row_id:
            return False
        for row in self.state[key]:
            if row.get("id") == row_id:
                row.update({k: v for k, v in fields.items() if v is not None})
                self._dirty = True
                return True
        return False

    def set_meters(self, program: Optional[List[float]], sources: Optional[Dict[str, Any]]) -> None:
        """Meters arrive ten times a second and are kept out of the flush path."""
        if program:
            self.state["meters"]["program"] = program
        if sources:
            self.state["meters"]["sources"].update(sources)

    def add_alert(self, alert: Dict[str, Any]) -> None:
        self.state["alerts"].insert(0, alert)
        del self.state["alerts"][MAX_ALERTS:]
        self._dirty = True

    def take_dirty(self) -> bool:
        """True once per batch that changed something. Resets as it answers."""
        was, self._dirty = self._dirty, False
        return was

    # ----------------------------------------------------------- lookups

    def source(self, source_id: str) -> Optional[Dict[str, Any]]:
        for row in self.state["sources"]:
            if row.get("id") == source_id:
                return row
        return None

    def output(self, output_id: str) -> Optional[Dict[str, Any]]:
        for row in self.state["outputs"]:
            if row.get("id") == output_id:
                return row
        return None

    def tally_of(self, source_id: str) -> str:
        """"program", "preview" or "off" for one source.

        Answered from `event/tally` when the surface asked for it, and worked
        out from the programme otherwise, so a surface that declined the tally
        stream still colours its buttons.
        """
        tally = self.state.get("tally") or {}
        if source_id in tally:
            return tally[source_id]
        if self.state.get("program") == source_id:
            return "program"
        if self.state.get("preview") == source_id:
            return "preview"
        return "off"
