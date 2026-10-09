// The phone deck: the whole mixer on a phone, one screen at a time.
//
// A desk shows every panel at once and lets the operator arrange them. A phone
// has room for one thing, so the deck shows one screen and a tab bar along the
// bottom, where a thumb already is. Live holds what a show is run from: the
// monitors, Take and the scenes, together, so nobody scrolls between the shot
// and the button. Sources, Audio and Outputs have a tab each, and Outputs
// carries the channels too, because a channel is where an RTMP feed comes in
// and its Add Channel button is on Outputs. More lists every other panel,
// plugin panels included, and a few things from the menus.
//
// Panels are the same elements the dock uses, made through the registry. A
// screen that is not showing has its panels suspended where they support it
// and taken down where they do not, so a phone pays for one screen's worth of
// pictures and meters and nothing else.

import { el } from "./dom.js";
import * as registry from "./registry.js";
import { icon } from "./phone-icons.js";
import { morePage } from "./phone-more.js";

const TABS = [
  { id: "live", title: "Live", panels: ["core/program", "core/scenes"] },
  { id: "sources", title: "Sources", panels: ["core/sources"] },
  { id: "audio", title: "Audio", panels: ["core/audio"] },
  { id: "outputs", title: "Outputs", panels: ["core/outputs", "core/channels"] },
  { id: "more", title: "More", panels: [] },
];
const KEY = "gmx.phone.page";

export class PhoneDeck {
  constructor(workspace) {
    this.w = workspace;
    this.client = workspace.client;
    this.frames = new Map();
    this.scrolls = new Map();
    this.host = workspace.root.parentElement;
    this.host.classList.add("phone");
    workspace.root.classList.add("phone-deck");
    this.head = el("header.phone-head", { hidden: true });
    this.more = el("div.phone-more", { hidden: true });
    this.tabs = el("nav.phone-nav", { "aria-label": "Screens" }, TABS.map((t) => el("button.phone-tab", {
      type: "button", "data-tab": t.id, "aria-label": t.title, onclick: () => this.go(t.id),
    }, [icon(t.id), el("span.phone-tab-label", { text: t.title })])));
    workspace.root.append(this.head, this.more);
    this.host.append(this.tabs);
    try { this.page = localStorage.getItem(KEY) || "live"; } catch { this.page = "live"; }
  }

  /** Panels that have no tab of their own: they live under More. */
  extras() {
    const tabbed = new Set(TABS.flatMap((t) => t.panels));
    return registry.list().filter((p) => !p.slots.some((s) => s === "header" || s === "modal") && !tabbed.has(p.id));
  }

  /** The panel ids a page shows, with the ones not registered left out. */
  panelsOf(page) {
    if (page.startsWith("panel:")) return registry.get(page.slice(6)) ? [page.slice(6)] : [];
    return (TABS.find((t) => t.id === page)?.panels || []).filter((id) => registry.get(id));
  }

  /** Which tab lights up: a panel opened from More keeps More lit. */
  tab() {
    return this.page.startsWith("panel:") ? "more" : this.page;
  }

  go(page) {
    this.scrolls.set(this.page, this.w.root.scrollTop);
    this.page = page;
    try { localStorage.setItem(KEY, page); } catch { /* the page is remembered for the session */ }
    this.render();
    this.w.root.scrollTop = this.scrolls.get(page) || 0;
  }

  /** What the dock's show() and activate() mean here: bring that panel up. */
  open(id) {
    const tab = TABS.find((t) => t.panels.includes(id));
    this.go(tab ? tab.id : "panel:" + id);
    // The deck's own scroll, never scrollIntoView, which scrolls the window too.
    const at = this.frames.get(id)?.element;
    if (at) this.w.root.scrollTop += at.getBoundingClientRect().top - this.w.root.getBoundingClientRect().top - 12;
  }

  render() {
    if (!TABS.some((t) => t.id === this.page) && !this.panelsOf(this.page).length) this.page = "live";
    const want = this.panelsOf(this.page);
    for (const [id, frame] of this.frames) if (!want.includes(id)) this.park(id, frame);
    want.forEach((id, order) => this.show(id, order));
    this.heading();
    this.more.hidden = this.page !== "more";
    if (!this.more.hidden) this.more.replaceChildren(...morePage(this, this.extras()));
    this.host.dataset.phonePage = this.page.startsWith("panel:") ? "panel" : this.page;
    for (const b of this.tabs.children) b.setAttribute("aria-current", String(b.dataset.tab === this.tab()));
  }

  /** A title above every screen but Live, which spends the room on pictures. */
  heading() {
    const id = this.page.startsWith("panel:") ? this.page.slice(6) : null;
    const title = id ? registry.get(id)?.title || id : TABS.find((t) => t.id === this.page)?.title;
    this.head.hidden = this.page === "live";
    const back = id ? el("button.phone-back", { type: "button", "aria-label": "Back to More", onclick: () => this.go("more") }, [icon("back")]) : null;
    this.head.replaceChildren(...[back, el("h1", { text: title || "" })].filter(Boolean));
  }

  /** Make a panel, or wake the suspended one, at its place on the screen. */
  show(id, order) {
    let frame = this.frames.get(id);
    if (!frame) {
      const made = registry.instantiate(id, this.client, {});
      if (!made) return;
      const element = el("section.phone-frame", { "data-dock-panel": id, "aria-label": registry.get(id)?.title || id }, [made.node]);
      frame = { element, made };
      this.frames.set(id, frame);
      this.w.root.append(element);
    } else if (frame.element.hidden) {
      frame.element.hidden = false;
      try { frame.made.node.setWorkspaceActive(true); } catch (e) { console.error("Panel activation failed", id, e); }
    }
    // Order, never a move: moving a connected panel would disconnect it.
    frame.element.style.order = String(order + 1);
  }

  /** Off screen: suspended if the panel can be, taken down if it cannot. */
  park(id, frame) {
    if (typeof frame.made.node.setWorkspaceActive === "function") {
      if (frame.element.hidden) return;
      try {
        frame.made.node.setWorkspaceActive(false);
        frame.element.hidden = true;
        return;
      } catch (e) { console.error("Panel suspension failed", id, e); }
    }
    frame.made.destroy();
    frame.element.remove();
    this.frames.delete(id);
  }

  destroy() {
    for (const frame of this.frames.values()) { frame.made.destroy(); frame.element.remove(); }
    this.frames.clear();
    this.head.remove();
    this.more.remove();
    this.tabs.remove();
    this.host.classList.remove("phone");
    delete this.host.dataset.phonePage;
    this.w.root.classList.remove("phone-deck");
  }
}
