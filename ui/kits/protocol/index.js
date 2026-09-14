// The protocol kit: one object a scene surface talks to.
//
// It holds the mirror (mirror.js), the undo proxy (undo.js) and the small set
// of commands the tile grammar of 05 section 3a is made of. Everything it does
// is a `scene.*` call on the one public protocol; there is no privileged path
// for the first party UI, which is what lets the Tkinter designer and the
// agent do the same things (11 section 4).
//
// What is deliberately not here: geometry commands and per item filters. They
// belong to the composer, they are loaded with it, and a page that never opens
// the composer never fetches them.

import { SceneMirror } from "./mirror.js";
import { UndoProxy } from "./undo.js";

export { SceneMirror, geometryIndex } from "./mirror.js";
export { UndoProxy } from "./undo.js";

export class SceneClient {
  /**
   * @param {object} client an @godwinmix/client instance
   * @param {{undo?: object}} opts the shell's undo stack, when there is one
   */
  constructor(client, opts = {}) {
    this.client = client;
    this.mirror = new SceneMirror();
    this.undo = new UndoProxy(client, opts.undo || { push() {} });
    /** scene id -> the last view the core answered with, for its geometry. */
    this.views = new Map();
    /** `scene.list` summaries: name, colour, item count, armed. */
    this.summaries = [];
    this.listeners = new Set();
    this.dirty = false;
    this.offs = [];
    this.supported = true;
  }

  // ------------------------------------------------------------- lifecycle

  /**
   * Learn who we are, take a snapshot, and follow the patches after it.
   *
   * The snapshot is `scene.list` and one `scene.get` per scene, because a
   * summary has no records in it. After that nothing is refetched: patches are
   * applied, and every mutating answer is a view that lands in the mirror.
   */
  async start() {
    try {
      const info = await this.client.call("core.info", {});
      if (info && info.token && info.token.id) this.mirror.setClientId(info.token.id);
    } catch {
      // A core that will not say who we are costs us echo suppression and
      // nothing else: the mirror still converges on what the core sends.
    }
    this.offs.push(
      this.client.on("event", ({ name, params }) => {
        if (name === "scene.patch") this.onPatch(params);
        else if (name === "flush") this.settle();
        else if (name === "preview.changed" || name === "program.took") this.changed();
        else if (name === "resync") this.refresh();
      })
    );
    await this.refresh();
    return this;
  }

  stop() {
    for (const off of this.offs) off();
    this.offs = [];
    this.listeners.clear();
  }

