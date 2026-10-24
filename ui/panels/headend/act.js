// What a press or a key does in the headend view: filter, sort, pick a
// group, full screen, close, and the setup dialogs (add, edit, scan, the
// channel list, profiles, import), each fetched by import() when first
// pressed. The order and problems only are this
// browser's and are remembered; the rest starts fresh each time.

import { WORD, stateOf } from "./model.js";

const KEY = "gmx.headend";

export function prefs() {
  const base = { sort: "number", problems: false, text: "", group: "", state: "" };
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) || "{}");
    return { ...base, sort: saved.sort || base.sort, problems: !!saved.problems };
  } catch {
    return base;
  }
}

export function savePrefs(o) {
  try {
    localStorage.setItem(KEY, JSON.stringify({ sort: o.sort, problems: o.problems }));
  } catch { /* a private window keeps nothing, and needs nothing */ }
}

export function act(view, what, value) {
  const o = view.opts;
  if (what === "close") return view.close();
  if (what === "full") return full(view.root);
  const done = () => view.reload();
  const setup = view.setup || undefined;
  if (what === "import") return import("./import.js").then((m) => m.importFlussonic(view.client, { following: view.box.follow, onDone: done }));
  if (what === "add" || what === "edit") return import("./editor.js").then((m) => m.editChannel(view.client, { setup, entry: what === "edit" ? value : null, onDone: done }));
  if (what === "scan") return import("./scan.js").then((m) => m.scanFeed(view.client, { setup, onDone: done }));
  if (what === "profiles") return import("./profiles.js").then((m) => m.manageProfiles(view.client, { setup, onChange: done }));
  if (what === "channels") return import("./channels.js").then((m) => m.channelList(view.client, { onChange: done, stateOf: (id) => stateWord(view, id) }));
  if (what === "flow") return import("../flowmap/view.js").then((m) => m.showFlowMap(view.client));
  if (what === "problems") o.problems = !o.problems;
  else if (what === "state") o.state = o.state === value ? "" : value;
  else if (what in o) o[what] = value;
  savePrefs(o);
  view.scroll.scrollTop = 0;
  view.later();
}

/** A channel's state word, from the tiles' own numbers. */
function stateWord(view, id) {
  const show = view.data.find(id);
  return show ? WORD[stateOf(show, view.data.stats.get(id))] : "";
}

function full(root) {
  if (document.fullscreenElement) return document.exitFullscreen().catch(() => {});
  if (root.requestFullscreen) root.requestFullscreen().catch(() => {});
}

const typing = (t) => t && t.closest && t.closest("input, select, textarea");

/** Escape closes the drawer, then the view; Enter opens the focused tile. */
export function keyed(view, e) {
  if (document.querySelector(".scrim") || !view.root.isConnected) return;
  if (e.key === "Escape") {
    if (typing(e.target) && e.target.value) return;
    e.preventDefault();
    return view.side.id ? view.side.close() : view.close();
  }
  if (e.key === "/" && !typing(e.target)) {
    e.preventDefault();
    return view.tools.search.focus();
  }
  const tile = e.target.closest && e.target.closest(".he-tile");
  if (tile && (e.key === "Enter" || e.key === " ")) {
    e.preventDefault();
    view.pick(tile.dataset.id);
  }
}
