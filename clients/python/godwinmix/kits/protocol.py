"""The scene document as a client believes it to be, and the drag in flight.

A port of `ui/kits/protocol/mirror.js`, `predict.js` and `undo.js`. The method
names are snake_case, the way the rest of this library spells things, but the
dictionaries that come back out keep the JavaScript's own key names (`applied`,
`echo`, `gap`, `seq`, `added`, `updated`, `removed`), so a reader moving from
one file to the other is not made to translate.

Three rules, from 11 section 4, and they are most of this file:

* apply patches, never refetch. A patch is one transaction.
* suppress the echo of your own edits by `source_client`. Suppressed does not
  mean discarded: the record still lands, because the core may have clamped
  what was asked for. It means the caller is told the change is its own, so a
  drag in flight is not redrawn from underneath the hand.
* ignore record kinds and trailing fields you do not know, so a mirror written
  today survives a document written by a newer core.
"""

from __future__ import annotations

import math
from typing import Any, Dict, Iterable, List, Optional

#: The record kinds this mirror understands. Anything else is kept, not read.
KNOWN = frozenset(("scene", "item"))


def _num(value: Any, fallback: float = 0.0) -> float:
    """What `Number(x)` gives, with a NaN or an infinity treated as absent."""
    if isinstance(value, bool) or value is None:
        return fallback
    try:
        out = float(value)
    except (TypeError, ValueError):
        return fallback
    if math.isnan(out) or math.isinf(out):
        return fallback
    return out


def _order_of(record: Dict[str, Any]) -> str:
    """The fractional sort key (`order.rs`), compared as a string.

    Siblings sort lexically so an item can be moved between two others without
    renumbering everything below it.
    """
    order = record.get("order")
    return "" if order is None else str(order)


class SceneMirror:
    """The record mirror: one copy of the document, fed by views and patches."""

    def __init__(self, client_id: Optional[str] = None):
        self.client_id: Optional[str] = client_id or None
        #: id -> record, every kind, exactly as it arrived.
        self.records: Dict[str, Dict[str, Any]] = {}
        #: The highest patch sequence number applied.
        self.seq: int = 0
        #: How many records had a kind this build does not know.
        self.unknown: int = 0
        #: The canvas of the last view that carried one.
        self.canvas: Optional[Dict[str, Any]] = None

    def set_client_id(self, client_id: Optional[str]) -> None:
        self.client_id = client_id or None

    # ------------------------------------------------------------- reading

    def scenes(self) -> List[Dict[str, Any]]:
        """Every scene record, in document order."""
        found = [r for r in self.records.values() if r.get("kind") == "scene"]
        return sorted(found, key=_order_of)

    def record(self, record_id: str) -> Optional[Dict[str, Any]]:
        """One record by id, or None."""
        return self.records.get(record_id)

    def items(self, scene_id: str) -> List[Dict[str, Any]]:
        """The items of one scene, bottom of the stack first."""
        found = [r for r in self.records.values() if r.get("kind") == "item" and r.get("parent") == scene_id]
        return sorted(found, key=_order_of)

    def descendants(self, scene_id: str) -> List[Dict[str, Any]]:
        """Everything under a scene, groups and their children included."""
        out: List[Dict[str, Any]] = []

        def walk(parent: str) -> None:
            for record in self.items(parent):
                out.append(record)
                walk(record["id"])

        walk(scene_id)
        return out

    def scene_of(self, record_id: str) -> Optional[str]:
        """The scene a record belongs to, following parents up."""
        at = self.records.get(record_id)
        while at and at.get("kind") == "item" and at.get("parent"):
            up = self.records.get(at["parent"])
            if up is None:
                return at["parent"]
            at = up
        return at["id"] if at and at.get("kind") == "scene" else None

    # ------------------------------------------------------------- writing

    def apply_view(self, view: Optional[Dict[str, Any]]) -> Dict[str, List[str]]:
        """One scene as a mutating command answered with it.

        The scene's own subtree is replaced and nothing else is touched, so an
        answer about one scene never disturbs another.
        """
        if not isinstance(view, dict) or not isinstance(view.get("records"), list):
            return {"changed": []}
        keep = {r.get("id") for r in view["records"]}
        stale = [view.get("id")] + [r["id"] for r in self.descendants(view.get("id"))]
        for record_id in stale:
            # A plain drop, not `_forget`: what is dropped here is the scene
            # itself or one of its items, both kinds this build knows, so the
            # unknown count has nothing to say about it.
            if record_id not in keep:
                self.records.pop(record_id, None)
        changed: List[str] = []
        for record in view["records"]:
            self._put(record)
            changed.append(record.get("id"))
        self.canvas = view.get("canvas") or self.canvas
        return {"changed": changed}

    def reset(self, views: Optional[Iterable[Dict[str, Any]]] = None) -> None:
        """Replace the whole mirror.

        `scene.list` answers with summaries rather than records, so this takes
        whatever views a client gathered, in one go.
        """
        self.records.clear()
        self.unknown = 0
        for view in views or []:
            self.apply_view(view)

    def apply_patch(self, patch: Optional[Dict[str, Any]]) -> Dict[str, Any]:
        """One `event/scene.patch`.

        Answers with `applied`, `echo`, `gap`, `seq`, `added`, `updated` and
        `removed`, which is what a composer needs to decide whether to redraw.
        """
        empty: Dict[str, Any] = {
            "applied": False,
            "echo": False,
            "gap": False,
            "seq": self.seq,
            "added": [],
            "updated": [],
            "removed": [],
        }
        if not isinstance(patch, dict):
            return empty
        # `presence` (who is looking at what) is a scope this build does not
        # draw. Ignoring it is the forward compatible half of the rule above.
        # Nothing was applied, so the sequence number reported is the one the
        # mirror is actually at: a caller that trusted the patch's own number
        # would think it had caught up with something it never read.
        scope = patch.get("scope")
        if scope and scope != "document":
            return empty

        seq = int(_num(patch.get("seq")))
        # A patch older than what has already been applied is a duplicate from
        # a reconnect. A patch that skips numbers means events were dropped and
        # the caller wants a fresh snapshot rather than a document with a hole
        # in it.
        gap = self.seq > 0 and seq > self.seq + 1
        if 0 < seq <= self.seq:
            return empty

        added: List[str] = []
        updated: List[str] = []
        removed: List[str] = []
        for record in patch.get("added") or []:
            self._put(record)
            added.append(record.get("id"))
        for change in patch.get("updated") or []:
            after = change.get("after") if isinstance(change, dict) else None
            if not isinstance(after, dict) or not after.get("id"):
                continue
            self._put(after)
            updated.append(after["id"])
        for entry in patch.get("removed") or []:
            # A removal names the id; `removed_records` is the core's own
            # business and this mirror does not read it.
            key = entry if isinstance(entry, str) else (entry or {}).get("id")
            if not key:
                continue
            self._forget(key)
            removed.append(key)
        if seq > 0:
            self.seq = seq
        return {
            "applied": True,
            "echo": bool(self.client_id and patch.get("source_client") == self.client_id),
            "gap": gap,
            "seq": self.seq,
            "added": added,
            "updated": updated,
            "removed": removed,
        }

    # ----------------------------------------------------------- internals

    def _put(self, record: Optional[Dict[str, Any]]) -> None:
        if not isinstance(record, dict) or not record.get("id"):
            return
        had = self.records.get(record["id"])
        if had is not None and had.get("kind") not in KNOWN:
            self.unknown -= 1
        if record.get("kind") not in KNOWN:
            self.unknown += 1
        self.records[record["id"]] = record

    def _forget(self, record_id: Optional[str]) -> None:
        record = self.records.pop(record_id, None)
        if record is not None and record.get("kind") not in KNOWN:
            self.unknown -= 1


