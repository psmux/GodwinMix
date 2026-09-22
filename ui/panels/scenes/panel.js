// Scenes: a row of tiles you tap, rename, colour and drag things into.
//
// This is the right hand half of 05 section 3a. A scene is a tile like an input
// is a tile, the same file manager gestures work on both, and every gesture is
// one command on the public protocol:
//
//   tap                     program.take {scene}     (arm, in producer mode)
//   double tap, Enter       scene.edit.begin         the composer, on a copy
//   F2                      scene.rename
//   a colour from the menu  scene.rename {color}
//   drag inputs to space    scene.create_from        laid out by count
//   drag inputs to a tile   scene.item.add
//   drag an item to a tile  scene.item.move / copy
//   Ctrl+C, Ctrl+V          scene.duplicate
//   copy and paste layout   scene.layout.copy / paste
//   Delete                  scene.remove, with an Undo toast
//   Ctrl+Z, Ctrl+Shift+Z    scene.undo / scene.redo
//
// The panel holds no truth of its own. It mirrors the document through the
// protocol kit, which applies `event/scene.patch` and files the view every
// mutating call answers with, so two browsers and an agent editing the same
// collection agree without any of them being special.

import { el, clear, on } from "../../shell/dom.js";
import { DragSelect } from "../../shell/pointer.js";
import { Selection } from "../../shell/selection.js";
import { registerAll } from "../../shell/commands.js";
import { focusedScene, setFocusedScene, onFocusChanged } from "../../shell/focus.js";
import { shell } from "../../shell/shell.js";
import { toast, errorToast } from "../../shell/toast.js";
import { settings } from "../../shell/settings.js";
import { openSceneSources } from "../sources/chooser-loader.js";
import { acquireScenes } from "../../shell/scene-session.js";

/** Colours a scene can be given, matching the swatches in the tile menu. */
const DEFAULT_COLOUR = "var(--kind-stream)";

/**
 * Tabs or tiles, remembered on this device.
 *
 * Tabs are for the desk: a scene list that fits one line down the side of a
 * laptop, where a click chooses what you are working on rather than cutting
 * the programme. Tiles are the file manager grammar above, unchanged.
 */
const VIEW_KEY = "gmx.scenes.view";

function savedView() {
  try {
    return localStorage.getItem(VIEW_KEY) === "tiles" ? "tiles" : "tabs";
  } catch {
    return "tabs";
  }
}

class ScenesPanel extends HTMLElement {
  static get panel() {
    return { id: "core/scenes", title: "Scenes", slots: ["main"], tag: "gmx-scenes" };
  }

  setClient(client) {
    this.client = client;
    this.sceneSession = acquireScenes(client, shell.undo);
    this.scenes = this.sceneSession.scenes;
  }

  connectedCallback() {
    if (this.built) return;
    this.built = true;
    this.tiles = new Map();
    this.tabs = new Map();
    this.view = savedView();
    this.selection = new Selection();
    this.clipboard = null;
    this.layoutClip = null;

    this.count = el("span.sm.dim");
    // Named for the view it switches to, the way the add picker names its own.
    this.viewBtn = el("button.btn.sm", {
      title: "Show the scenes as a strip of tabs or as a grid of tiles",
      onclick: () => this.setView(this.view === "tabs" ? "tiles" : "tabs"),
    });
    this.bar = el("div.row.pad", {}, [
      el("strong", { text: "Scenes" }),
      this.count,
      el("span.grow"),
      this.viewBtn,
      el("button.btn", { text: "New scene", title: "An empty scene to drag inputs into", onclick: () => this.newScene() }),
    ]);

    // The tab strip. A tab is not a tile: clicking one says which scene the
    // operator is working on and touches nothing else, because the tile
    // gesture above takes on a single click and a strip that sits under the
    // thumb cannot afford that. The one button here that reaches the
    // programme is the one that says Take.
    this.take = el("button.btn.sm", {
      text: "Take",
      title: "Put the focused scene on air. In producer mode it arms it instead.",
      onclick: () => {
        const id = focusedScene([...this.tabs.keys()]);
        if (id) this.activate(id);
      },
    });
    this.strip = el("div.tabs.scene-tabs", { role: "tablist", "aria-label": "Scenes" });

    // The grid is the drop target for empty space: dropping inputs on it makes
    // a scene. It takes focus so the keys below belong to this panel and not
    // to the tray, which is how two panels share F2 without fighting.
    this.grid = el("div.gallery", {
      role: "listbox",
      "aria-label": "Scenes",
      tabindex: "0",
      "data-drop": "scenes:empty",
      style: { minHeight: "96px" },
    });
    this.hint = el("div.dim.sm.pad", {
      text: "Choose a scene, then use + in Sources to add existing sources or create new ones. Double click a scene to edit its layout. Outputs send the programme and are shared by every scene.",
      style: { maxWidth: "56ch" },
    });
    this.append(this.bar, this.strip, this.grid, this.hint);

    this.drag = new DragSelect({
      container: this.grid,
      selection: this.selection,
      order: () => [...this.tiles.keys()],
      onChange: () => this.paintSelection(),
      onActivate: (id) => this.activate(id),
      onOpen: (id) => this.open(id),
      // Marked as ours, so a scene tile dropped on empty space is understood
      // as a tile being put down and not as a request to build a scene out of
      // scenes.
      onDrop: (info) => this.dropped(Object.assign({ from: "scenes" }, info)),
      onMenu: (id, e) => this.more().then((m) => m.menu(this, id, e)),
    });

    this.offs = [
      on(window, "gmx:tiles-dropped", (e) => this.dropped(e.detail)),
      on(this.grid, "keydown", (e) => this.key(e)),
      on(this.grid, "pointerdown", () => this.grid.focus({ preventScroll: true })),
      registerAll(this.commands()),
      this.client.onRender(() => this.paintTally()),
      // The focus is the shell's, not this panel's: the composer and the
      // source list move it too, and the strip follows wherever it goes.
      onFocusChanged(() => this.paintTally()),
    ];

    this.offs.push(this.scenes.onChange(() => this.render()));
    this.sceneSession.ready.catch((e) => console.error("the scene server did not answer", e));
    this.render();
  }

