// Which scene Sources is listing, as a row of chips a thumb can switch.
//
// On a desk the scene strip sits beside the tray, so picking a scene there is
// picking what the tray shows. A phone has one screen at a time and nothing
// beside the tray, so the tray carries its own row. The stylesheet hides it
// everywhere but the phone deck. A chip only moves the focus; it never arms
// or takes, so switching scenes here cannot change what is on air.

import { el } from "../../shell/dom.js";
import { setFocusedScene } from "../../shell/focus.js";

export class SceneStrip {
  constructor() {
    this.node = el("div.src-scenes", { role: "tablist", "aria-label": "Scene" });
    this.signature = "";
  }

  /** Draw the chips for `list` (scene summaries) with `focused` lit. */
  paint(list, focused) {
    const signature = list.map((s) => `${s.id}/${s.name}/${count(s)}`).join("|");
    if (signature !== this.signature) {
      this.signature = signature;
      this.node.replaceChildren(...list.map((s) => el("button.src-scene", {
        type: "button", role: "tab", "data-id": s.id,
        onclick: () => setFocusedScene(s.id),
      }, [el("span.ellipsis", { text: s.name }), el("span.num", { text: String(count(s)) })])));
    }
    this.node.hidden = list.length < 2;
    for (const chip of this.node.children) {
      const on = chip.dataset.id === focused;
      chip.setAttribute("aria-selected", String(on));
      if (on) this.reveal(chip);
    }
  }

  /** The lit chip scrolled into the row, never scrollIntoView, which scrolls the page too. */
  reveal(chip) {
    const row = this.node;
    const left = chip.offsetLeft;
    if (left < row.scrollLeft || left + chip.offsetWidth > row.scrollLeft + row.clientWidth) row.scrollLeft = left - 12;
  }
}

/** The sources a scene draws, each once: the number of tiles Sources shows for it. */
export function count(summary) {
  return Array.isArray(summary.sources) ? new Set(summary.sources).size : summary.items || 0;
}
