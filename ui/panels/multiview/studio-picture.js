// The picture in Studio mode's preview pane: the armed scene from the core's
// preview stream, or a single source from its own tile in the mosaic. Its own
// module so studio.js stays about the controls.

import { sheetWidthFor } from "../../client/frames.js";
import { settings } from "../../shell/settings.js";
import { armedScene } from "./studio-armed.js";

/** The mosaic cell a source is drawn in, or null. */
function cellOf(s, source) {
  const cells = (s.layout && s.layout.cells) || (s.multiview && s.multiview.cells) || [];
  const cell = cells.find((c) => c.source === source);
  return cell ? cell.index : null;
}

/**
 * The picture in the preview pane.
 *
 * A scene is its own subscription: `ext.preview` is what builds the
 * compositor, asked for only in Studio mode, with a scene armed, and while
 * somebody can see it. A source is its tile in the mosaic, which costs
 * nothing more than the programme monitor already pays.
 */
export function retunePreview(panel, s) {
  const seen = settings().producer && panel.visible && panel.workspaceActive !== false && !document.hidden;
  const scene = seen && armedScene(s);
  const source = seen && !scene && document.body.dataset.armed;
  const cell = source ? cellOf(s, source) : null;
  if (!seen || (!scene && cell === null)) {
    releasePreview(panel);
    return;
  }
  if (cell !== null) {
    releaseStream(panel);
    if (panel.previewCell !== cell) {
      detach(panel);
      panel.detachPreview = panel.client.sheet.attach(panel.previewCanvas, cell);
      panel.previewCell = cell;
    }
    size(panel);
    return;
  }
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

function detach(panel) {
  if (panel.detachPreview) panel.detachPreview();
  panel.detachPreview = null;
  panel.previewCell = undefined;
}

export function releasePreview(panel) {
  releaseStream(panel);
  detach(panel);
}
