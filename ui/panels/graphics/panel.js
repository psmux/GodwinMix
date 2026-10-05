// The Graphics gallery: every lower third, background, ticker, bug, title
// card, web graphic, clip and virtual set, whoever made it, as cards.
//
// Nothing here runs unless the panel is showing. The list is read when it
// is first shown and every few seconds after while it stays showing (so a
// graphic an agent saves appears without a click), and a card's picture is
// drawn by the mixer only when that card is on screen.

import { el, clear, on } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { FILTERS, visible } from "./model.js";
import { card, watchPictures } from "./card.js";
import * as act from "./actions.js";

const POLL_MS = 5000;
const ACCEPT = ".svg,.html,.htm,.zip,.png,.webp,.jpg,.jpeg,.gif,.webm,.mov,.mp4,.json,.toml";

class GraphicsView extends HTMLElement {
  setClient(client) {
    this.client = client;
  }

  connectedCallback() {
    if (this.built) return;
    this.built = true;
    this.items = [];
    this.filter = "all";
    this.state = new Map();
    this.appendChild(el("link", { rel: "stylesheet", href: new URL("./graphics.css", import.meta.url).href }));
    this.count = el("span.sm.dim");
    this.search = el("input.gx-search", { type: "search", placeholder: "Search graphics", "aria-label": "Search graphics", oninput: () => this.render() });
    this.file = el("input", { type: "file", accept: ACCEPT, multiple: true, hidden: true });
    on(this.file, "change", () => this.take([...this.file.files]));
    this.chips = el("div.row.wrap.gx-filters");
    this.grid = el("div.gx-grid");
    this.append(
      el("div.row.wrap.pad.gx-head", {}, [
        el("strong", { text: "Graphics" }),
        this.count,
        this.search,
        el("span.grow"),
        el("button.btn", { text: "Make one with an AI agent", onclick: () => import("./make.js").then((m) => m.makeWithAgent(this.client)) }),
        el("button.btn", { text: "Import", title: "SVG, HTML, a zip, a picture or a clip. Or drop files here.", onclick: () => this.file.click() }),
        el("button.btn", { text: "Export", title: "Your graphics as one zip, for another mixer", onclick: () => act.exportItems(this.client, []).catch((e) => errorToast(e, "Export")) }),
      ]),
      this.chips,
      this.grid,
      this.file
    );
    this.dropping();
    this.paintChips();
    this.load();
  }

  setWorkspaceActive(active) {
    this.active = active;
    if (active) this.load();
    else this.quiet();
  }

  disconnectedCallback() {
    this.quiet();
    this.pictures?.disconnect();
  }

  quiet() {
    clearTimeout(this.timer);
    this.timer = null;
  }

  /** Read the list, and again in a moment while the panel stays showing. */
  async load() {
    this.quiet();
    if (this.active === false || !this.isConnected) return;
    try {
      const listing = await this.client.call("gallery.list", { limit: 500 });
      const sig = JSON.stringify(listing.items.map((i) => [i.id, i.saved, i.name, i.placed]));
      if (sig !== this.sig) {
        this.sig = sig;
        this.items = listing.items || [];
        for (const i of this.items) if (i.placed && i.placed.length && !this.state.has(i.id)) this.state.set(i.id, "placed");
        this.render();
      }
    } catch (e) {
      clear(this.grid).appendChild(el("p.sm.dim.pad", { text: e.message }));
    }
    if (document.visibilityState !== "hidden") this.timer = setTimeout(() => this.load(), POLL_MS);
  }

  reload() {
    this.sig = null;
    this.load();
  }

  paintChips() {
    clear(this.chips);
    for (const f of FILTERS) {
      this.chips.appendChild(el("button.btn.gx-chip", { text: f.label, "aria-pressed": String(f.id === this.filter), onclick: () => ((this.filter = f.id), this.paintChips(), this.render()) }));
    }
  }

  render() {
    const shown = visible(this.items, this.filter, this.search.value);
    this.count.textContent = `${shown.length} of ${this.items.length}`;
    this.pictures?.disconnect();
    clear(this.grid);
    if (!shown.length) {
      this.grid.appendChild(el("p.sm.dim.pad", { text: this.items.length ? "Nothing matches. Clear the search or pick All." : "No graphics yet. Make one with an AI agent, or import or drop a file here." }));
      return;
    }
    const host = { client: this.client, state: this.state, reload: () => this.reload(), edit: (item) => this.edit(item) };
    for (const item of shown) this.grid.appendChild(card(host, item));
    this.pictures = watchPictures(this.grid);
  }

  edit(item) {
    import("./edit.js").then((m) => m.editGraphic(this.client, item, () => this.reload()));
  }

  async take(files) {
    if (!files.length) return;
    await act.importFiles(this.client, files);
    this.file.value = "";
    this.reload();
  }

  /** Files dropped on the gallery come here, not to the media library. */
  dropping() {
    const has = (e) => [...(e.dataTransfer?.types || [])].includes("Files");
    on(this, "dragover", (e) => {
      if (!has(e)) return;
      e.preventDefault();
      e.stopPropagation();
      this.classList.add("gx-drop");
    });
    on(this, "dragleave", () => this.classList.remove("gx-drop"));
    on(this, "drop", (e) => {
      if (!has(e)) return;
      e.preventDefault();
      e.stopPropagation();
      this.classList.remove("gx-drop");
      this.take([...e.dataTransfer.files]);
    });
  }
}

customElements.define("gmx-graphics-view", GraphicsView);
