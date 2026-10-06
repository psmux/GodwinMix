// The picture in Studio mode's preview pane. Its own module so studio.js
// stays about the controls. Three ways to fill it, by what is in preview
// (studio-next.js):
//
// * a scene armed on the core: the core's preview stream. `ext.preview` is
//   what builds that compositor, asked for only in Studio mode, with a scene
//   armed, and while somebody can see it;
// * a single source: its own tile out of the mosaic the programme monitor
//   already receives, which costs nothing more;
// * a suggested scene, which nobody armed: drawn on this page out of that
//   same mosaic, cell by cell at the scene's boxes, as the Scenes panel draws
//   its pictures. Nothing more is asked of the core for it.

import { sheetWidthFor } from "../../client/frames.js";
import { settings } from "../../shell/settings.js";
import { drawScene } from "../scenes/draw.js";
import { sceneKit } from "./studio-next.js";
import { resolve } from "./studio-armed.js";

/** The mosaic cell a source is drawn in, or null. */
function cellOf(s, source) {
  const cells = (s.layout && s.layout.cells) || (s.multiview && s.multiview.cells) || [];
  const cell = cells.find((c) => c.source === source);
  return cell ? cell.index : null;
}

export function retunePreview(panel, s) {
  const seen = settings().producer && panel.visible && panel.workspaceActive !== false && !document.hidden;
  const next = seen ? resolve(panel, s) : null;
  if (!next) return releasePreview(panel);
  if (next.kind === "source") return showTile(panel, cellOf(s, next.id));
  if (next.why !== "armed") return showDrawn(panel);
  showStream(panel);
}

function showTile(panel, cell) {
  releaseStream(panel);
  stopDrawing(panel);
  if (cell === null) return detach(panel);
  if (panel.previewCell !== cell) {
    detach(panel);
    panel.detachPreview = panel.client.sheet.attach(panel.previewCanvas, cell);
    panel.previewCell = cell;
  }
  size(panel);
}

/** A suggestion, put together from the mosaic each time a sheet arrives. */
function showDrawn(panel) {
  releaseStream(panel);
  detach(panel);
  size(panel);
  if (panel.offDrawn) return;
  panel.offDrawn = panel.client.sheet.observe((bitmap, layout) => {
    const kit = sceneKit(panel);
    const next = panel.next;
    const view = kit && next && next.kind === "scene" ? kit.view(next.id) : null;
    if (!view) return;
    const cells = new Map((layout.cells || []).filter((c) => c.source).map((c) => [c.source, c]));
    drawScene(panel.previewCanvas, view, kit.mirror, cells, bitmap);
  });
}

function showStream(panel) {
  stopDrawing(panel);
  if (panel.previewCell !== undefined) detach(panel);
  const box = panel.previewCanvas.getBoundingClientRect();
  const fps = settings().multiviewFps;
  // One picture rather than a sheet, so one cell across.
  const width = panel.resizeTimer && panel.previewWant ? panel.lastPreviewWidth : sheetWidthFor(box.width || 320, 1);
  if (!panel.previewWant) panel.previewWant = panel.client.want("preview", { fps, width });
  else if (width !== panel.lastPreviewWidth || fps !== panel.lastPreviewFps) {
    panel.previewWant.update({ fps, width });
  }
  panel.lastPreviewWidth = width;
  panel.lastPreviewFps = fps;
  size(panel);
  if (!panel.detachPreview) panel.detachPreview = panel.client.preview.attach(panel.previewCanvas);
}

/** The backing store follows the element at the device's pixel ratio. */
function size(panel) {
  const box = panel.previewCanvas.getBoundingClientRect();
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const w = Math.round((box.width || 320) * dpr);
  const h = Math.round((w * 9) / 16);
  if (!panel.resizeTimer && w > 0 && (panel.previewCanvas.width !== w || panel.previewCanvas.height !== h)) {
    panel.previewCanvas.width = w;
    panel.previewCanvas.height = h;
  }
}

function releaseStream(panel) {
  if (panel.previewWant) {
    panel.previewWant.release();
    panel.previewWant = null;
  }
}

function stopDrawing(panel) {
  if (panel.offDrawn) panel.offDrawn();
  panel.offDrawn = null;
}

function detach(panel) {
  if (panel.detachPreview) panel.detachPreview();
  panel.detachPreview = null;
  panel.previewCell = undefined;
}

export function releasePreview(panel) {
  releaseStream(panel);
  stopDrawing(panel);
  detach(panel);
}
