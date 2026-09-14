// The shell: slots, the panels in them, and the handful of things that belong
// to the window rather than to any panel.
//
// It owns no mixer state. Every panel reads the client's store and renders at
// flush. What the shell owns is where things sit, what the keyboard does, the
// undo stack, the drawer, and the file drop zone.

import { el, clear, on } from "./dom.js";
import * as layout from "./layout.js";
import * as registry from "./registry.js";
import { Keymap } from "./keymap.js";
import { UndoStack } from "./undo.js";
import { registerAll, register, addProtocolCommands } from "./commands.js";
import { openPalette, methodForm } from "./palette.js";
import { openSettings, settings, onSettingsChanged } from "./settings.js";
import { initTheme } from "./theme.js";
import { toast, errorToast } from "./toast.js";
import { openPicker, pickFromDrop } from "./picker.js";
import { modal } from "./modal.js";
import { SchemaForm } from "../client/schema-form.js";

/** Everything a panel might want that is not the client. One object, one import. */
export const shell = {
  client: null,
  undo: new UndoStack(),
  keymap: new Keymap(),
  /** Put a node in the sidebar drawer. Call with null to close it. */
  drawer(node) {
    const slot = document.querySelector(".slot-sidebar");
    if (!slot) return;
    clear(slot);
    if (node) slot.appendChild(node);
    document.body.classList.toggle("drawer-open", !!node);
  },
  drawerOpen() {
    const slot = document.querySelector(".slot-sidebar");
    return !!(slot && slot.firstChild);
  },
  toast,
  errorToast,
  modal,
  openPicker: (what, opts) => openPicker(shell.client, what, opts),
};

export class GmxShell extends HTMLElement {
  connectedCallback() {
    if (this.dataset.mounted) return;
    this.dataset.mounted = "1";
    this.slots = {};
    for (const name of layout.SLOTS) {
      const node = el("div.slot-" + name, { "data-slot": name });
      this.slots[name] = node;
      this.appendChild(node);
    }
    this.mounted = new Map();
    this.layout = layout.load();
  }

  /** Build every panel the layout names, in the slot it names. */
  mountPanels(client) {
    for (const [slot, ids] of Object.entries(this.layout)) {
      const host = this.slots[slot];
      if (!host) continue;
      for (const id of ids) {
        if (this.mounted.has(id)) continue;
        const made = registry.instantiate(id, client, {});
        if (!made) continue;
        host.appendChild(made.node);
        this.mounted.set(id, made);
      }
    }
    // A panel that registered for a slot the layout does not mention, which is
    // every plugin panel on a first run, goes into the first slot it declares.
    for (const spec of registry.list()) {
      if (this.mounted.has(spec.id)) continue;
      const slot = spec.slots.find((s) => this.slots[s]) || "sidebar";
      const made = registry.instantiate(spec.id, client, {});
      if (!made) continue;
      this.slots[slot].appendChild(made.node);
      this.mounted.set(spec.id, made);
      this.layout = layout.place(this.layout, slot, spec.id);
    }
    layout.save(this.layout);
  }

  unmount(id) {
    const made = this.mounted.get(id);
    if (!made) return;
    made.destroy();
    this.mounted.delete(id);
  }
}

customElements.define("gmx-shell", GmxShell);

/**
 * Boot. Called once from index.js with a connected client.
 */
export async function mountShell(client, root) {
  initTheme();
  shell.client = client;

  const node = root || document.querySelector("gmx-shell") || el("gmx-shell");
  if (!node.isConnected) document.body.appendChild(node);

  // Plugin panels first, so a panel that wants a slot gets one on the first
  // mount rather than after a reload.
  await registry.discover(location.origin);
  node.mountPanels(client);

  registry.onPanelsChanged(() => node.mountPanels(client));

  shellCommands(client, node);
  shell.keymap.attach(window);
  connectionBanner(client);
  fileDrop(client);
  applyTileWidth();
  onSettingsChanged((s, key) => {
    if (key === "tileWidth") applyTileWidth();
  });

  // Every method the core publishes becomes a palette entry. Silent on a core
  // that has no core.api, which is every core today.
  addProtocolCommands(client, (m) => methodForm(client, m, SchemaForm)).catch(() => {});

  return node;
}

