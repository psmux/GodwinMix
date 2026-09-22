// The shell: slots, the panels in them, and the handful of things that belong to
// the window rather than to any panel. It owns no mixer state; every panel reads
// the client's store and renders at flush. What it owns is where things sit,
// what the keyboard does, the undo stack, the drawer and the file drop zone.

import { el, clear, on } from "./dom.js";
import * as layout from "./layout.js";
import * as registry from "./registry.js";
import { Keymap } from "./keymap.js";
import { UndoStack } from "./undo.js";
import { registerAll, register, addProtocolCommands } from "./commands.js";
import { openSettings, settings, onSettingsChanged } from "./settings.js";
import { initTheme } from "./theme.js";
import { toast, errorToast } from "./toast.js";
import { openPicker, pickFromDrop } from "./picker-loader.js";
import { modal } from "./modal.js";
import { Workspace } from "./dock.js";
import { mountRestartBar } from "./restart-bar.js";

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
    for (const spec of registry.list()) {
      const slot = spec.slots.find(s => s === "header" || s === "modal");
      if (!slot || this.mounted.has(spec.id)) continue;
      const made = registry.instantiate(spec.id, client, {});
      if (!made) continue;
      this.slots[slot].appendChild(made.node);
      this.mounted.set(spec.id, made);
    }
    if (!this.workspace) {
      this.classList.add("has-workspace");
      this.workspace = new Workspace(this, client, this.layout);
    }
    this.workspace.sync();
  }

  unmount(id) {
    const made = this.mounted.get(id);
    if (!made) return;
    made.destroy();
    this.mounted.delete(id);
  }
}

/** Keep each control group separate from the programme monitor. */
export function panelSection(id, node) {
  const titles = { "core/sources": "Sources", "core/scenes": "Scenes", "core/outputs": "Outputs", "core/media": "Media", "core/alerts": "Alerts" };
  if (!titles[id]) return node;
  const key = "gmx.section." + id;
  let open = id === "core/sources" || id === "core/scenes";
  try {
    const saved = localStorage.getItem(key);
    if (saved !== null) open = saved === "open";
  } catch { /* Keep the default when storage is unavailable. */ }
  const section = el("details.panel-section", { open, "data-panel": id }, [
    el("summary", { text: titles[id] }), node,
  ]);
  section.addEventListener("toggle", () => {
    try { localStorage.setItem(key, section.open ? "open" : "closed"); } catch { /* Optional preference. */ }
  });
  return section;
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
  // Settings written to the file that wait for a restart, and the restart.
  mountRestartBar(client);
  // Notifications belong to the window, including when Alerts is closed.
  client.on("alert", a => toast({ kind: a.severity, text: a.message, ms: a.severity === "info" ? 6000 : 12000 }));
  fileDrop(client);
  applyTileWidth();
  onSettingsChanged((s, key) => {
    if (key === "tileWidth") applyTileWidth();
  });

  // Every method the core publishes becomes a palette entry. Silent on a core
  // that has no core.api, which is every core today. The form itself is built
  // by the palette module, which arrives the first time somebody opens one.
  addProtocolCommands(client, (m) => openMethodForm(client, m)).catch(() => {});

  return node;
}

/**
 * The palette, and the schema form it builds its method dialogs from.
 *
 * Ten kilobytes that a page which never sees Ctrl+K never fetches. The same
 * rule as the composer and the legacy adapter: nothing runs, and nothing is
 * downloaded, unless somebody asks for it.
 */
function palette() {
  return import("./palette.js");
}

async function openMethodForm(client, method) {
  const [{ methodForm }, { SchemaForm }] = await Promise.all([palette(), import("../client/schema-form.js")]);
  return methodForm(client, method, SchemaForm);
}

function applyTileWidth() {
  document.documentElement.style.setProperty("--tile-w", settings().tileWidth + "px");
}

function shellCommands(client, node) {
  registerAll([
    { id: "shell.palette", title: "Command palette", group: "Shell", key: "Ctrl+K", run: () => palette().then((m) => m.openPalette()) },
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
    { id: "output.add", title: "Add an output", group: "Outputs", run: () => openPicker(client, "output") },
    {
      id: "program.black",
      title: "Cut to black",
      group: "Programme",
      key: "0",
      // `program.take {}`, named neither a source nor a scene, is the slate.
      // The one exception the core documents is a scene that is armed: with
      // nothing named it takes that instead, which is the behaviour the
      // protocol has had since before scenes and is not this key's to change.
      run: () => client.call("program.take", {}).catch((e) => errorToast(e, "Cut to black")),
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
      // The media panel claims this when it mounts; without it there is
      // nowhere to put a file and saying so beats swallowing the drop.
      if (shell.onFiles) shell.onFiles(dt.files);
      else toast({ text: "There is nowhere to put a file: the Media panel is not on screen." });
      return;
    }
    const text = dt.getData("text/uri-list") || dt.getData("text/plain");
    if (text) pickFromDrop(client, text.split("\n")[0]);
  });
}
