// The media library and the ad break.
//
// Folders down the left, which are real folders in the media folder; the
// files of the one picked on the right, with their tags; a search over
// names and tags. Files and folders drag onto a folder, and every move is
// also on the right click or long press menu. Upload puts files in the
// folder being looked at. The ad break sits underneath, as it always has.

import { el, clear, on } from "../../shell/dom.js";
import { registerAll } from "../../shell/commands.js";
import { shell } from "../../shell/shell.js";
import { visible, folderLabel } from "./model.js";
import { paintRail, openFolders, keepOpen } from "./rail.js";
import { dragging } from "./drag.js";
import { row } from "./row.js";
import { folderMenu, newFolder } from "./menus.js";
import { AdBreak } from "./adbreak.js";
import { paintBar, rowHost, carried, dropped } from "./bar.js";
import { uploadAll } from "./upload.js";

const REFRESH_MS = 500;

class MediaPanel extends HTMLElement {
  setClient(client) {
    this.client = client;
  }

  connectedCallback() {
    if (this.built) return;
    this.built = true;
    Object.assign(this, { items: [], folders: [], folder: null, deep: false, tags: [], picking: false, chosen: new Set(), open: openFolders() });
    this.appendChild(el("link", { rel: "stylesheet", href: new URL("./media.css", import.meta.url).href }));
    this.ad = new AdBreak(this.client);
    this.count = el("span.sm.dim");
    this.search = el("input.mx-search", { type: "search", placeholder: "Search names and tags", "aria-label": "Search media", oninput: () => this.render() });
    this.rail = el("div.mx-rail", { role: "tree", "aria-label": "Folders" });
    this.list = el("div.col.mx-list");
    this.bar = el("div.row.wrap.mx-bar");
    this.status = el("div.sm.faint.pad");
    this.file = el("input", { type: "file", accept: "video/*,image/*,.mkv,.ts,.flv,.webm,.mov,.svg,.gif", multiple: true, hidden: true });
    on(this.file, "change", () => this.upload(this.file.files));
    this.pickButton = el("button.btn", { text: "Select", title: "Choose several files to move or tag at once", onclick: () => this.setPicking(!this.picking) });
    this.append(
      el("div.row.wrap.pad.mx-head", {}, [el("strong", { text: "Media" }), this.count, this.search, el("span.grow"), this.pickButton, el("button.btn", { text: "Rescan", onclick: () => this.load() }), el("button.btn", { text: "Upload", onclick: () => this.file.click() })]),
      this.bar,
      el("div.mx-body", {}, [el("div.col", {}, [this.rail, el("button.btn.mx-new", { text: "New folder", onclick: () => newFolder(this, this.folder || "") })]), this.list]),
      ...this.ad.nodes(),
      this.status,
      this.file
    );
    this.offs = [
      this.client.on("media-changed", () => this.loadSoon()),
      this.client.onRender(() => this.workspaceActive !== false && this.ad.paint()),
      registerAll([
        { id: "media.upload", title: "Upload a clip", group: "Media", run: () => this.file.click() },
        { id: "adbreak.start", title: "Roll the ad break", group: "Media", run: () => this.ad.roll() },
        { id: "adbreak.end", title: "End the ad break", group: "Media", run: () => this.ad.end() },
      ]),
      dragging(this, (node) => carried(this, node), (what, into) => dropped(this, what, into)),
    ];
    // The shell hands dropped files here rather than guessing what they are for.
    shell.onFiles = (files) => this.upload(files);
    this.load();
  }

  setWorkspaceActive(active) {
    this.workspaceActive = active;
    if (!active) return clearTimeout(this.soon);
    this.ad.paint();
    this.load();
  }

  disconnectedCallback() {
    for (const off of this.offs || []) off();
    this.offs = [];
    clearTimeout(this.soon);
    if (shell.onFiles) shell.onFiles = null;
  }

  loadSoon() {
    if (this.workspaceActive === false) return;
    // A conversion sends one event per percent. Coalesce.
    clearTimeout(this.soon);
    this.soon = setTimeout(() => this.load(), REFRESH_MS);
  }

  async load() {
    if (this.workspaceActive === false) return;
    try {
      const listing = await this.client.call("media.list", {});
      Object.assign(this, { items: listing.items || [], folders: listing.folders || [], dir: listing.dir, error: listing.error });
      if (this.folder && !this.folders.includes(this.folder)) this.folder = null;
      for (const n of [...this.chosen]) if (!this.items.some((i) => i.name === n)) this.chosen.delete(n);
      this.render();
    } catch (e) {
      clear(this.list).appendChild(el("p.dim.sm", { text: e.message, style: { margin: 0 } }));
    }
  }

  render() {
    paintRail(this, this.rail);
    paintBar(this);
    const shown = visible(this.items, { folder: this.folder, deep: this.folder === null || this.deep, tags: this.tags, query: this.search.value });
    this.count.textContent = this.items.length ? (shown.length === this.items.length ? String(this.items.length) : `${shown.length} of ${this.items.length}`) : "";
    clear(this.list);
    if (!shown.length) {
      if (this.error) this.list.appendChild(el("p.dim.sm", { text: this.error, style: { margin: "0 0 4px" } }));
      const empty = this.items.length ? "Nothing here. Pick All files or clear the search." : "Nothing here yet. Press Upload, or drop a file on the window.";
      return this.list.appendChild(el("p.dim.sm", { text: empty, style: { margin: 0 } }));
    }
    const host = { ...rowHost(this), showFolder: this.folder === null || this.deep };
    for (const item of shown) this.list.appendChild(row(host, item));
  }

  setPicking(on) {
    this.picking = on;
    if (!on) this.chosen.clear();
    this.pickButton.textContent = on ? "Done" : "Select";
    this.render();
  }

  select(path) {
    this.folder = path;
    this.render();
  }

  toggle(path) {
    if (this.open.has(path)) this.open.delete(path);
    else this.open.add(path);
    keepOpen(this.open);
    this.render();
  }

  menu(path, x, y) {
    folderMenu(this, path, x, y);
  }

  upload(files) {
    return uploadAll(this.client, [...(files || [])], this.folder || "", this.status, folderLabel(this.folder || "")).then(() => this.load());
  }
}

// Drawn inside the small `gmx-media` loader (entry.js), which is all the page
// carries until the Media tab is shown.
customElements.define("gmx-media-view", MediaPanel);
export default MediaPanel;