  setWorkspaceActive(active) {
    this.workspaceActive = active;
    // Sources shares this mirror; only the hidden surface stops painting.
    if (active) this.render();
  }

  disconnectedCallback() {
    for (const off of this.offs || []) off();
    this.offs = [];
    if (this.drag) this.drag.destroy();
    if (this.sceneSession) this.sceneSession.release();
  }

  // ---------------------------------------------------------------- render

  render() {
    if (this.workspaceActive === false) return;
    const list = this.scenes.scenes();
    this.count.textContent = list.length ? String(list.length) : "";
    this.hint.hidden = false;
    const tabbed = this.view === "tabs" && this.scenes.supported;
    this.viewBtn.textContent = this.view === "tabs" ? "Tiles" : "Tabs";
    this.strip.hidden = !tabbed;
    this.grid.hidden = tabbed;
    if (!this.scenes.supported) {
      this.hint.hidden = false;
      this.hint.textContent =
        "This mixer has no scene server, so there are no scenes yet. Tapping an input still puts it on air.";
      return;
    }

    const signature = list.map((s) => s.id).join("|");
    if (signature !== this.signature) {
      this.signature = signature;
      clear(this.grid);
      this.tiles.clear();
      for (const summary of list) {
        const tile = this.buildTile(summary);
        this.tiles.set(summary.id, tile);
        this.grid.appendChild(tile.node);
      }
    }
    for (const summary of list) this.syncTile(this.tiles.get(summary.id), summary);
    // Built in either view, so the strip is right the moment it is shown and
    // the Take button has a scene to put on air whichever one is up.
    this.renderTabs(list);
    this.paintSelection();
    this.paintTally();
  }

  /** Tabs or tiles, for this panel and for the next time this browser opens. */
  setView(view) {
    this.view = view === "tiles" ? "tiles" : "tabs";
    try {
      localStorage.setItem(VIEW_KEY, this.view);
    } catch {
      /* the choice lasts the session */
    }
    this.render();
  }

  /**
   * The strip, rebuilt only when a name, a count or the list itself moved. A
   * tab the pointer is resting on must not be swapped out under it, and this
   * runs on every change the document sends.
   */
  renderTabs(list) {
    const signature = list.map((s) => `${s.id}/${s.name}/${s.items || 0}`).join("|");
    if (signature !== this.tabSignature) {
      this.tabSignature = signature;
      clear(this.strip);
      this.tabs.clear();
      for (const summary of list) {
        const tab = el(
          "button",
          {
            role: "tab",
            "data-id": summary.id,
            title: "Work on this scene. Double click to arrange it, Take to put it on air.",
            onclick: () => setFocusedScene(summary.id),
            ondblclick: () => this.open(summary.id),
          },
          [el("span.ellipsis", { text: summary.name }), el("span.num.dim", { text: String(summary.items || 0) })]
        );
        // The way in to the composer that can be seen. A double click opens
        // it too, but nobody finds a double click. Beside the tab and not in
        // it, because a button may not hold another button.
        const edit = this.editButton(summary);
        this.tabs.set(summary.id, tab);
        this.strip.appendChild(el("span.scene-tab", {}, [tab, edit]));
      }
      this.strip.appendChild(this.take);
    }
    this.seedFocus(list);
  }

