// The tray: every input as a tile.
//
// This is the panel 05 section 3a is about. Tiles you select, rename, colour,
// sweep over and tap. A tap puts a source on air; in producer mode it arms it
// instead. The gallery toggle steps every tile between live, snapshot, icon and
// label, and only the live mode costs the core anything.

import { el, clear, on } from "../../shell/dom.js";
import { DragSelect } from "../../shell/pointer.js";
import { Selection } from "../../shell/selection.js";
import { contextMenu } from "../../shell/menu.js";
import { registerAll } from "../../shell/commands.js";
import { shell } from "../../shell/shell.js";
import { toast, errorToast } from "../../shell/toast.js";
import { confirmModal } from "../../shell/modal.js";
import { openPicker } from "../../shell/picker.js";
import { settings, setSetting, onSettingsChanged, GALLERY_MODES } from "../../shell/settings.js";
import { AudioGestures, ScrubGestures } from "../../shell/fader.js";
import { addView, dropViews, takeMeters } from "../../shell/meter.js";
import { sheetWidthFor } from "../../client/frames.js";
import { SOURCE_KINDS, kindOfUri } from "../../client/kinds.js";
import { buildTile, syncTile, setTileMode } from "./tile.js";
import { setLocal, nameOf } from "./local.js";
import { emptyState } from "../../shell/firstrun.js";

class SourcesPanel extends HTMLElement {
  static get panel() {
    return { id: "core/sources", title: "Sources", slots: ["main"], tag: "gmx-sources" };
  }

  setClient(client) {
    this.client = client;
    this.audio = new AudioGestures(client);
    this.scrub = new ScrubGestures(client);
  }

  connectedCallback() {
    if (this.built) return;
    this.built = true;
    this.tiles = new Map();
    this.selection = new Selection();
    this.filter = "";
    this.perTile = new Map();

    this.search = el("input", { type: "search", placeholder: "Filter", "aria-label": "Filter sources", style: { maxWidth: "180px" } });
    on(this.search, "input", () => {
      this.filter = this.search.value.trim().toLowerCase();
      this.render(this.client.state);
    });

    this.modeSelect = el("select", { "aria-label": "Tile pictures", title: "What the tiles show" });
    for (const [id, title] of GALLERY_MODES) this.modeSelect.appendChild(el("option", { value: id, text: title }));
    this.modeSelect.value = settings().gallery;
    on(this.modeSelect, "change", () => setSetting("gallery", this.modeSelect.value));

    this.count = el("span.sm.dim");
    this.bar = el("div.row.pad", {}, [
      el("strong", { text: "Sources" }),
      this.count,
      el("span.grow"),
      this.search,
      this.modeSelect,
      el("button.btn.primary", { text: "Add", title: "Ctrl+N", onclick: () => openPicker(this.client, "source") }),
    ]);

    this.grid = el("div.gallery", { role: "listbox", "aria-label": "Sources" });
    this.append(this.bar, this.grid);

    this.drag = new DragSelect({
      container: this.grid,
      selection: this.selection,
      order: () => this.order(),
      onChange: () => this.paintSelection(),
      onActivate: (id) => this.activate(id),
      onOpen: (id) => this.openDrawer(id),
      onDrop: (info) => this.onDrop(info),
      onMenu: (id, e) => this.menu(id, e),
    });

    this.offs = [
      this.client.onRender((s) => this.render(s)),
      this.client.on("meters", (p) => takeMeters(p)),
      this.client.on("position", (p) => this.scrub.take(p.source, p.position_ms, p.duration_ms)),
      onSettingsChanged((s, key) => {
        if (key === "gallery") {
          this.modeSelect.value = s.gallery;
          this.applyMode();
        }
        if (key === "tileWidth" || key === "multiviewFps") this.retune();
      }),
      on(document, "visibilitychange", () => this.retune()),
      registerAll(this.commands()),
    ];

    this.io = new IntersectionObserver((entries) => {
      this.visible = entries.some((e) => e.isIntersecting);
      this.retune();
    }, { threshold: 0.01 });
    this.io.observe(this);
    this.visible = true;
    this.ro = new ResizeObserver(() => this.retune());
    this.ro.observe(this.grid);

    this.render(this.client.state);
  }

  disconnectedCallback() {
    for (const off of this.offs || []) off();
    this.offs = [];
    if (this.drag) this.drag.destroy();
    if (this.io) this.io.disconnect();
    if (this.ro) this.ro.disconnect();
    dropViews("tile:");
    this.release();
    if (this.positionWant) this.positionWant.release();
  }

