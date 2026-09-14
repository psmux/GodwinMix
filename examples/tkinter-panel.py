#!/usr/bin/env python3
"""A GodwinMix control panel in Tkinter.

    pip install godwinmix pillow
    python3 examples/tkinter-panel.py --url http://127.0.0.1:8080 --token TOKEN

A source list with take buttons, tally colour, a preview of the programme and a
settings form rendered from a schema. The asyncio loop runs on a thread of its
own and reaches the widgets through `after_idle`, as Tkinter is not thread safe.
docs/how-to/write-a-ui.md walks through every part of it.
"""

import argparse
import asyncio
import io
import os
import threading
import tkinter as tk
from tkinter import ttk

import godwinmix
from godwinmix.tk import SchemaForm
TALLY = {"program": "#c62828", "preview": "#2e7d32", "off": "#3a3a3a"}

# A real panel asks `plugin.describe` for a plugin's schema; the shape is this.
SETTINGS_SCHEMA = {
    "type": "object",
    "title": "Preview",
    "properties": {
        "name": {"type": "string", "title": "Watching", "enum": ["program", "sheet"], "default": "program"},
        "width": {"type": "integer", "title": "Width", "minimum": 160, "default": 640, "x-gmx-unit": "px"},
    },
}


class Panel:
    def __init__(self, root, url, token):
        self.root, self.url, self.token = root, url, token
        self.client, self.photo, self.rows = None, None, {}
        self.loop = asyncio.new_event_loop()
        self.preview_stop = threading.Event()

        root.title("GodwinMix")
        root.columnconfigure(1, weight=1)
        self.status = ttk.Label(root, text=f"connecting to {url}…")
        self.status.grid(row=0, column=0, columnspan=2, sticky="w", padx=8, pady=(8, 4))
        self.sources = ttk.Frame(root)
        self.sources.grid(row=1, column=0, sticky="nw", padx=8)
        self.picture = tk.Label(root, background="#111", width=64, height=18)
        self.picture.grid(row=1, column=1, sticky="nsew", padx=8)
        self.settings = SchemaForm(root, SETTINGS_SCHEMA, {})
        self.settings.grid(row=2, column=1, sticky="ew", padx=8, pady=8)
        ttk.Button(root, text="Apply", command=self.restart_preview).grid(row=3, column=1, sticky="e", padx=8)

        threading.Thread(target=self._run_loop, daemon=True).start()
        root.protocol("WM_DELETE_WINDOW", self.quit)

    def _run_loop(self):
        asyncio.set_event_loop(self.loop)
        self.loop.run_until_complete(self._connect())
        self.loop.run_forever()

    async def _connect(self):
        try:
            self.client = await godwinmix.connect(self.url, self.token)
        except OSError as e:
            self.root.after_idle(lambda: self.status.config(text=str(e)))
            return
        self.client.on_flush(lambda state: self.root.after_idle(self.draw, state))
        # Tally is the one stream this panel asks the core to run for it.
        await self.client.subscribe(ext={"tally": True})
        await self.client.settled()
        self.restart_preview()

    def take(self, source_id):
        """A button press, carried from the Tk thread onto the asyncio one."""
        async def go():
            try:
                await self.client.take(source_id)
            except godwinmix.RpcError as e:
                self.root.after_idle(lambda: self.status.config(text=f"{e.title}: {e.message}"))

        if self.client:
            asyncio.run_coroutine_threadsafe(go(), self.loop)

    def draw(self, state):
        """Called once per flush, never once per event."""
        on_air = state["program"] or "the slate"
        self.status.config(text=f"programme: {on_air}   up {state['uptime_secs']}s   seq {state['seq']}")
        live = {s["id"] for s in state["sources"]}
        for source in state["sources"]:
            row = self.rows.get(source["id"]) or self._add_row(source)
            row["lamp"].config(background=TALLY.get(self.client.tally_of(source["id"]), TALLY["off"]))
            row["button"].config(text=f"{source['name']}  ({source['state']})")
        for source_id in [i for i in self.rows if i not in live]:
            self.rows.pop(source_id)["frame"].destroy()

    def _add_row(self, source):
        frame = ttk.Frame(self.sources)
        frame.pack(fill="x", pady=1)
        lamp = tk.Frame(frame, width=12, height=24, background=TALLY["off"])
        lamp.pack(side="left", padx=(0, 6))
        button = ttk.Button(frame, text=source["id"], command=lambda i=source["id"]: self.take(i))
        button.pack(side="left", fill="x", expand=True)
        self.rows[source["id"]] = {"frame": frame, "lamp": lamp, "button": button}
        return self.rows[source["id"]]

    def restart_preview(self):
        """Start the preview thread again with whatever the settings form says."""
        self.preview_stop.set()  # the running one notices and returns
        self.preview_stop = threading.Event()
        settings = self.settings.read()
        args = (self.preview_stop, settings.get("name", "program"), settings.get("width", 640))
        threading.Thread(target=self._preview, args=args, daemon=True).start()

    def _preview(self, stop, name, width):
        try:
            from PIL import Image, ImageTk
        except ImportError:
            self.root.after_idle(lambda: self.picture.config(text="pip install pillow for the preview"))
            return
        for jpeg in godwinmix.preview_stream(self.client, name, width=width):
            if stop.is_set():
                return
            self.root.after_idle(self._show, ImageTk.PhotoImage(Image.open(io.BytesIO(jpeg))))

    def _show(self, image):
        self.photo = image  # Tk drops an image nobody is holding a reference to.
        self.picture.config(image=image, width=image.width(), height=image.height())

    def quit(self):
        self.preview_stop.set()
        if self.client:
            asyncio.run_coroutine_threadsafe(self.client.close(), self.loop)
        self.root.destroy()


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--url", default=os.environ.get("GMX_URL", "http://127.0.0.1:8080"))
    ap.add_argument("--token", default=os.environ.get("GMX_TOKEN"))
    args = ap.parse_args()
    root = tk.Tk()
    Panel(root, args.url, args.token)
    root.mainloop()


if __name__ == "__main__":
    main()