def geometry_index(view: Optional[Dict[str, Any]]) -> Dict[str, Dict[str, Any]]:
    """Geometry by item id, from the flattened list a command answers with.

    A composer draws handles off this and never recomputes layout itself, which
    is why it never disagrees with the compositor about where an item is.
    """
    out: Dict[str, Dict[str, Any]] = {}
    for box in (view or {}).get("geometry") or []:
        out[box.get("item")] = box
    return out


class Prediction:
    """Drag at input rate, truth in the core.

    A drag cannot wait for a round trip. Not because the socket is slow, a
    loopback call is tens of microseconds, but because a blocked redraw costs a
    frame: a click tolerates 100 ms and a drag tolerates about 25. So the
    client draws its own move at once, sends it with a sequence number, and the
    core echoes the last number it applied. Echoes older than the latest input
    are discarded, and the drawing snaps to the core's answer only when the
    numbers meet.

    Nothing here knows what a transform is. It holds `props` dictionaries,
    whatever they contain, which is why the same class serves a corner drag, an
    opacity dial and a plugin's own dial on `params.key_tolerance`.
    """

    def __init__(self) -> None:
        #: The last number handed out. Monotonic for the life of the client.
        self.seq: int = 0
        #: The highest number the core has said it applied.
        self.acked: int = 0
        #: item id -> {"seq": int, "props": dict} still in flight.
        self.pending: Dict[str, Dict[str, Any]] = {}

    @property
    def busy(self) -> bool:
        """True while any item has a move the core has not confirmed."""
        return bool(self.pending)

    def predict(self, item: str, props: Dict[str, Any]) -> int:
        """Draw this now, send it with the number this returns.

        A second prediction for the same item replaces the first: the newer one
        is where the hand is, and the older one is a frame nobody will ever see
        again.
        """
        self.seq += 1
        had = self.pending.get(item)
        merged = merge_props(had["props"] if had else {}, props)
        self.pending[item] = {"seq": self.seq, "props": merged}
        return self.seq

    def settle(self, seq: Any) -> List[str]:
        """The core has applied everything up to and including `seq`.

        Two things call this: the answer to `scene.item.set`, which is the echo
        on the RPC transport, and `event/scene.patch` carrying the client's own
        number back. Either is enough; both together are simply earlier.

        Answers with the items that are the core's again.
        """
        number = int(_num(seq))
        if number <= self.acked:
            return []
        self.acked = number
        done = [item for item, held in self.pending.items() if held["seq"] <= number]
        for item in done:
            del self.pending[item]
        return done

    def reset(self) -> None:
        """Throw away every prediction, for a cancelled drag or a resync."""
        self.pending.clear()

    def resolve(self, item: str, server_props: Optional[Dict[str, Any]]) -> Optional[Dict[str, Any]]:
        """What to draw for one item.

        Our own move while it is in flight, the core's record once it has
        caught up.
        """
        held = self.pending.get(item)
        if held is None:
            return server_props
        return merge_props(server_props or {}, held["props"])

    def accepts(self, item: str, echo_seq: Any) -> bool:
        """Should an incoming change be drawn over this item?

        No while a newer move is still held for it: that is the echo of
        something the operator has already dragged past, and drawing it is the
        rubber band this class exists to remove.
        """
        held = self.pending.get(item)
        if held is None:
            return True
        return int(_num(echo_seq)) >= held["seq"]


