import * as model from './dock-model.js';
import { splitter } from './dock-pointer.js';
import { place } from './dock.js';

export function positionWorkspace(workspace) {
  const box = { x: 0, y: 0, w: workspace.root.clientWidth, h: workspace.root.clientHeight };
  const result = model.rectangles(workspace.state.tree, box);
  for (const rect of result.panels) {
    const frame = workspace.frames.get(rect.node.active);
    if (frame) place(frame.element, rect);
  }
  const current = new Set(result.splits.map(r => r.node));
  for (const [tree, entry] of workspace.splits) {
    if (!current.has(tree)) { entry.element.remove(); workspace.splits.delete(tree); }
  }
  for (const rect of result.splits) {
    let entry = workspace.splits.get(rect.node);
    if (!entry) { entry = { element: splitter(workspace, rect), rect }; workspace.splits.set(rect.node, entry); }
    Object.assign(entry.rect, rect);
    place(entry.element, rect);
    entry.element.setAttribute('aria-valuenow', Math.round(rect.node.ratio * 100));
  }
}
