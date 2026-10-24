// What a person does to a show tab: switch to it, rename it, start or stop
// it, close it, remove it, and make a new one. Fetched the first time any of
// that is asked for; show-tabs.js only draws.
//
// Switching loads the page again on the other show's socket (`?show=<id>`,
// the one contract every panel already follows), so no panel is left showing
// one show's scenes against another show's programme. The layout is this
// browser's, not the show's, and stays as it was.

import { el } from "./dom.js";
import { contextMenu } from "./menu.js";
import { errorToast } from "./toast.js";
import { remove } from "./show-remove.js";
import { close } from "./show-close.js";
import { draw } from "./show-tabs.js";
import { rename } from "./show-rename.js";

export { rename };

/** The styles for the rename box and New show, once. */
export function sheet() {
  if (document.getElementById("gmx-shows-css")) return;
  document.head.append(el("link#gmx-shows-css", { rel: "stylesheet", href: new URL("./shows.css", import.meta.url).href }));
}

/** Every event the tab row forwards. */
export function handle(view, e) {
  sheet();
  const tab = e.target.closest(".showtab");
  const show = tab && view.shows.find((s) => s.id === tab.dataset.show);
  if (e.type === "click" && e.target.closest("[data-act=new]")) return import("./show-file.js").then((m) => m.newShow(view.client));
  if (e.type === "click" && e.target.closest("[data-act=wall]")) return import("../panels/wall/view.js").then((m) => m.toggleWall(view.client));
  if (!show || e.target.closest("input")) return;
  // The × and a middle click close the show, as they close a browser's tab,
  // and never switch to it first. A quick second click on the × is not a
  // rename either.
  if (e.type === "auxclick") return e.button === 1 && close(view, show);
  if (e.target.closest(".showtab-x") && (e.type === "click" || e.type === "dblclick")) return e.type === "click" && close(view, show);
  if (e.type === "click") return clicked(view, show, e);
  // A double click renames this show only. On another show's tab the first
  // click has already switched, as a browser's tab does.
  // Not when the first click of it was the one that closed a page over it.
  if (e.type === "dblclick") return show.id === view.current && !view.switching && e.timeStamp - coverClosed > 800 && rename(view, show);
  if (e.type === "contextmenu") return tabMenu(view, show, e.clientX, e.clientY);
  if (e.type === "keydown") return key(view, show, tab, e);
}

let pending = null;
let coverClosed = -Infinity;

/**
 * A click on another show switches to it at once, the way a browser's tab
 * does: the tab shows as chosen in the same frame and the page moves. It
 * used to wait a quarter second to see whether a second click made it a
 * rename, and a double click (a habit from opening things) renamed the
 * other show and never switched. A click on this show's own tab opens its
 * menu, unless a second click makes it a rename; Enter on the selected tab
 * opens the menu from the keyboard. With a page open over the show (the
 * monitoring wall, headend, flow map, routing or plugins), a click on any
 * tab closes that page first, and on this show's tab that is all it does.
 */
function clicked(view, show, e) {
  clearTimeout(pending);
  // Those pages cover the show's page, so a tab click that left one open
  // looked like it did nothing.
  const covers = document.querySelectorAll("[data-cover]");
  if (covers.length) {
    for (const c of covers) c.leave();
    coverClosed = e.timeStamp;
    if (show.id === view.current) return;
  }
  if (view.switching) return;
  if (show.id !== view.current) return go(view, show.id);
  if (e.detail > 1) return;
  const tab = e.target.closest(".showtab");
  const r = tab.getBoundingClientRect();
  pending = setTimeout(() => tabMenu(view, show, r.left, r.bottom + 4), e.detail === 1 ? 250 : 0);
}

/** Switch, with the row saying so until the page has moved. */
export function go(view, id) {
  if (view.switching || id === view.current) return;
  view.switching = id;
  draw(view);
  switchTo(id);
}

/** The next show along (1) or the one before (-1), round the ends. */
export function step(view, by) {
  const ids = view.shows.map((s) => s.id);
  if (ids.length < 2) return;
  const at = Math.max(0, ids.indexOf(view.switching || view.current));
  go(view, ids[(at + by + ids.length) % ids.length]);
}

function key(view, show, tab, e) {
  const tabs = [...view.list.querySelectorAll(".showtab")];
  const at = tabs.indexOf(tab);
  const to = { ArrowLeft: at - 1, ArrowRight: at + 1, Home: 0, End: tabs.length - 1 }[e.key];
  if (to !== undefined) {
    const next = tabs[(to + tabs.length) % tabs.length];
    for (const t of tabs) t.tabIndex = t === next ? 0 : -1;
    return next.focus();
  }
  if (e.key === "F2") return rename(view, show);
  if (e.key === "Delete") return remove(view, show);
  if (e.key === "ContextMenu") {
    const r = tab.getBoundingClientRect();
    return tabMenu(view, show, r.left, r.bottom + 4);
  }
}

/** Where the page goes; a test puts its own `go` here. */
export const nav = { go: (url) => location.assign(url) };

export function switchTo(id) {
  const url = new URL(location.href);
  url.searchParams.set("show", id);
  nav.go(url.toString());
}

function tabMenu(view, show, x, y) {
  const busy = show.state === "starting";
  const running = show.state === "running" || busy;
  contextMenu(x, y, [
    show.id !== view.current && { label: "Switch to this show", run: () => switchTo(show.id) },
    { label: "Rename", key: "F2", run: () => rename(view, show) },
    // A show without compositing has no page of its own to hold an Outputs
    // panel, so its outputs, input and settings open from its tab.
    show.compositing === false && { label: "Outputs and settings…", run: () => import("../panels/wall/detail.js").then((m) => m.showDetail(view.client, show)) },
    { kind: "separator" },
    running
      ? { label: busy ? "Stop starting" : "Stop show", run: () => startStop(view, show, "show.stop") }
      : { label: "Start show", run: () => startStop(view, show, "show.start") },
    { kind: "separator" },
    { label: "Close show", run: () => close(view, show) },
    { label: "Remove show…", key: "Delete", run: () => remove(view, show) },
  ]);
}

async function startStop(view, show, method) {
  try {
    await view.client.call(method, { id: show.id });
  } catch (e) {
    errorToast(e, method === "show.start" ? `Starting ${show.name}` : `Stopping ${show.name}`);
  }
  view.read();
}