def echo_seq_of(patch: Optional[Dict[str, Any]]) -> int:
    """The sequence number a patch is echoing back.

    `client_seq` is the name this kit asks for. A core that spells it
    differently is read anyway rather than ignored: getting the number late
    costs a snap, getting it never costs a stuck prediction.
    """
    if not isinstance(patch, dict):
        return 0
    for key in ("client_seq", "echo_seq", "seq_echo"):
        if patch.get(key) is not None:
            return int(_num(patch[key]))
    return 0


def merge_props(base: Optional[Dict[str, Any]], nxt: Optional[Dict[str, Any]]) -> Dict[str, Any]:
    """Merge `nxt` onto `base` the way `scene.item.set` merges props.

    A nested object is merged, everything else is replaced, and neither
    argument is modified. The client has to do the same arithmetic as the core
    or the predicted frame and the confirmed frame differ by whatever the
    client forgot to carry over.
    """
    out = dict(base or {})
    for key, value in (nxt or {}).items():
        had = out.get(key)
        if isinstance(had, dict) and isinstance(value, dict):
            out[key] = merge_props(had, value)
        else:
            out[key] = value
    return out


class UndoProxy:
    """Ctrl+Z on this window is `scene.undo` in the core.

    The undo stack for the document lives in the core, as an inverse diff stack
    (11 section 4). A client that kept its own would be wrong the moment a
    second client, an agent or the CLI changed anything, so this class does not
    keep one. What it keeps is the shell's stack of one line labels, each step
    of which calls the core and lets the core decide what the inverse is.

    A drag is one step, not forty. `scene.history.mark` is what folds the moves
    between two marks into a single entry, which is why :meth:`group` exists.

    The calls are coroutines, because :class:`godwinmix.Client` is asyncio.
    `stack` is anything with a `push` taking one dictionary.
    """

    def __init__(self, client: Any, stack: Any):
        self.client = client
        self.stack = stack
        #: False once the core has said it has no history, so we stop asking.
        self.available = True
        #: Called with the core's answer after every step, when it is set.
        self.on_step: Optional[Any] = None

    async def mark(self, label: Optional[str] = None) -> None:
        """Name what follows, so a drag becomes one Ctrl+Z."""
        if not self.available:
            return
        try:
            await self.client.call("scene.history.mark", {"label": label} if label else {})
        except Exception as error:  # a core without history says so once
            if getattr(error, "code", None) == -32601:
                self.available = False
                return
            raise

    async def group(self, label: str, work: Any, offer: bool = False) -> Any:
        """Run a batch of edits as one undo step.

        The first mark carries the label; the second closes the group, which is
        the shape `scene.history.mark` documents. The entry is pushed only if
        the work succeeded: a failed edit has nothing to take back.
        """
        await self.mark(label)
        result = await work()
        await self.mark(None)
        self.record(label, offer)
        return result

    def record(self, label: str, offer: bool = False) -> None:
        """One finished change, as a line in the shell's undo menu.

        `offer` puts an Undo button in a toast, which the destructive ones
        want.
        """
        if not self.available:
            return
        self.stack.push({"label": label, "offer": bool(offer), "undo": self.undo, "redo": self.redo})

    async def undo(self) -> Any:
        return await self.step("scene.undo")

    async def redo(self) -> Any:
        return await self.step("scene.redo")

    async def step(self, method: str) -> Any:
        """The core answers with the patch it applied and how many steps are left.

        That is enough for a menu to grey itself out without asking a second
        question.
        """
        answer = await self.client.call(method, {})
        if self.on_step:
            self.on_step(answer)
        return answer