  // ---------------------------------------------------------------- data

  sources(s) {
    const list = s.sources || [];
    if (!this.filter) return list;
    return list.filter((x) => (nameOf(x) + " " + x.id + " " + x.uri).toLowerCase().includes(this.filter));
  }

  order() {
    return [...this.tiles.keys()];
  }

  mode(id) {
    return this.perTile.get(id) || settings().gallery;
  }

  // ---------------------------------------------------------------- render

  render(s) {
    if (this.audio.busy || this.scrub.busy) {
      // Replacing a node mid drag ends the browser's pointer capture with no
      // way to resume, so the rebuild waits for the hand to come off.
      this.audio.defer(() => this.render(this.client.state));
      return;
    }
    const list = this.sources(s);
    this.count.textContent = list.length ? `${list.length}` : "";

    const signature = list.map((x) => [x.id, x.has_audio !== false, x.seekable === true].join(":")).join("|");
    if (signature !== this.signature) {
      this.signature = signature;
      this.rebuild(list);
    }

    const producer = settings().producer;
    list.forEach((source, i) => {
      const tile = this.tiles.get(source.id);
      if (!tile) return;
      const key = source.id + "/gain";
      syncTile(tile, source, {
        tally: this.client.store.tallyOf(source.id),
        selected: this.selection.has(source.id),
        slot: i < 9 ? i + 1 : 0,
        gain: this.audio.shown(key, source.gain === undefined ? 1 : source.gain),
        faderBusy: this.audio.active.has(key),
        scrubBusy: this.scrub.active.has(source.id),
        showStrip: settings().meters || settings().faders,
        position: source.seekable ? this.scrub.read(source.id) : null,
      });
      if (source.seekable && source.position_ms !== undefined) {
        this.scrub.seed(source.id, source.position_ms, source.duration_ms);
      }
      if (producer && source.id === document.body.dataset.armed) tile.node.classList.add("armed");
    });

    if (!s.sources.length && !this.filter) {
      if (!this.empty) {
        this.empty = emptyState(() => openPicker(this.client, "source"));
        this.appendChild(this.empty);
      }
      this.grid.hidden = true;
    } else if (this.empty) {
      this.empty.remove();
      this.empty = null;
      this.grid.hidden = false;
    }
    this.retune();
  }

  rebuild(list) {
    dropViews("tile:");
    clear(this.grid);
    this.tiles.clear();
    for (const source of list) {
      const tile = buildTile(source, {
        audio: this.audio,
        scrub: this.scrub,
        onGear: (id) => this.openDrawer(id),
        onMute: (id, muted) => this.audio.setMuted(id, muted).catch((e) => errorToast(e, "Mute")),
      });
      this.tiles.set(source.id, tile);
      this.grid.appendChild(tile.node);
      if (source.has_audio !== false) addView("tile:" + source.id, "src:" + source.id, tile.meter, "v");
    }
    this.applyMode();
    this.paintSelection();
  }

  paintSelection() {
    for (const [id, tile] of this.tiles) tile.node.classList.toggle("selected", this.selection.has(id));
  }

  applyMode() {
    const global = settings().gallery;
    this.grid.className = "gallery mode-" + global;
    for (const [id, tile] of this.tiles) setTileMode(tile, this.mode(id));
    this.retune();
    this.refreshStills(true);
  }

  // ------------------------------------------------------------ pictures

