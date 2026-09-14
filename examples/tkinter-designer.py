#!/usr/bin/env python3
"""A GodwinMix scene designer in Tkinter.

    pip install godwinmix pillow
    python3 examples/tkinter-designer.py --url http://127.0.0.1:8080 --token TOKEN

The token is the one in `godwinmix.toml` under `[control] token`, or whatever
`gmx token` printed. `GMX_URL` and `GMX_TOKEN` are read when the flags are left
off. Pillow is optional: without it the preview stays grey and everything else
still works.

What it does, and where each part comes from:

* the scene list, from `scene.list`. Arm one with `scene.preview.set`, put it on
  air with `program.take`.
* the picture behind the handles: `/mjpeg/preview` when the core has the route,
  a still from `scene.preview.frame` when it does not, and plain grey when
  neither is available. A designer who thinks they are seeing their scene and
  is seeing nothing should be told which, so the note under the stage says.
* the item outlines, the handles and the snap guides, from
  `godwinmix.kits.tkcanvas`. Every number in them is computed by
  `godwinmix.kits.canvas`, which is the same arithmetic the web composer runs.
* a drag, sent as `scene.item.set` with `duration_ms` 0 and a sequence number
  from `godwinmix.kits.protocol.Prediction`, so the drawing never rubber bands
  on a slow echo.
* the inspector, built by `godwinmix.kits.tkrender` from the item type's data
  schema and the `[provides.designer]` block of its plugin. A plugin that ships
  a schema and a designer block gets a working editor here with no HTML
  anywhere, which is the point of 11 section 5.
* Apply and Discard, against `scene.edit.begin`, `scene.edit.apply` and
  `scene.edit.discard`. Editing is off air until you apply.

The asyncio loop runs on a thread of its own and reaches the widgets through
`after_idle`, as Tkinter is not thread safe. examples/tkinter-panel.py is the
smaller version of the same shape.
"""

import argparse
import asyncio
import base64
import io
import json
import os
import threading
import time
import tkinter as tk
import urllib.error
import urllib.request
from tkinter import ttk

import godwinmix
from godwinmix.kits.protocol import Prediction, SceneMirror, echo_seq_of, geometry_index
from godwinmix.kits.tkcanvas import DesignerCanvas
from godwinmix.kits.tkrender import inspector

GREY = "#2b2b2b"
PREVIEW_WIDTH = 640


