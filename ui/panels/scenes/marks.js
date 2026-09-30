// The mark on a scene's tab or tile when it draws a source that is not
// running. Hovering names them; a click opens the Fix dialog.
//
// Fetched the first time any scene has such a source, so a page whose
// sources all run never loads it. The panel keeps the check itself, which is
// a few lines; the buttons, their words and their stylesheet live here.

import { el } from "../../shell/dom.js";
import { sceneNotRunning, nameOf, openFix } from "./fix-note.js";

let styled = false;

function style() {
  if (styled) return;
  styled = true;
  document.head.appendChild(el("link", { rel: "stylesheet", href: new URL("./fix.css", import.meta.url).href }));
}

/** Every tab and tile of the panel, marked or cleared from the mixer's state now. */
export function paint(panel) {
  style();
  for (const [id, tab] of panel.tabs) {
    const holder = tab.parentNode;
    if (holder) update(panel, id, markIn(panel, id, holder, tab.nextSibling));
  }
  for (const [id, tile] of panel.tiles) update(panel, id, markIn(panel, id, tile.face, null));
}

/** The mark in this holder, made the first time it is wanted. */
function markIn(panel, id, holder, before) {
  const found = holder.querySelector(":scope > .scene-warn");
  if (found) return found;
  const mark = el("button.scene-warn", {
    text: "!",
    hidden: true,
    type: "button",
    "data-nodrag": "",
    onclick: (event) => {
      event.stopPropagation();
      openFix({ client: panel.client, scenes: panel.scenes, scene: id });
    },
    ondblclick: (event) => event.stopPropagation(),
  });
  holder.insertBefore(mark, before);
  return mark;
}

function update(panel, id, mark) {
  const ids = sceneNotRunning(panel.client, panel.scenes, id);
  mark.hidden = !ids.length;
  if (!ids.length) return;
  const names = ids.map((x) => nameOf(panel.client, x)).join(", ");
  const text = `Not running: ${names}. The scene goes to air without ${ids.length > 1 ? "them" : "it"}. Click to fix.`;
  mark.title = text;
  mark.setAttribute("aria-label", text);
}
