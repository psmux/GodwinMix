// A sheet that opens from a button beside Take: the transition picker and the
// effects list. Fixed to the window rather than to the take bar, so it has the
// room it needs on a short dock and never pushes Take or Cut anywhere. On a
// phone it is a sheet along the bottom of the screen.
//
// One open at a time. A press outside it, or Escape, closes it.

import { on } from "../../shell/dom.js";

let current = null;

function clamp(v, lo, hi) {
  return Math.max(lo, Math.min(v, Math.max(lo, hi)));
}

/** Put the sheet beside its button, inside the window. */
function place(sheet, trigger) {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const phone = vw <= 600;
  sheet.classList.toggle("bottom", phone);
  if (phone) {
    sheet.style.left = sheet.style.top = sheet.style.width = "";
    return;
  }
  const box = trigger.getBoundingClientRect();
  const width = Math.min(Number(sheet.dataset.width) || 560, vw - 16);
  sheet.style.width = `${width}px`;
  sheet.style.left = `${clamp(box.left + box.width / 2 - width / 2, 8, vw - width - 8)}px`;
  sheet.style.top = `${clamp(box.top, 8, vh - sheet.offsetHeight - 8)}px`;
}

/**
 * Wire `sheet` to open from `trigger`. `onOpen` fills it, `onClose` empties
 * it, so whatever moves inside it moves only while it is open.
 */
export function popover(sheet, trigger, { onOpen, onClose } = {}) {
  sheet.hidden = true;
  sheet.classList.add("studio-sheet");
  trigger.setAttribute("aria-expanded", "false");
  let offs = [];

  const api = {
    get open() {
      return !sheet.hidden;
    },
    show() {
      if (current && current !== api) current.close();
      current = api;
      sheet.hidden = false;
      trigger.setAttribute("aria-expanded", "true");
      if (onOpen) onOpen();
      place(sheet, trigger);
      offs = [
        on(document, "pointerdown", (e) => {
          if (!sheet.contains(e.target) && !trigger.contains(e.target)) api.close();
        }, true),
        on(document, "keydown", (e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            api.close();
            trigger.focus();
          }
        }, true),
        on(window, "resize", () => place(sheet, trigger)),
      ];
    },
    close() {
      if (sheet.hidden) return;
      sheet.hidden = true;
      trigger.setAttribute("aria-expanded", "false");
      offs.forEach((off) => off());
      offs = [];
      if (current === api) current = null;
      if (onClose) onClose();
    },
    toggle() {
      if (sheet.hidden) api.show();
      else api.close();
    },
    /** After the contents changed size. */
    place() {
      if (!sheet.hidden) place(sheet, trigger);
    },
  };
  on(trigger, "click", () => api.toggle());
  return api;
}