  /**
   * Ask the core for exactly the pictures that are on screen, at exactly the
   * width they are drawn at, and for nothing else. A gallery on icon or label
   * mode releases the subscription entirely, which is the whole point.
   */
  retune() {
    const s = this.client.state;
    const positions = this.visible && !document.hidden && s.sources.some((source) => source.seekable);
    if (positions && !this.positionWant) this.positionWant = this.client.want("positions", true);
    else if (!positions && this.positionWant) {
      this.positionWant.release();
      this.positionWant = null;
    }
    const live = [...this.tiles.entries()].filter(([id]) => this.mode(id) === "live");
    const wanted = this.visible && !document.hidden && live.length > 0 && s.multiview && s.multiview.enabled;
    if (!wanted) {
      this.release();
      return;
    }
    const box = live[0][1].pic.getBoundingClientRect();
    const cols = (s.multiview && s.multiview.cols) || 1;
    const width = sheetWidthFor(box.width || settings().tileWidth, cols);
    const fps = settings().multiviewFps;
    if (!this.want) this.want = this.client.want("multiview", { fps, width });
    else if (width !== this.lastWidth || fps !== this.lastFps) this.want.update({ fps, width });
    this.lastWidth = width;
    this.lastFps = fps;

    for (const off of this.detachers || []) off();
    this.detachers = [];
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    for (const [id, tile] of live) {
      const source = (s.sources || []).find((x) => x.id === id);
      if (!source || source.cell === null || source.cell === undefined) continue;
      const w = Math.round((box.width || 168) * dpr);
      const h = Math.round((w * 9) / 16);
      if (w > 0 && (tile.pic.width !== w || tile.pic.height !== h)) {
        tile.pic.width = w;
        tile.pic.height = h;
      }
      this.detachers.push(this.client.sheet.attach(tile.pic, source.cell));
    }
  }

  release() {
    if (this.want) {
      this.want.release();
      this.want = null;
    }
    for (const off of this.detachers || []) off();
    this.detachers = [];
  }

  /** Snapshot mode: one JPEG per tile, refreshed on a click or on a timer. */
  refreshStills(force) {
    if (this.stillTimer) clearInterval(this.stillTimer);
    this.stillTimer = null;
    const still = [...this.tiles.entries()].filter(([id]) => this.mode(id) === "snapshot");
    if (!still.length) return;
    const load = () => {
      for (const [id, tile] of still) tile.still.src = this.client.snapshotUrl(id, tile.still.clientWidth * 2 || 320);
    };
    if (force) load();
    const secs = settings().snapshotSecs;
    if (secs > 0) this.stillTimer = setInterval(load, secs * 1000);
  }

  // ------------------------------------------------------------ actions

  activate(id) {
    const tile = this.tiles.get(id);
    if (settings().gallery === "snapshot" && tile) {
      tile.still.src = this.client.snapshotUrl(id, tile.still.clientWidth * 2 || 320);
    }
    if (settings().producer) {
      document.body.dataset.armed = id;
      this.client.call("scene.preview.set", { scene: id }).catch(() => {
        // No preview on this core: the armed highlight is all there is, which
        // 05 says is the acceptable fallback.
      });
      this.render(this.client.state);
      return;
    }
    this.client.call("program.take", { source: id }).catch((e) => errorToast(e, "Take"));
  }

  /**
   * The scenes panel, when one is on the page and its core has a scene server.
   *
   * Asked for through the DOM rather than imported, because importing it would
   * pull the scene panel and its protocol kit into every page that never opens
   * one. A core with no scene server leaves `supported` false and this answers
   * null, which is what sends the number keys back to the tray.
   */
  scenesPanel() {
    const node = document.querySelector("gmx-scenes");
    if (!node || !node.scenes || !node.scenes.supported) return null;
    return node.scenes.scenes().length ? node : null;
  }

  /**
   * Number key n, which 05 section 3a says is the nth scene.
   *
   * Scenes are what an operator cuts between once there are any, so 1 to 9
   * count the scene tiles. A collection with no scenes in it, or a core with no
   * scene server at all, falls back to counting the inputs, which is what these
   * keys did before scenes existed.
   */
  takeSlot(n) {
    const slot = Math.max(1, n || 1);
    const panel = this.scenesPanel();
    if (panel) {
      const scene = panel.scenes.scenes()[slot - 1];
      if (scene) panel.activate(scene.id);
      return;
    }
    const id = this.order()[slot - 1];
    if (id) this.activate(id);
  }

  async openDrawer(id) {
    const source = this.client.store.source(id);
    if (!source) return;
    // Fetched on the first gear click rather than with the page.
    const { SchemaForm } = await import("../../client/schema-form.js");
    const kind = SOURCE_KINDS.find((k) => k.id === kindOfUri(source.uri)) || SOURCE_KINDS[0];
    let schema = kind.schema;
    try {
      const described = await this.client.call("plugin.describe", { instance: id });
      if (described && described.schema) schema = described.schema;
    } catch {
      /* no plugin.describe on this core: the built in schema stands in */
    }
    const form = new SchemaForm(schema, { uri: source.uri, name: nameOf(source) });
    const apply = el("button.btn.primary", {
      text: "Apply",
      onclick: async () => {
        try {
          await this.client.call("source.set", Object.assign({ id }, form.read()));
          toast({ text: "Saved." });
        } catch (e) {
          if (e.code === -32601) {
            setLocal(id, { name: form.read().name });
            toast({ kind: "warning", text: "This mixer cannot save settings on a source yet, so the name is kept on this device only." });
            this.render(this.client.state);
          } else {
            errorToast(e, "Save");
          }
        }
      },
    });
    shell.drawer(
      el("div.pad.col", {}, [
        el("div.row", {}, [el("strong.grow", { text: nameOf(source) }), el("button.btn.icon", { text: "×", onclick: () => shell.drawer(null) })]),
        el("div.sm.dim", { text: source.uri }),
        form.el,
        el("div.row", {}, [apply]),
      ])
    );
  }