class Designer:
    def __init__(self, root, url, token):
        self.root, self.url, self.token = root, url, token
        self.client = None
        self.loop = asyncio.new_event_loop()
        self.preview_stop = threading.Event()
        self.photo = None

        # The document as this client believes it to be, and the drag in flight.
        self.mirror = SceneMirror()
        self.prediction = Prediction()
        self.scenes = []
        self.scene = None
        self.draft = None
        self.designers = {}  # item type id -> {"schema", "designer", "ui"}
        self.panel = None

        root.title("GodwinMix designer")
        root.columnconfigure(1, weight=1)
        root.rowconfigure(1, weight=1)
        self._build(root)
        threading.Thread(target=self._run_loop, daemon=True).start()
        root.protocol("WM_DELETE_WINDOW", self.quit)

    # ------------------------------------------------------------- widgets

    def _build(self, root):
        self.status = ttk.Label(root, text=f"connecting to {self.url}…")
        self.status.grid(row=0, column=0, columnspan=3, sticky="w", padx=8, pady=(8, 4))

        left = ttk.Frame(root)
        left.grid(row=1, column=0, sticky="ns", padx=(8, 4))
        ttk.Label(left, text="Scenes").pack(anchor="w")
        self.scene_list = tk.Listbox(left, width=22, height=12, exportselection=False)
        self.scene_list.pack(fill="y", expand=True)
        self.scene_list.bind("<<ListboxSelect>>", lambda _e: self.open_selected())
        for text, command in (("Arm", self.arm), ("Take", self.take), ("Edit", self.begin_edit)):
            ttk.Button(left, text=text, command=command).pack(fill="x", pady=1)

        stage = ttk.Frame(root)
        stage.grid(row=1, column=1, sticky="nsew", padx=4)
        stage.rowconfigure(0, weight=1)
        stage.columnconfigure(0, weight=1)
        # Tk has no transparent canvas, so the overlay is what you look at. The
        # Label under it holds the PhotoImage (Tk drops an image nobody holds a
        # reference to) and is the plain grey last fallback with no preview.
        self.picture = tk.Label(stage, background=GREY, borderwidth=0)
        self.picture.grid(row=0, column=0, sticky="nsew")
        self.canvas = tk.Canvas(stage, background=GREY, highlightthickness=0, width=640, height=360)
        self.canvas.grid(row=0, column=0, sticky="nsew")
        self.note = ttk.Label(stage, text="no picture yet")
        self.note.grid(row=1, column=0, sticky="w", pady=(4, 0))
        self.painter = DesignerCanvas(
            self.canvas,
            on_drag=self.item_dragged,
            on_drop=self.item_dropped,
            on_select=self.item_selected,
        )

        right = ttk.Frame(root, width=320)
        right.grid(row=1, column=2, sticky="ns", padx=(4, 8))
        ttk.Label(right, text="Inspector").pack(anchor="w")
        self.inspector_box = ttk.Frame(right)
        self.inspector_box.pack(fill="both", expand=True)
        buttons = ttk.Frame(right)
        buttons.pack(fill="x", pady=6)
        ttk.Button(buttons, text="Apply", command=self.apply_edit).pack(side="left")
        ttk.Button(buttons, text="Discard", command=self.discard_edit).pack(side="left", padx=4)
        ttk.Button(buttons, text="Send settings", command=self.send_settings).pack(side="right")

    # ----------------------------------------------------------- the loop

    def _run_loop(self):
        asyncio.set_event_loop(self.loop)
        self.loop.run_until_complete(self._connect())
        self.loop.run_forever()

    async def _connect(self):
        try:
            self.client = await godwinmix.connect(self.url, self.token)
        except OSError as error:
            self.say(str(error))
            return
        # `scene.patch` is the document's own delta stream. A core that does not
        # send it yet simply never fires the handler, and every mutating call
        # still answers with the whole view, which is what `apply_view` reads.
        await self.client.subscribe(events=list(godwinmix.UI_EVENTS) + ["scene.*"])
        self.client.on("scene.patch", self.patched)
        await self.client.settled()
        await self.load_scenes()

    def later(self, coroutine):
        """Run a coroutine on the asyncio thread from a widget callback."""
        if self.client:
            return asyncio.run_coroutine_threadsafe(coroutine, self.loop)
        return None

    def say(self, text):
        self.root.after_idle(lambda: self.status.config(text=text))

    async def guarded(self, coroutine, what):
        """Run one call and put a refusal in the status line rather than a traceback."""
        try:
            return await coroutine
        except godwinmix.RpcError as error:
            self.say(f"{what}: {error.message}")
        except OSError as error:
            self.say(f"{what}: {error}")
        return None

    # ---------------------------------------------------------- the scenes

    async def load_scenes(self):
        listing = await self.guarded(self.client.scene_list(), "scene.list")
        if listing is None:
            return
        self.scenes = listing.get("scenes") or []
        self.root.after_idle(self._fill_scene_list)
        armed = next((s for s in self.scenes if s.get("armed")), None)
        wanted = armed or (self.scenes[0] if self.scenes else None)
        if wanted:
            await self.open_scene(wanted["id"])

    def _fill_scene_list(self):
        self.scene_list.delete(0, "end")
        for summary in self.scenes:
            mark = " *" if summary.get("armed") else ""
            self.scene_list.insert("end", f"{summary['name']}{mark}")

    def selected_scene(self):
        chosen = self.scene_list.curselection()
        return self.scenes[chosen[0]] if chosen else None

    def open_selected(self):
        summary = self.selected_scene()
        if summary and summary["id"] != self.scene:
            self.later(self.open_scene(summary["id"]))

    async def open_scene(self, scene_id):
        view = await self.guarded(self.client.scene_get(scene=scene_id), "scene.get")
        if view is None:
            return
        self.scene = view["id"]
        await self.load_designers(view)
        self.use_view(view)
        self.restart_preview()

    def use_view(self, view):
        """One `SceneView` into the mirror, and out again as boxes to draw."""
        self.mirror.apply_view(view)
        geometry = geometry_index(view)
        items = []
        for record in self.mirror.descendants(view["id"]):
            box = geometry.get(record["id"])
            if box is None:
                continue
            items.append(
                {
                    "id": record["id"],
                    "label": record.get("name") or record["id"],
                    "box": {"x": box["x"], "y": box["y"], "width": box["width"], "height": box["height"]},
                    "transform": record.get("transform") or {},
                    "designer": (self.designers.get(type_of(record)) or {}).get("designer"),
                }
            )
        canvas = view.get("canvas") or {"width": 1920, "height": 1080}
        self.root.after_idle(self._redraw, canvas, items)

    def _redraw(self, canvas, items):
        selected = self.painter.selected
        self.painter.set_scene(canvas, items)
        if selected and any(i["id"] == selected for i in items):
            self.painter.selected = selected
        self.painter.draw()

    def patched(self, patch):
        """One `event/scene.patch`: apply it, and redraw unless it is ours."""
        answer = self.mirror.apply_patch(patch)
        if answer["gap"]:
            # Numbers were skipped, so something was missed. Reread rather than
            # draw a document with a hole in it.
            self.later(self.open_scene(self.scene))
            return
        if not answer["applied"]:
            return
        self.prediction.settle(echo_seq_of(patch))
        self.later(self.reread())

    async def reread(self):
        if self.scene:
            view = await self.guarded(self.client.scene_get(scene=self.scene), "scene.get")
            if view:
                self.use_view(view)

    # ------------------------------------------------------------- editing

    def arm(self):
        summary = self.selected_scene()
        if summary:
            self.later(self._arm(summary))

    async def _arm(self, summary):
        if await self.guarded(self.client.scene_preview_set(scene=summary["id"]), "arm") is not None:
            self.say(f"armed {summary['name']}")
            await self.load_scenes()

    def take(self):
        summary = self.selected_scene()
        if summary:
            self.later(self.guarded(self.client.program_take(scene=summary["id"]), "take"))

    def begin_edit(self):
        if self.scene:
            self.later(self._begin_edit())

    async def _begin_edit(self):
        begun = await self.guarded(self.client.scene_edit_begin(scene=self.scene), "scene.edit.begin")
        if begun is None:
            return
        self.draft = begun.get("draft")
        self.say(f"editing a copy. Nothing reaches air until Apply. draft {self.draft}")
        if begun.get("view"):
            self.use_view(begun["view"])

    def apply_edit(self):
        if self.draft:
            self.later(self._finish(self.client.scene_edit_apply(draft=self.draft), "applied"))

    def discard_edit(self):
        if self.draft:
            self.later(self._finish(self.client.scene_edit_discard(draft=self.draft), "discarded"))

    async def _finish(self, call, what):
        if await self.guarded(call, what) is None:
            return
        self.draft = None
        self.say(f"{what} the draft")
        await self.open_scene(self.scene)

    # ------------------------------------------------------------ dragging

    def item_dragged(self, item, props, _box):
        """Every move of the pointer: draw it here, send it with its number.

        The painter has already redrawn from the box `apply_drag` answered
        with. This only has to get the props to the core, and the sequence
        number is what lets a late echo be thrown away rather than drawn.
        """
        seq = self.prediction.predict(item, props)
        self.later(self._send(item, props, seq))

    async def _send(self, item, props, seq):
        answer = await self.guarded(
            self.client.scene_item_set(
                item=item,
                props=props,
                scene=self.scene,
                draft=self.draft,
                duration_ms=0,
                seq=seq,
            ),
            "scene.item.set",
        )
        if answer is None:
            self.prediction.settle(seq)  # the move failed; stop holding it
            return
        # A mutating call answers with the view, which is the core's own word
        # on where the item ended up, clamps included.
        self.prediction.settle(seq)
        # The answer is the view: the core's own word on where the item ended
        # up, clamps included. It is drawn only once nothing is in flight, or
        # it would drag the handle backwards under the hand.
        if answer.get("records") and not self.prediction.busy:
            self.use_view(answer)

    def item_dropped(self, item):
        self.say(f"{item} released")

    def item_selected(self, item):
        self.root.after_idle(self._build_inspector, item)

    # ----------------------------------------------------------- inspector

    async def load_designers(self, view):
        """One `plugin.describe` per item type on this scene, cached by type id.

        The schema is what the inspector renders. The `designer` block is what
        says which handles the item has and, when it names one, where its UI
        schema lives.
        """
        wanted = {type_of(r) for r in view.get("records") or []} - {None} - set(self.designers)
        for type_id in wanted:
            described = await self.guarded(
                self.client.plugin_describe(id=type_id.split("/")[0]),
                f"plugin.describe {type_id}",
            )
            if described is None:
                self.designers[type_id] = {}
                continue
            designer = designer_block(described, type_id)
            self.designers[type_id] = {
                "schema": (described.get("schemas") or {}).get(type_id),
                "designer": designer,
                "ui": await asyncio.to_thread(self.fetch_ui, described.get("name"), designer),
            }

    def fetch_ui(self, plugin, designer):
        """The UI schema the designer block names, over the core's plugin route.

        A path that will not load costs the layout and nothing else: the data
        schema below it still renders every property, which is the fallback
        chain of 11 section 5 doing its job.
        """
        path = (designer or {}).get("ui")
        if not path or not plugin:
            return None
        base = godwinmix.urls.http_base(self.url)
        name = str(path).split("/")[-1]
        url = f"{base}/plugins/{plugin}/ui/{name}"
        if self.token:
            url += f"?token={self.token}"
        try:
            with urllib.request.urlopen(url, timeout=5) as response:
                return json.loads(response.read())
        except (urllib.error.URLError, OSError, ValueError):
            return None

    def _build_inspector(self, item_id):
        for child in self.inspector_box.winfo_children():
            child.destroy()
        self.panel = None
        record = self.mirror.record(item_id) if item_id else None
        if record is None:
            return
        known = self.designers.get(type_of(record)) or {}
        schema = known.get("schema")
        if not schema:
            ttk.Label(self.inspector_box, text="this item type ships no schema").pack(anchor="w")
            return
        value = (record.get("content") or {}).get("params") or {}
        self.panel = inspector(self.inspector_box, schema, ui=known.get("ui"), value=value)
        self.panel.pack(fill="both", expand=True)

    def send_settings(self):
        """The inspector's values, onto the selected item's own params."""
        if not self.panel or not self.painter.selected:
            return
        props = {"content": {"params": self.panel.read()}}
        item = self.painter.selected
        self.later(self._send(item, props, self.prediction.predict(item, props)))

    # ------------------------------------------------------------- picture

    def restart_preview(self):
        self.preview_stop.set()  # the running one notices and returns
        self.preview_stop = threading.Event()
        threading.Thread(target=self._preview, args=(self.preview_stop,), daemon=True).start()

    def _preview(self, stop):
        """MJPEG first, the still second, grey third.

        `/mjpeg/preview` is the armed scene composited at preview rate.
        `scene.preview.frame` is the floor, and it needs a mosaic running, so a
        core with neither leaves the grey canvas and the note says so.
        """
        decode = _decoder()
        if decode is None:
            self.root.after_idle(lambda: self.note.config(text="pip install pillow for the preview"))
            return
        try:
            for jpeg in godwinmix.read_mjpeg(self.client.mjpeg_url("preview", width=PREVIEW_WIDTH)):
                if stop.is_set():
                    return
                self.root.after_idle(self._show, decode(jpeg), "the armed scene, live")
        except (urllib.error.URLError, OSError, StopIteration):
            pass
        self._poll_stills(stop, decode)

    def _poll_stills(self, stop, decode):
        while not stop.is_set():
            jpeg = self._still()
            if jpeg:
                self.root.after_idle(self._show, decode(jpeg), "a still of the armed scene")
            else:
                self.root.after_idle(lambda: self.note.config(text="no preview: arm a scene, or start a mosaic"))
            time.sleep(2.0)

    def _still(self):
        """One `scene.preview.frame`, from a thread, as bytes or None.

        The core spells the picture `image`; the reference web composer reads
        `jpeg`. Both are read here rather than picking a side.
        """
        waiting = self.later(self.client.scene_preview_frame(width=PREVIEW_WIDTH))
        if waiting is None:
            return None
        try:
            answer = waiting.result(timeout=10) or {}
        except Exception:
            return None
        encoded = answer.get("image") or answer.get("jpeg")
        return base64.b64decode(encoded) if encoded else None

    def _show(self, image, note):
        self.photo = image  # Tk drops an image nobody is holding a reference to.
        self.picture.config(image=image)
        self.canvas.delete("gmx-picture")
        self.canvas.create_image(0, 0, image=image, anchor="nw", tags="gmx-picture")
        self.canvas.tag_lower("gmx-picture")
        self.note.config(text=note)
        self.painter.draw()

    def quit(self):
        self.preview_stop.set()
        if self.client:
            if self.draft:
                self.later(self.client.scene_edit_discard(draft=self.draft))
            self.later(self.client.close())
        self.root.destroy()


def type_of(record):
    """The item type id of one record: `cam1`, or `lower-thirds/ticker`."""
    content = record.get("content") or {}
    if content.get("type") == "graphic":
        return content.get("graphic")
    if content.get("type") == "source":
        return content.get("source")
    return None


def designer_block(described, type_id):
    """`[provides.designer]` for one provide of a plugin's manifest."""
    short = type_id.split("/")[-1]
    for provide in (described.get("manifest") or {}).get("provides") or []:
        if provide.get("id") in (type_id, short):
            return provide.get("designer")
    return None


def _decoder():
    """A JPEG to a Tk image, when Pillow is installed. None when it is not."""
    try:
        from PIL import Image, ImageTk
    except ImportError:
        return None

    def decode(jpeg):
        return ImageTk.PhotoImage(Image.open(io.BytesIO(jpeg)))

    return decode


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--url", default=os.environ.get("GMX_URL", "http://127.0.0.1:8080"))
    ap.add_argument("--token", default=os.environ.get("GMX_TOKEN"))
    args = ap.parse_args()
    root = tk.Tk()
    Designer(root, args.url, args.token)
    root.mainloop()


if __name__ == "__main__":
    main()