function applyTileWidth() {
  document.documentElement.style.setProperty("--tile-w", settings().tileWidth + "px");
}

function shellCommands(client, node) {
  registerAll([
    { id: "shell.palette", title: "Command palette", group: "Shell", key: "Ctrl+K", run: () => openPalette() },
    { id: "shell.settings", title: "Settings", group: "Shell", key: "Ctrl+,", run: () => openSettings(client) },
    { id: "shell.undo", title: "Undo", group: "Shell", key: "Ctrl+Z", enabled: () => shell.undo.canUndo, run: () => shell.undo.undo() },
    { id: "shell.redo", title: "Redo", group: "Shell", key: "Ctrl+Shift+Z", enabled: () => shell.undo.canRedo, run: () => shell.undo.redo() },
    { id: "shell.shortcuts", title: "Keyboard shortcuts", group: "Shell", key: "?", run: () => shortcutSheet() },
    {
      id: "shell.reload-panels",
      title: "Reload plugin panels",
      group: "Shell",
      run: async () => {
        await registry.discover(location.origin);
        node.mountPanels(client);
        toast({ text: "Plugin panels reloaded." });
      },
    },
    { id: "source.add", title: "Add an input", group: "Sources", key: "Ctrl+N", run: () => openPicker(client, "source") },
    { id: "output.add", title: "Add an output", group: "Outputs", run: () => openPicker(client, "output") },
    {
      id: "program.black",
      title: "Cut to black",
      group: "Programme",
      key: "0",
      run: () => client.call("program.take", { source: null }).catch((e) => errorToast(e, "Cut to black")),
    },
  ]);
}

function shortcutSheet() {
  const rows = Object.entries(shell.keymap.map).map(([chord, id]) =>
    el("div.row", {}, [el("span.num", { text: chord, style: { minWidth: "8em" } }), el("span.dim", { text: id })])
  );
  modal({ title: "Keyboard", body: el("div.col.sm", {}, rows) });
}

/** One banner when the socket is down, so nobody stares at stale numbers. */
function connectionBanner(client) {
  const banner = el("div.scrim", { style: { zIndex: "90" } }, [
    el("div.dialog", { style: { padding: "var(--pad)", maxWidth: "460px" } }, [
      el("div.body", {}, [
        el("h2", { text: "Disconnected from the mixer" }),
        el("p.dim", { text: "Retrying. The programme keeps going out while this page is away; only the page is blind." }),
      ]),
    ]),
  ]);
  banner.hidden = true;
  document.body.appendChild(banner);
  client.on("open", () => {
    banner.hidden = true;
  });
  client.on("close", () => {
    banner.hidden = false;
  });
}

/**
 * Files, URLs and addresses dropped on the window.
 *
 * This is the one place HTML5 drag and drop is used, because the drag starts
 * outside the page and there is no other way to receive it. Everything that
 * drags inside the page uses pointer events (see pointer.js), which is what
 * keeps the two from fighting in WebView2.
 */
function fileDrop(client) {
  let depth = 0;
  const show = () => document.body.classList.add("dropping");
  const hide = () => document.body.classList.remove("dropping");

  on(window, "dragenter", (e) => {
    if (!e.dataTransfer) return;
    depth += 1;
    show();
  });
  on(window, "dragover", (e) => {
    // Without this the browser navigates away from the page mid programme.
    e.preventDefault();
  });
  on(window, "dragleave", () => {
    depth = Math.max(0, depth - 1);
    if (depth === 0) hide();
  });
  on(window, "drop", async (e) => {
    e.preventDefault();
    depth = 0;
    hide();
    const dt = e.dataTransfer;
    if (!dt) return;
    if (dt.files && dt.files.length) {
      shell.emit && shell.emit("files", dt.files);
      if (shell.onFiles) shell.onFiles(dt.files);
      else toast({ text: "Open the Media panel to upload files." });
      return;
    }
    const text = dt.getData("text/uri-list") || dt.getData("text/plain");
    if (text) pickFromDrop(client, text.split("\n")[0]);
  });
}