  onDrop(info) {
    const target = info.target ? String(info.target) : "";
    // A drop that landed on the Scenes panel belongs to that panel: empty
    // space there makes a scene from the selection, a scene tile adds them to
    // it. The event is how two panels in two slots talk without importing each
    // other, which is what keeps every panel replaceable.
    if (target === "scenes:empty" || target.startsWith("scene:")) {
      window.dispatchEvent(
        new CustomEvent("gmx:tiles-dropped", { detail: Object.assign({}, info, { from: "sources" }) })
      );
      return;
    }
    // Tray folders are tags rather than scenes, and `source.group` is not
    // wired to this panel yet, so a drop onto another tile says so rather than
    // silently doing nothing.
    if (info.target && info.targetId && !this.selection.has(info.targetId)) {
      toast({ text: "Tray folders arrive with source.group. To build a scene, drag these onto Scenes." });
    }
  }

  menu(id, e) {
    const ids = this.selection.list(this.order());
    const many = ids.length > 1;
    const key = (cmd) => shell.keymap.keyFor(cmd);
    contextMenu(e.clientX, e.clientY, [
      id && { label: settings().producer ? "Arm" : "Put on air", key: "Click", run: () => this.activate(id) },
      id && { label: "Rename", key: key("tray.rename") || "F2", disabled: many, run: () => this.beginRename(id) },
      id && { kind: "colours", onColour: (colour) => this.setColour(ids, colour) },
      id && { label: "Settings", run: () => this.openDrawer(id) },
      id && { kind: "separator" },
      id && { label: many ? `Remove ${ids.length}` : "Remove", key: key("tray.delete") || "Delete", run: () => this.remove(ids) },
      { kind: "separator" },
      { label: "Add an input", key: key("tray.add") || "Ctrl+N", run: () => openPicker(this.client, "source") },
      { label: "Select all", key: key("tray.select-all") || "Ctrl+A", run: () => this.selectAll() },
    ].filter(Boolean));
  }

  async setColour(ids, colour) {
    for (const id of ids) {
      try {
        await this.client.call("source.set", { id, color: colour });
      } catch (e) {
        if (e.code !== -32601) {
          errorToast(e, "Colour");
          return;
        }
        setLocal(id, { color: colour });
      }
    }
    this.render(this.client.state);
  }

