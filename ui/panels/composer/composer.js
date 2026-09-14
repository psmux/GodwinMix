// The composer: a scene, off air, on a copy.
//
// It opens as a modal on a draft (`scene.edit.begin`) and goes full screen with
// one more tap. Nothing it does reaches air until Apply, which is the OBS
// pitfall of editing the programme scene live turned into a choice: a graphics
// operator who wants that asks for it with the switch, and the switch says so.
//
// Everything in this file is loaded on first open and never before. A volunteer
// who runs a whole service from the tiles never fetches it, which is why the
// page stays inside its budget with a designer in it.

import { el, clear, on } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { ComposerCanvas } from "./canvas.js";
import { Inspector } from "./inspector.js";
import { Catalogue } from "./catalogue.js";
import { operations } from "./ops.js";

let styled = false;

/** The stylesheet travels with the module, so neither is in the eager set. */
function style() {
  if (styled) return;
  styled = true;
  document.head.appendChild(
    el("link", { rel: "stylesheet", href: new URL("./composer.css", import.meta.url).href })
  );
}

/**
 * @param {{client, scenes, scene: string, live?: boolean}} opts
 * @returns {Promise<Composer>}
 */
export async function openComposer(opts) {
  style();
  const composer = new Composer(opts);
  await composer.open();
  return composer;
}

export class Composer {
  constructor(opts) {
    this.client = opts.client;
    this.scenes = opts.scenes;
    this.scene = opts.scene;
    this.live = !!opts.live;
    this.draft = null;
    this.catalogue = new Catalogue(opts.client);
    this.selection = [];
  }

  // ------------------------------------------------------------------ open

  async open() {
    const begun = await this.begin();
    this.canvas = new ComposerCanvas({
      scenes: this.scenes,
      designerFor: (record) => this.catalogue.designerFor(record),
      onSelect: (ids) => this.select(ids),
      onDragStart: () => this.scenes.undo.mark("Moved an item"),
      onDragEnd: () => this.endDrag(),
      onView: (view) => this.useView(view),
      onError: (e) => errorToast(e, "Move"),
    });
    this.canvas.scene = this.scene;
    this.canvas.draft = this.draft;
    this.inspector = new Inspector({
      client: this.client,
      scenes: this.scenes,
      catalogue: this.catalogue,
      context: () => ({ scene: this.scene, draft: this.draft, items: this.selection }),
      onChanged: () => this.reread(),
    });

    this.dialog = modal({
      title: `Composer: ${begun.name || this.scene}`,
      wide: true,
      body: this.body(),
      footer: this.footer(),
      onClose: () => this.close(),
    });
    this.dialog.el.classList.add("composer");
    this.offs = [on(window, "keydown", (e) => this.key(e), true)];

    await this.catalogue.load();
    this.useView(begun.view || (await this.read()));
    this.canvas.resize();
    await this.picture();
    this.select([]);
    return this;
  }

  /** The working copy. A core with no drafts is edited directly, and says so. */
  async begin() {
    try {
      const begun = await this.scenes.editBegin(this.scene, this.live);
      this.draft = begun.draft || null;
      this.live = !!begun.live;
      return begun;
    } catch (e) {
      if (e && e.code === -32601) {
        toast({ kind: "warning", text: "This mixer has no drafts, so these edits go straight to the scene." });
        return { view: await this.read() };
      }
      throw e;
    }
  }

  read() {
    return this.client.call("scene.get", { scene: this.scene });
  }

  /**
   * Take the view a command answered with.
   *
   * Every mutating call answers with the records and the flattened geometry
   * (11 section 4), and a call carrying a `draft` answers with the draft's own
   * view. That is the only way to see a draft: it is nobody else's business
   * until it is applied, so `scene.get` does not show it and this is not a
   * shortcut but the contract.
   */
  useView(view) {
    if (!view || !Array.isArray(view.records)) return;
    this.view = view;
    this.canvas.setView(view);
    this.showInspector();
  }

  async reread() {
    if (this.draft) {
      // The answers already brought the draft up to date. Redrawing from the
      // scene would show what is on air rather than what is being edited.
      this.canvas.setView(this.view);
      this.showInspector();
      return;
    }
    this.useView(await this.read().catch(() => null));
  }

