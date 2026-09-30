// A live picture on each scene's tab and tile, drawn in the page.
//
// Nothing extra is encoded on the mixer for this. The page already decodes the
// multiview sheet for the input tiles, and each scene is put together here from
// the cells of that sheet, at the boxes the core worked out for the scene's
// items, in the order the compositor stacks them. The mosaic is asked for at a
// low rate and a small width while the Scenes panel is on screen with live
// pictures on, and given back the moment either stops (01, principle 2).
//
// Fetched the first time the panel is on screen with live pictures on, so a
// page on icons never loads it.

import { el } from "../../shell/dom.js";
import { settings } from "../../shell/settings.js";
import { sheetWidthFor } from "../../client/frames.js";
import { drawScene } from "./draw.js";

/** Frames a second for scene pictures, whatever the tiles are asking for. */
export const RATE = 3;
/** The width in CSS pixels a sheet cell is sized for, when only scenes want it. */
const CELL = 120;

export class ScenePictures {
  constructor(panel) {
    if (!document.querySelector('link[href$="/scenes/pictures.css"]')) {
      document.head.appendChild(el("link", { rel: "stylesheet", href: new URL("./pictures.css", import.meta.url).href }));
    }
    this.panel = panel;
    this.client = panel.client;
    this.want = null;
    this.off = null;
    this.last = 0;
    this.stale = new Set();
    this.tried = new Map();
  }

  /** On screen, live pictures chosen, a mosaic to draw from, a scene to draw. */
  wanted() {
    const p = this.panel;
    const mv = this.client.state.multiview;
    return settings().gallery === "live" && p.visible !== false && p.workspaceActive !== false &&
      !document.hidden && !!(mv && mv.enabled) && p.scenes.scenes().length > 0;
  }

  /** Ask for the mosaic or give it back, and show or hide the pictures. */
  tune() {
    const on = this.wanted();
    if (on && !this.want) {
      const cols = this.client.state.multiview.cols || 1;
      this.want = this.client.want("multiview", { fps: RATE, width: sheetWidthFor(CELL, cols) });
      this.off = this.client.sheet.observe((bitmap, layout) => this.frame(bitmap, layout));
    } else if (!on && this.want) {
      this.stop();
    }
    this.panel.classList.toggle("scene-pictures", on);
    if (on) this.sync();
  }

  stop() {
    if (this.want) this.want.release();
    if (this.off) this.off();
    this.want = this.off = null;
  }

  /** A canvas in every tab and tile, made the first time each is seen. */
  sync() {
    for (const tab of this.panel.tabs.values()) {
      if (!tab.querySelector(":scope > canvas")) tab.prepend(el("canvas.scene-pic", { width: 96, height: 54, "aria-hidden": "true" }));
    }
    for (const tile of this.panel.tiles.values()) {
      if (!tile.face.querySelector(":scope > canvas")) tile.face.prepend(el("canvas.scene-pic", { width: 320, height: 180, "aria-hidden": "true" }));
    }
    this.check();
  }

  /**
   * A scene whose items changed since its boxes were read (an undo, another
   * browser, an agent) is read again, so the picture never draws a layout the
   * scene no longer has. Only while the pictures are up.
   */
  check() {
    const { scenes } = this.panel;
    for (const s of scenes.scenes()) {
      const view = scenes.view(s.id);
      const had = view ? view.records.filter((r) => r.kind === "item").map((r) => r.id).sort().join() : "";
      const has = scenes.mirror.descendants(s.id).map((r) => r.id).sort().join();
      // Once per state of the items, so two lists that never agree cost one
      // read and not a loop.
      if (had !== has && this.tried.get(s.id) !== has) {
        this.tried.set(s.id, has);
        this.stale.add(s.id);
      }
    }
    if (!this.stale.size || this.reading) return;
    const ids = [...this.stale];
    this.stale.clear();
    this.reading = scenes.reread(ids).finally(() => { this.reading = null; });
  }

  /** One sheet: every scene drawn from it, no faster than RATE. */
  frame(bitmap, layout) {
    const now = performance.now();
    if (now - this.last < 1000 / RATE - 30) return;
    this.last = now;
    const { scenes, tabs, tiles, grid, strip } = this.panel;
    const cells = new Map((layout.cells || []).filter((c) => c.source).map((c) => [c.source, c]));
    for (const s of scenes.scenes()) {
      const view = scenes.view(s.id);
      if (!view) continue;
      const tab = tabs.get(s.id);
      const tile = tiles.get(s.id);
      const on = [];
      if (tab && !strip.hidden) on.push(tab.querySelector(":scope > canvas"));
      if (tile && !grid.hidden) on.push(tile.face.querySelector(":scope > canvas"));
      for (const canvas of on) if (canvas) drawScene(canvas, view, scenes.mirror, cells, bitmap);
    }
  }

  destroy() {
    this.stop();
    this.panel.classList.remove("scene-pictures");
  }
}