  /** Nothing focused yet: the armed scene, then the live one, then the first. */
  seedFocus(list) {
    const ids = list.map((s) => s.id);
    if (!ids.length || focusedScene(ids)) return;
    const live = this.scenes.live();
    setFocusedScene(this.scenes.armed() || (ids.includes(live) ? live : null) || ids[0]);
  }

  /** The pencil on a scene, in either view: open the composer on it. */
  editButton(summary) {
    return el("button.scene-edit", {
      text: "\u270E",
      "data-nodrag": "",
      title: editLabel(summary.name),
      "aria-label": editLabel(summary.name),
      onclick: (event) => {
        // Not a tap on the tile underneath, which would put the scene on air.
        event.stopPropagation();
        this.open(summary.id);
      },
      ondblclick: (event) => event.stopPropagation(),
    });
  }

  buildTile(summary) {
    const node = el("div.tile", {
      "data-id": summary.id,
      "data-drop": "scene:" + summary.id,
      tabindex: "-1",
      role: "option",
    });
    const face = el("div.kindbox", { style: { position: "relative", aspectRatio: "16 / 9", display: "grid", placeItems: "center" } });
    const items = el("span.num.dim", { style: { fontSize: "var(--fs-lg)" } });
    const add = el("button.scene-add-source", { text: "+", "data-nodrag": "",
      onclick: event => { event.stopPropagation(); setFocusedScene(summary.id); openSceneSources(this.client, this.scenes, this.scenes.summary(summary.id)); },
    });
    const edit = this.editButton(summary);
    face.append(items, add, edit);
    // The rename is on the name itself, which is where a file manager puts it,
    // because the pencil beside it opens the composer and everyone reads a
    // pencil as rename. F2 and the tile's menu do the same thing.
    const name = el("span.name.grow.ellipsis", {
      title: "Double click to rename this scene. F2 renames the selected one.",
      ondblclick: (event) => {
        event.stopPropagation();
        this.beginRename(summary.id);
      },
    });
    const dot = el("span.dot");
    const bar = el("div.bar", {}, [dot, name]);
    const chips = el("div.row", {
      style: { flexWrap: "wrap", gap: "3px", padding: "0 6px 6px", minHeight: "0" },
    });
    node.append(face, bar, chips);
    return { id: summary.id, node, face, items, add, edit, name, dot, chips, chipIds: "", labelled: "" };
  }