  body() {
    this.note = el("span.sm.faint.grow");
    this.bar = el("div.composer-bar.row", {}, [...this.tools(), el("span.grow"), this.note, ...this.toggles()]);
    const side = el("div.composer-side", {}, [this.inspector.el]);
    return el("div.composer-body", {}, [this.bar, this.canvas.el, side]);
  }

  footer() {
    const liveSwitch = el("input", { type: "checkbox", checked: this.live });
    on(liveSwitch, "change", () => this.setLive(liveSwitch.checked));
    return [
      el("label.inline", { title: "Edits reach the programme as you make them" }, [
        liveSwitch,
        el("span.sm", { text: "Edit on air" }),
      ]),
      el("span.grow"),
      el("button.btn", { text: "Discard", onclick: () => this.discard() }),
      el("button.btn.primary", { text: "Apply", onclick: () => this.apply() }),
    ];
  }

  /** The semantic commands, as buttons that cannot be off by twelve pixels. */
  tools() {
    const ops = operations(this.scenes, () => ({ scene: this.scene, draft: this.draft, items: this.selection }));
    this.opButtons = [];
    const groups = new Map();
    for (const op of ops) {
      if (!groups.has(op.group)) groups.set(op.group, []);
      const button = el("button.btn.sm", {
        text: op.title,
        title: `${op.group}: ${op.title}`,
        onclick: () =>
          this.scenes.undo
            .group(op.title, () => op.run())
            .then((answer) => {
              this.useView(answer);
              return this.reread();
            })
            .catch((e) => errorToast(e, op.title)),
      });
      this.opButtons.push({ op, button });
      groups.get(op.group).push(button);
    }
    return [...groups.entries()].map(([name, buttons]) =>
      el("div.composer-group", {}, [el("span.sm.faint", { text: name }), ...buttons])
    );
  }

  toggles() {
    const make = (label, key, on_) => {
      const input = el("input", { type: "checkbox", checked: on_ });
      on(input, "change", () => {
        this.canvas[key] = input.checked;
        this.canvas.draw();
      });
      return el("label.inline.sm", {}, [input, el("span", { text: label })]);
    };
    const full = el("button.btn.sm", {
      text: "Full screen",
      onclick: () => {
        this.dialog.el.classList.toggle("composer-full");
        this.canvas.resize();
      },
    });
    return [make("Safe areas", "safe", true), make("Rulers", "rulers", false), make("Snap", "snap", true), full];
  }

  // ------------------------------------------------------------- selection

  select(ids) {
    this.selection = ids;
    this.canvas.setSelection(ids);
    for (const { op, button } of this.opButtons || []) button.disabled = ids.length < op.min;
    this.showInspector();
  }

  showInspector() {
    const records = this.selection.map((id) => this.canvas.record(id)).filter(Boolean);
    this.inspector.show(records).catch((e) => console.error("the inspector threw", e));
  }

  async endDrag() {
    await this.scenes.undo.mark(null);
    this.scenes.undo.record("Moved an item");
    await this.reread();
  }

  // --------------------------------------------------------------- picture

  /**
   * What goes behind the handles, in the order 11 section 4 gives:
   *
   *   1. `/mjpeg/preview`, when the armed scene is the one being edited: this
   *      is the scene, composited, at preview rate.
   *   2. `scene.preview.frame`, a still on the RPC socket, which is the
   *      universal floor and needs a mosaic running.
   *   3. the programme picture, with this scene's layout drawn over it, so
   *      there is something true on screen to place items against.
   *
   * Each step says what you are looking at, because a designer who thinks they
   * are seeing their scene and is seeing the programme will place things wrong.
   */
  async picture() {
    clear(this.canvas.picture);
    const armed = this.client.state.preview || this.scenes.armed();
    const summary = this.scenes.summary(this.scene);
    const isArmed = summary && (armed === summary.id || armed === summary.name);
    if (isArmed) {
      this.canvas.picture.appendChild(el("img", { src: this.streamUrl("/mjpeg/preview"), alt: "" }));
      this.note.textContent = "The armed scene, live.";
      return;
    }
    try {
      const frame = await this.client.call("scene.preview.frame", { width: 960 });
      if (frame && frame.jpeg) {
        this.canvas.picture.appendChild(el("img", { src: `data:image/jpeg;base64,${frame.jpeg}`, alt: "" }));
        this.note.textContent = "A still of the armed scene. Arm this one to see it move.";
        return;
      }
    } catch {
      /* no mosaic up: the programme picture is the next best true thing */
    }
    this.canvas.picture.appendChild(el("img", { src: this.streamUrl("/mjpeg/program"), alt: "" }));
    this.note.textContent = "The programme, with this scene's layout drawn over it.";
  }