  /** Called whenever the document changed. Returns the removal function. */
  onChange(fn) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  changed() {
    this.dirty = true;
    // Events arrive in batches ended by `event/flush`, which is where a panel
    // repaints. A core that sends a patch outside a batch still gets drawn, a
    // tick later, rather than waiting for the next thing to happen.
    if (this.timer) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      this.settle();
    }, 16);
  }

  settle() {
    if (!this.dirty) return;
    this.dirty = false;
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    for (const fn of this.listeners) {
      try {
        fn(this);
      } catch (e) {
        console.error("a scene listener threw", e);
      }
    }
  }

  onPatch(patch) {
    const out = this.mirror.applyPatch(patch);
    if (out.gap) {
      // Events were dropped. A document with a hole in it is worse than a
      // second of staleness, so it is thrown away and read again.
      this.refresh();
      return out;
    }
    if (this.onEcho) this.onEcho(patch, out);
    if (out.applied) this.changed();
    return out;
  }

  /** The whole document again: after a resync, or on a core with no patches. */
  async refresh() {
    let list;
    try {
      list = await this.client.call("scene.list", {});
    } catch (e) {
      // A core with no scene server. Every caller checks `supported` and shows
      // the sentence rather than a broken panel.
      if (e && e.code === -32601) this.supported = false;
      this.summaries = [];
      this.changed();
      return [];
    }
    this.supported = true;
    this.summaries = (list && list.scenes) || [];
    const views = [];
    for (const summary of this.summaries) {
      try {
        views.push(await this.client.call("scene.get", { scene: summary.id }));
      } catch {
        /* a scene that went away between the list and the read */
      }
    }
    this.mirror.reset(views);
    this.views.clear();
    for (const view of views) this.views.set(view.id, view);
    this.changed();
    return this.summaries;
  }

  // --------------------------------------------------------------- reading

  scenes() {
    return this.summaries;
  }

  summary(id) {
    return this.summaries.find((s) => s.id === id || s.name === id) || null;
  }

  view(id) {
    return this.views.get(id) || null;
  }

  /** The armed scene's id, from the summaries, or null. */
  armed() {
    const one = this.summaries.find((s) => s.armed);
    return one ? one.id : null;
  }

  // -------------------------------------------------------------- commands

  /**
   * Every mutating call goes through here so the answer lands in the mirror.
   * A command that answers with a view updates that scene; one that answers
   * with `{changed}` leaves the mirror to the patch that follows.
   */
  async call(method, params) {
    const answer = await this.client.call(method, params || {});
    this.take(answer);
    return answer;
  }

  /** File a view a command answered with. */
  take(answer) {
    if (!answer || !answer.id || !Array.isArray(answer.records)) return;
    this.views.set(answer.id, answer);
    this.mirror.applyView(answer);
    const summary = this.summary(answer.id);
    if (summary) {
      summary.name = answer.name;
      summary.color = answer.color === undefined ? summary.color : answer.color;
      summary.items = answer.records.filter((r) => r.kind === "item").length;
    }
    this.changed();
  }

  take_(scene) {
    return this.client.call("program.take", { scene });
  }

  arm(scene) {
    return this.client.call("scene.preview.set", scene ? { scene } : {});
  }

  createFrom(sources, name) {
    return this.call("scene.create_from", name ? { sources, name } : { sources });
  }

  add(name) {
    return this.call("scene.add", { name });
  }

  rename(scene, fields) {
    return this.call("scene.rename", Object.assign({ scene }, fields));
  }

  duplicate(scene, name) {
    return this.call("scene.duplicate", name ? { scene, name } : { scene });
  }

  async remove(scene) {
    const answer = await this.client.call("scene.remove", { scene });
    this.views.delete(scene);
    await this.refresh();
    return answer;
  }

  itemAdd(scene, content, extra) {
    return this.call("scene.item.add", Object.assign({ scene, content }, extra || {}));
  }

  itemRemove(scene, item) {
    return this.call("scene.item.remove", { scene, item });
  }

  /**
   * One item's properties. `seq` is the client's own number, echoed on the
   * patch, so a drag can discard the echoes of moves it has drawn past.
   */
  itemSet(scene, item, props, opts = {}) {
    const params = { scene, item, props, duration_ms: opts.duration_ms ?? 0 };
    if (opts.seq) params.seq = opts.seq;
    if (opts.draft) params.draft = opts.draft;
    if (opts.easing) params.easing = opts.easing;
    return this.call("scene.item.set", params);
  }

  async itemMove(scene, item, toScene, copy) {
    const answer = await this.client.call(copy ? "scene.item.copy" : "scene.item.move", {
      scene,
      item,
      to_scene: toScene,
    });
    this.take(answer);
    // Two scenes changed and the answer describes one of them.
    await this.reread([scene, toScene]);
    return answer;
  }

  layoutCopy(scene) {
    return this.client.call("scene.layout.copy", { scene });
  }

  layoutPaste(scene, layout, match) {
    return this.call("scene.layout.paste", { scene, layout, match: match || "name" });
  }

  /** Read these scenes again, for the commands that touch more than one. */
  async reread(ids) {
    for (const id of new Set(ids.filter(Boolean))) {
      try {
        this.take(await this.client.call("scene.get", { scene: id }));
      } catch {
        /* gone, and the next refresh will say so */
      }
    }
    this.changed();
  }

  // ----------------------------------------------------------------- drafts

  editBegin(scene, live) {
    return this.client.call("scene.edit.begin", live ? { scene, live: true } : { scene });
  }

  async editApply(draft) {
    const answer = await this.client.call("scene.edit.apply", { draft });
    this.take(answer);
    return answer;
  }

  editDiscard(draft) {
    return this.client.call("scene.edit.discard", { draft });
  }
}
