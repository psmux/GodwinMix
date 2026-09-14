// Scenes: the slot, waiting for the scene server.
//
// Dragging sources together to make a scene is 05 section 3a and 11 in full. It
// needs `scene.create_from`, `scene.item.*` and the composer, none of which this
// core has. The panel is here so the layout and the keyboard map are already
// the right shape when they arrive, and so nobody wonders where scenes went.

import { el } from "../../shell/dom.js";

class ScenesPanel extends HTMLElement {
  static get panel() {
    return { id: "core/scenes", title: "Scenes", slots: ["main"], tag: "gmx-scenes" };
  }

  setClient(client) {
    this.client = client;
  }

  connectedCallback() {
    if (this.built) return;
    this.built = true;
    this.append(
      el("div.row.pad", {}, [el("strong", { text: "Scenes" }), el("span.grow")]),
      el("div.empty", { style: { padding: "20px var(--pad)" } }, [
        el("div.dim.sm", {
          text: "Dragging sources together to build a scene arrives with the scene server; until then a source is its own one item scene and tapping its tile puts it on air.",
          style: { maxWidth: "52ch" },
        }),
      ])
    );
  }
}

customElements.define("gmx-scenes", ScenesPanel);
window.godwinmixPanels.push(ScenesPanel);
export default ScenesPanel;