  streamUrl(path) {
    const transport = this.client.transport || {};
    const url = new URL(path, transport.base || location.origin);
    if (transport.token) url.searchParams.set("token", transport.token);
    return url.toString();
  }

  // ---------------------------------------------------------------- finish

  async setLive(wanted) {
    if (wanted === this.live) return;
    try {
      if (this.draft) await this.scenes.editDiscard(this.draft);
      this.live = wanted;
      const begun = await this.begin();
      this.canvas.draft = this.draft;
      this.useView(begun.view || (await this.read()));
      toast({
        text: wanted
          ? "Editing on air. Every change goes out as you make it."
          : "Editing a copy again. Nothing reaches air until Apply.",
      });
    } catch (e) {
      errorToast(e, "Edit on air");
    }
  }

  async apply() {
    try {
      if (this.draft) {
        await this.scenes.editApply(this.draft);
        this.draft = null;
        this.scenes.undo.record("Applied the composer's changes");
      }
      await this.scenes.reread([this.scene]);
      this.dialog.close();
      toast({ text: "Applied. Tap the scene to put it on air." });
    } catch (e) {
      errorToast(e, "Apply");
    }
  }

  async discard() {
    try {
      if (this.draft) await this.scenes.editDiscard(this.draft);
      this.draft = null;
      this.dialog.close();
    } catch (e) {
      errorToast(e, "Discard");
    }
  }

  key(e) {
    if (!this.dialog || !this.dialog.el.isConnected) return;
    if (e.target && (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA" || e.target.isContentEditable)) return;
    const step = e.shiftKey ? 10 : 1;
    const nudge = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }[e.key];
    if (nudge && this.selection.length) {
      e.preventDefault();
      this.nudge(nudge[0], nudge[1]);
      return;
    }
    if ((e.key === "Delete" || (e.key === "Backspace" && (e.metaKey || e.ctrlKey))) && this.selection.length) {
      e.preventDefault();
      this.removeSelected();
    }
  }

  async nudge(dx, dy) {
    for (const id of this.selection) {
      const record = this.canvas.record(id);
      if (!record) continue;
      const position = (record.transform && record.transform.position) || { x: 0, y: 0 };
      await this.scenes
        .itemSet(this.scene, id, { transform: { position: { x: position.x + dx, y: position.y + dy } } }, { duration_ms: 0, draft: this.draft })
        .catch((e) => errorToast(e, "Nudge"));
    }
    this.scenes.undo.record("Nudged an item");
    await this.reread();
  }

  async removeSelected() {
    const ids = this.selection.slice();
    for (const id of ids) {
      await this.scenes
        .call("scene.item.remove", Object.assign({ scene: this.scene, item: id }, this.draft ? { draft: this.draft } : {}))
        .catch((e) => errorToast(e, "Remove"));
    }
    this.scenes.undo.record(ids.length === 1 ? "Removed an item" : `Removed ${ids.length} items`, { offer: true });
    this.select([]);
    await this.reread();
  }

  close() {
    for (const off of this.offs || []) off();
    this.offs = [];
    if (this.canvas) this.canvas.destroy();
    // A draft nobody applied is nobody's business. Leaving it open would hold
    // a copy of the scene in the core until the process restarted.
    if (this.draft) this.scenes.editDiscard(this.draft).catch(() => {});
    this.draft = null;
  }
}