  beginRename(id) {
    const tile = this.tiles.get(id);
    if (!tile) return;
    const before = tile.name.textContent;
    tile.name.contentEditable = "true";
    tile.name.focus();
    const range = document.createRange();
    range.selectNodeContents(tile.name);
    const sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(range);

    const finish = async (commit) => {
      tile.name.contentEditable = "false";
      const after = tile.name.textContent.trim();
      if (!commit || !after || after === before) {
        tile.name.textContent = before;
        return;
      }
      try {
        await this.client.call("source.set", { id, name: after });
        shell.undo.push({
          label: `Renamed to ${after}`,
          undo: () => this.client.call("source.set", { id, name: before }),
          redo: () => this.client.call("source.set", { id, name: after }),
        });
      } catch (e) {
        if (e.code !== -32601) {
          errorToast(e, "Rename");
          tile.name.textContent = before;
          return;
        }
        setLocal(id, { name: after });
        shell.undo.push({
          label: `Renamed to ${after} (on this device)`,
          undo: () => {
            setLocal(id, { name: before === id ? null : before });
            this.render(this.client.state);
          },
          redo: () => {
            setLocal(id, { name: after });
            this.render(this.client.state);
          },
        });
      }
      this.render(this.client.state);
    };

    const off = on(tile.name, "keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Enter") {
        e.preventDefault();
        off();
        finish(true);
      } else if (e.key === "Escape") {
        e.preventDefault();
        off();
        finish(false);
      }
    });
    on(tile.name, "blur", () => {
      off();
      finish(true);
    }, { once: true });
  }

  async remove(ids) {
    if (!ids.length) return;
    if (settings().confirmRemove) {
      const what = ids.length === 1 ? `"${ids[0]}"` : `${ids.length} sources`;
      if (!(await confirmModal(`Remove ${what}? Nothing that is on air is interrupted.`, "Remove"))) return;
    }
    const removed = ids.map((id) => this.client.store.source(id)).filter(Boolean);
    for (const id of ids) {
      try {
        await this.client.call("source.remove", { id });
      } catch (e) {
        errorToast(e, "Remove");
        return;
      }
    }
    const restore = async () => {
      for (const s of removed) {
        await this.client.call("source.add", { id: s.id, name: s.name, uri: s.uri });
      }
    };
    shell.undo.push({
      label: ids.length === 1 ? `Removed ${ids[0]}` : `Removed ${ids.length} sources`,
      undo: restore,
      redo: async () => {
        for (const id of ids) await this.client.call("source.remove", { id });
      },
      offer: true,
    });
  }

  selectAll() {
    this.selection.selectAll(this.order());
    this.paintSelection();
  }

  // ------------------------------------------------------------ commands

  commands() {
    const selected = () => this.selection.list(this.order());
    return [
      { id: "tray.add", title: "Add an input", group: "Sources", key: "Ctrl+N", run: () => openPicker(this.client, "source") },
      { id: "tray.filter", title: "Filter the tray", group: "Sources", key: "Ctrl+F", run: () => this.search.focus() },
      { id: "tray.select-all", title: "Select all", group: "Sources", key: "Ctrl+A", run: () => this.selectAll() },
      {
        id: "tray.escape",
        title: "Clear the selection",
        group: "Sources",
        key: "Escape",
        run: () => {
          this.selection.clear();
          this.paintSelection();
          shell.drawer(null);
        },
      },
      { id: "tray.delete", title: "Remove the selection", group: "Sources", key: "Delete", enabled: () => selected().length > 0, run: () => this.remove(selected()) },
      { id: "tray.rename", title: "Rename", group: "Sources", key: "F2", enabled: () => selected().length === 1, run: () => this.beginRename(selected()[0]) },
      { id: "tray.open", title: "Open settings", group: "Sources", key: "Enter", enabled: () => selected().length === 1, run: () => this.openDrawer(selected()[0]) },
      {
        id: "tray.take-slot",
        title: "Put scene 1 to 9 on air, or input 1 to 9 when there are no scenes",
        group: "Programme",
        key: "1 to 9",
        run: (n) => this.takeSlot(n),
      },
      { id: "tray.copy", title: "Copy", group: "Sources", key: "Ctrl+C", enabled: () => selected().length > 0, run: () => this.copy(selected()) },
      { id: "tray.cut", title: "Cut", group: "Sources", key: "Ctrl+X", enabled: () => selected().length > 0, run: () => this.copy(selected(), true) },
      { id: "tray.paste", title: "Paste", group: "Sources", key: "Ctrl+V", enabled: () => !!this.clipboard, run: () => this.paste() },
      {
        id: "tray.mode",
        title: "Change what the tiles show",
        group: "Sources",
        run: () => {
          const ids = GALLERY_MODES.map(([id]) => id);
          const next = ids[(ids.indexOf(settings().gallery) + 1) % ids.length];
          setSetting("gallery", next);
          toast({ text: `Tiles: ${next}.` });
        },
      },
    ];
  }

  copy(ids, cut) {
    this.clipboard = ids.map((id) => this.client.store.source(id)).filter(Boolean);
    if (cut) this.remove(ids);
    else toast({ text: ids.length === 1 ? `Copied ${ids[0]}.` : `Copied ${ids.length} sources.` });
  }

  async paste() {
    if (!this.clipboard || !this.clipboard.length) return;
    for (const s of this.clipboard) {
      try {
        await this.client.call("source.add", { name: nameOf(s) + " copy", uri: s.uri });
      } catch (e) {
        errorToast(e, "Paste");
        return;
      }
    }
    toast({ text: "Pasted." });
  }
}

customElements.define("gmx-sources", SourcesPanel);
window.godwinmixPanels.push(SourcesPanel);
export default SourcesPanel;
