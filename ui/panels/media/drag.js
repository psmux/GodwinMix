// Dragging files and folders onto a folder, with pointer events rather than
// the browser's own drag and drop, which the page keeps for files coming in
// from outside (see fileDrop in shell.js).
//
// A mouse or a pen drags. A finger scrolls the list instead, and moves
// things with "Move to" on the long press menu.

import { el } from "../../shell/dom.js";

const START_PX = 6;

/**
 * Watch `root` for drags that start on `[data-drag-media]` (a file row) or
 * `[data-drag-folder]` (a folder node) and end on `[data-folder]`.
 * `what(node)` says what is carried: {kind: "files", names} or
 * {kind: "folder", path}. `drop(carried, folder)` does the move.
 */
export function dragging(root, what, drop) {
  let press = null;
  let ghost = null;
  let over = null;
  let quietUntil = 0;
  const quiet = (c) => {
    if (Date.now() < quietUntil) c.stopPropagation();
  };

  const target = (x, y) => {
    const hit = document.elementFromPoint(x, y);
    return hit && hit.closest ? hit.closest("[data-folder]") : null;
  };
  const mark = (node) => {
    if (over === node) return;
    over?.classList.remove("mx-over");
    over = node;
    over?.classList.add("mx-over");
  };
  const end = () => {
    ghost?.remove();
    ghost = null;
    mark(null);
    press = null;
  };

  const down = (e) => {
    if (e.button !== 0 || e.pointerType === "touch" || e.target.closest("button, input, select, a, label")) return;
    const source = e.target.closest("[data-drag-media], [data-drag-folder]");
    if (!source || !root.contains(source)) return;
    press = { x: e.clientX, y: e.clientY, source, carried: null };
  };
  const move = (e) => {
    if (!press) return;
    if (!ghost) {
      if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < START_PX) return;
      press.carried = what(press.source);
      if (!press.carried) return end();
      const label = press.carried.kind === "files" ? (press.carried.names.length > 1 ? `${press.carried.names.length} files` : press.carried.names[0].split("/").pop()) : press.carried.path.split("/").pop();
      ghost = el("div.mx-ghost", { text: label });
      document.body.appendChild(ghost);
    }
    e.preventDefault();
    ghost.style.left = `${e.clientX + 12}px`;
    ghost.style.top = `${e.clientY + 8}px`;
    mark(target(e.clientX, e.clientY));
  };
  const up = (e) => {
    if (!press) return;
    const dragged = !!ghost;
    const carried = press.carried;
    const onto = dragged ? target(e.clientX, e.clientY) : null;
    end();
    if (!dragged) return;
    // The click that follows a drag is not a click on the row.
    quietUntil = Date.now() + 80;
    if (onto) drop(carried, onto.dataset.folder);
  };

  root.addEventListener("pointerdown", down);
  window.addEventListener("click", quiet, true);
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", end);
  return () => {
    root.removeEventListener("pointerdown", down);
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", end);
    window.removeEventListener("click", quiet, true);
    end();
  };
}