  syncTile(tile, summary) {
    if (!tile) return;
    if (!tile.name.isContentEditable && tile.name.textContent !== summary.name) tile.name.textContent = summary.name;
    // Both buttons say which scene they act on, so both follow a rename. One
    // place rather than one per button, because the next button added to a
    // tile will want the same thing.
    if (tile.labelled !== summary.name) {
      tile.labelled = summary.name;
      label(tile.add, `Add sources to ${summary.name}`);
      label(tile.edit, editLabel(summary.name));
    }
    tile.node.style.setProperty("--tile-color", summary.color || DEFAULT_COLOUR);
    tile.face.style.background = "color-mix(in srgb, var(--tile-color) 22%, transparent)";
    tile.items.textContent = summary.items ? String(summary.items) : "";
    tile.node.title = summary.sources && summary.sources.length ? summary.sources.join(", ") : "No inputs in this scene yet";
    tile.dot.className = "dot " + (summary.items ? "live" : "");

    // The item chips: each one is draggable into another scene, which is the
    // `scene.item.move` half of the grammar. They are marked `data-nodrag` so
    // the tile's own drag pipeline leaves them to the handler below.
    const records = this.scenes.mirror.items(summary.id);
    const ids = records.map((r) => r.id).join(",");
    if (ids !== tile.chipIds) {
      tile.chipIds = ids;
      clear(tile.chips);
      for (const record of records.slice(0, 6)) {
        tile.chips.appendChild(
          el("span.pill.sm", {
            text: nameOfItem(record),
            "data-nodrag": "",
            "data-item": record.id,
            "data-scene": summary.id,
            title: "Drag onto another scene to move it there. Hold Alt to copy.",
            style: { cursor: "grab", maxWidth: "10ch", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
            onpointerdown: (e) => this.dragChip(e, summary.id, record.id),
          })
        );
      }
    }
  }

  paintSelection() {
    for (const [id, tile] of this.tiles) tile.node.classList.toggle("selected", this.selection.has(id));
  }

  /** The red frame on air and the amber one armed, from the core's own view. */
  paintTally() {
    if (this.workspaceActive === false) return;
    // The live scene is `scene`; `program` carries a source id when the
    // programme is a single source, which is a one item scene's shorthand.
    // `live()` resolves the name the core froze at take time back to an id, or
    // the red frame falls off the scene the moment somebody renames it.
    const program = this.scenes.live() || this.client.state.scene || this.client.state.program;
    const armed = this.client.state.preview || this.scenes.armed();
    // The core names a scene by id or by name, depending on which command put
    // it there, so both are worth comparing.
    const is = (id, who) => {
      const summary = this.scenes.summary(id);
      return who === id || (summary && who === summary.name);
    };
    for (const [id, tile] of this.tiles) {
      tile.node.classList.toggle("program", is(id, program));
      tile.node.classList.toggle("armed", is(id, armed));
    }
    const focused = focusedScene([...this.tabs.keys()]);
    for (const [id, tab] of this.tabs) {
      tab.classList.toggle("program", is(id, program));
      tab.classList.toggle("armed", is(id, armed));
      tab.classList.toggle("on", id === focused);
      tab.setAttribute("aria-selected", id === focused ? "true" : "false");
    }
  }

  // --------------------------------------------------------------- actions

  selected() {
    return this.selection.list([...this.tiles.keys()]);
  }

  async activate(id) {
    if (settings().producer) {
      try {
        await this.scenes.arm(id);
      } catch (e) {
        errorToast(e, "Arm");
      }
      this.render();
      return;
    }
    this.scenes.take(id).catch((e) => errorToast(e, "Take"));
  }

  /** The composer, on a draft. Loaded on first open and never before. */
  async open(id) {
    const summary = this.scenes.summary(id);
    if (!summary) return;
    try {
      const { openComposer } = await import("../composer/composer.js");
      // Kept so the panel can close it, and so the DOM harness can drive it.
      this.composer = await openComposer({ client: this.client, scenes: this.scenes, scene: summary.id });
    } catch (e) {
      errorToast(e, "Composer");
    }
  }

  async newScene() {
    try {
      const made = await this.scenes.add("Scene");
      this.scenes.undo.record("Added a scene");
      await this.scenes.refresh();
      if (made && made.id) this.beginRename(made.id);
    } catch (e) {
      errorToast(e, "New scene");
    }
  }

  /**
   * Edit a scene's name in place. Answers whether there was a tile to edit.
   *
   * The name is edited on the tile, so a page showing the tab strip is turned
   * back to tiles first: nothing in a strip of tabs can hold a caret.
   */
  beginRename(id) {
    const tile = this.tiles.get(id);
    if (!tile) return false;
    if (this.grid.hidden) this.setView("tiles");
    const before = tile.name.textContent;
    tile.name.contentEditable = "true";
    tile.name.focus();
    const range = document.createRange();
    range.selectNodeContents(tile.name);
    const sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(range);

    // Escape ends the edit but the blur that follows it is still to come, and
    // a second finish wrote the old name back over a rename that had already
    // landed. Whichever of the two arrives first is the one that counts.
    let done = false;
    const finish = async (commit) => {
      if (done) return;
      done = true;
      tile.name.contentEditable = "false";
      const after = tile.name.textContent.trim();
      if (!commit || !after || after === before) {
        tile.name.textContent = before;
        return;
      }
      try {
        await this.scenes.rename(id, { name: after });
        this.scenes.undo.record(`Renamed to ${after}`);
      } catch (e) {
        tile.name.textContent = before;
        errorToast(e, "Rename");
      }
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
    return true;
  }

  /**
   * The scene F2 would rename, for a caller that has no tile under its hand.
   *
   * The selected one first, then the scene the operator says they are working
   * on, then the one on air. The tab strip has no selection at all, which is
   * why the palette's own entry did nothing on the view this page opens in.
   */
  renameTarget() {
    const ids = [...this.tiles.keys()];
    return this.selected()[0] || focusedScene(ids) || this.scenes.live() || null;
  }

  /** The palette's "Rename a scene": find one, select it, edit its name. */
  renameScene() {
    const id = this.renameTarget();
    if (!id || !this.tiles.has(id)) {
      toast({ text: "There is no scene to rename. Make one with New scene, then rename it with F2." });
      return;
    }
    this.selection.set([id]);
    this.paintSelection();
    this.beginRename(id);
  }

  async setColour(ids, colour) {
    for (const id of ids) {
      try {
        await this.scenes.rename(id, { color: colour });
      } catch (e) {
        errorToast(e, "Colour");
        return;
      }
    }
    this.scenes.undo.record(ids.length === 1 ? "Recoloured a scene" : `Recoloured ${ids.length} scenes`);
  }

  async remove(ids) {
    if (!ids.length) return;
    const names = ids.map((id) => (this.scenes.summary(id) || {}).name || id);
    for (const id of ids) {
      try {
        await this.scenes.remove(id);
      } catch (e) {
        errorToast(e, "Remove");
        return;
      }
    }
    // Undo is the core's: `scene.remove` is on its history stack, so taking it
    // back is one call and the items come back with their ids intact.
    this.scenes.undo.record(names.length === 1 ? `Removed ${names[0]}` : `Removed ${names.length} scenes`, { offer: true });
    this.selection.clear();
    await this.scenes.refresh();
  }

  // ----------------------------------------------------------------- drops

  /**
   * Everything that lands on this panel, from here or from the tray.
   *
   * The tray's own pointer pipeline finds the drop target under the pointer
   * anywhere on the page, so a drag that starts over an input and ends over a
   * scene arrives here as an event rather than through a shared object.
   */
  async dropped(info) {
    if (!info || !info.target) return;
    const target = String(info.target);
    if (target === "scenes:empty") {
      if (info.from === "scenes") return;
      await this.createFrom(info.ids || []);
      return;
    }
    if (!target.startsWith("scene:")) return;
    const scene = target.slice("scene:".length);
    if (info.item) {
      const more = await this.more();
      await more.moveItem(this, info, scene);
      return;
    }
    if (info.from === "scenes") return;
    await this.addSources(scene, info.ids || []);
  }

  /** Two or more inputs on empty space: a scene, laid out by count, no dialog. */
  async createFrom(sources) {
    if (!sources.length) return;
    try {
      const made = await this.scenes.createFrom(sources);
      this.scenes.undo.record(sources.length === 1 ? "Made a scene" : `Made a scene from ${sources.length} inputs`);
      await this.scenes.refresh();
      if (made && made.id) {
        this.selection.set([made.id]);
        this.paintSelection();
        toast({ text: `Made "${made.name}". Tap it to put it on air, double tap to arrange it.` });
      }
    } catch (e) {
      errorToast(e, "New scene");
    }
  }

  async addSources(scene, sources) {
    if (!sources.length) return;
    try {
      for (const source of sources) await this.scenes.itemAdd(scene, { source });
      this.scenes.undo.record(sources.length === 1 ? "Added an input to a scene" : `Added ${sources.length} inputs to a scene`, { offer: true });
      await this.scenes.reread([scene]);
    } catch (e) {
      errorToast(e, "Add to scene");
    }
  }

  /**
   * One item chip, dragged. Not the tile pipeline: a chip is a single thing,
   * never part of a multiple selection, and pointer capture on the chip is the
   * shortest path from a press to a drop.
   */
  dragChip(e, scene, item) {
    if (e.button !== 0) return;
    e.stopPropagation();
    const chip = e.currentTarget;
    const ghost = el("div.toast", { text: chip.textContent });
    Object.assign(ghost.style, { position: "fixed", left: "0", top: "0", zIndex: "90", pointerEvents: "none" });
    let moved = false;
    try {
      chip.setPointerCapture(e.pointerId);
    } catch {
      /* a browser that refuses capture still delivers move and up */
    }
    const move = on(window, "pointermove", (ev) => {
      if (!moved && Math.abs(ev.clientX - e.clientX) + Math.abs(ev.clientY - e.clientY) < 5) return;
      if (!moved) {
        moved = true;
        document.body.appendChild(ghost);
      }
      ghost.style.transform = `translate(${ev.clientX + 12}px, ${ev.clientY + 12}px)`;
    });
    const up = on(window, "pointerup", (ev) => {
      move();
      up();
      ghost.remove();
      if (!moved) return;
      const under = document.elementFromPoint(ev.clientX, ev.clientY);
      const drop = under && under.closest ? under.closest("[data-drop]") : null;
      if (!drop) return;
      this.dropped({ target: drop.dataset.drop, item, scene, copy: ev.altKey, from: "scenes" });
    });
  }

  // -------------------------------------------------------------- keyboard

  /**
   * The keys this panel owns while it has focus.
   *
   * Stopped here rather than routed through the shell's map, because F2 means
   * "rename the thing I am looking at" and the tray wants the same chord. A
   * panel that has the focus gets the key; nothing else has to be arbitrated.
   */
  key(e) {
    const accel = e.ctrlKey || e.metaKey;
    const ids = this.selected();
    const one = ids.length === 1 ? ids[0] : null;
    const chord = accel ? "Ctrl+" + e.key.toUpperCase() : e.key;
    const table = {
      F2: () => one && this.beginRename(one),
      Enter: () => one && this.open(one),
      Delete: () => ids.length && this.remove(ids),
      "Ctrl+BACKSPACE": () => ids.length && this.remove(ids),
      Escape: () => {
        this.selection.clear();
        this.paintSelection();
      },
      "Ctrl+A": () => {
        this.selection.selectAll([...this.tiles.keys()]);
        this.paintSelection();
      },
      "Ctrl+C": () => this.more().then((m) => m.copy(this, ids)),
      "Ctrl+V": () => this.more().then((m) => m.paste(this)),
    };
    const run = table[chord];
    if (!run) return;
    e.preventDefault();
    e.stopPropagation();
    Promise.resolve(run()).catch((err) => errorToast(err, "Scenes"));
  }

  // ------------------------------------------------------------ menu, keys

  /**
   * Everything past the two minute path: the menu, the clipboard, layouts and
   * moving an item between scenes. Fetched the first time one of them is asked
   * for, which on most days is never.
   */
  more() {
    return import("./more.js");
  }

  commands() {
    const one = () => this.selected()[0] || null;
    return [
      { id: "scenes.new", title: "New scene", group: "Scenes", run: () => this.newScene() },
      { id: "scenes.open", title: "Open the composer", group: "Scenes", enabled: () => !!one(), run: () => this.open(one()) },
      // No `enabled`: it finds a scene for itself, and says so in a toast when
      // there is none. A row that reads as available has to do something.
      { id: "scenes.rename", title: "Rename a scene", group: "Scenes", key: "F2", run: () => this.renameScene() },
      { id: "scenes.remove", title: "Remove the selected scenes", group: "Scenes", key: "Delete", enabled: () => this.selected().length > 0, run: () => this.remove(this.selected()) },
      { id: "scenes.duplicate", title: "Duplicate a scene", group: "Scenes", enabled: () => !!one(), run: () => this.more().then((m) => m.duplicate(this, this.selected())) },
      { id: "scenes.copy-layout", title: "Copy a scene's layout", group: "Scenes", enabled: () => !!one(), run: () => this.more().then((m) => m.copyLayout(this, one())) },
      { id: "scenes.paste-layout", title: "Paste a layout onto the selection", group: "Scenes", enabled: () => !!this.layoutClip, run: () => this.more().then((m) => m.pasteLayout(this, this.selected())) },
      { id: "scenes.arm", title: "Arm the selected scene", group: "Programme", enabled: () => !!one(), run: () => this.scenes.arm(one()).catch((e) => errorToast(e, "Arm")) },
      { id: "scenes.undo", title: "Undo the last scene change", group: "Scenes", run: () => this.scenes.undo.undo().catch((e) => errorToast(e, "Undo")) },
      { id: "scenes.redo", title: "Redo the last scene change", group: "Scenes", run: () => this.scenes.undo.redo().catch((e) => errorToast(e, "Redo")) },
    ];
  }
}

/**
 * What the pencil does, said in the composer's own words.
 *
 * It sits beside the name and reads as rename to nearly everyone, so the
 * tooltip and the label a screen reader announces both have to say layout.
 */
function editLabel(name) {
  return `Edit the layout of ${name} in the composer`;
}

/** A tooltip and the label a screen reader reads, kept the same on purpose. */
function label(node, text) {
  node.title = text;
  node.setAttribute("aria-label", text);
}

/** An item's name, or the source it draws, or something legible either way. */
function nameOfItem(record) {
  if (record.name) return record.name;
  const content = record.content || {};
  return content.source || content.graphic || content.ref || "item";
}

customElements.define("gmx-scenes", ScenesPanel);
window.godwinmixPanels.push(ScenesPanel);
export default ScenesPanel;
