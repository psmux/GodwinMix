// What a person does to a show tab: switch to it, rename it, start or stop
// it, remove it, and make a new one. Fetched the first time any of that is
// asked for; show-tabs.js only draws.
//
// Switching loads the page again on the other show's socket (`?show=<id>`,
// the one contract every panel already follows), so no panel is left showing
// one show's scenes against another show's programme. The layout is this
// browser's, not the show's, and stays as it was.

import { el } from "./dom.js";
import { contextMenu } from "./menu.js";
import { errorToast } from "./toast.js";
import { remove } from "./show-remove.js";

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
  if (e.type === "click") return clicked(view, show, e);
  if (e.type === "dblclick") return rename(view, show);
  if (e.type === "contextmenu") return tabMenu(view, show, e.clientX, e.clientY);
  if (e.type === "keydown") return key(view, show, tab, e);
}

let pending = null;

/**
 * A click on another show switches to it, unless a second click makes it a
 * rename. A click on this show's own tab opens its menu, so Enter on the
 * selected tab does the same from the keyboard.
 */
function clicked(view, show, e) {
  clearTimeout(pending);
  if (e.detail > 1) return;
  const tab = e.target.closest(".showtab");
  const r = tab.getBoundingClientRect();
  const open = () => tabMenu(view, show, r.left, r.bottom + 4);
  if (show.id === view.current) pending = setTimeout(open, e.detail === 1 ? 250 : 0);
  else pending = setTimeout(() => switchTo(show.id), e.detail === 1 ? 250 : 0);
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
    { kind: "separator" },
    running
      ? { label: busy ? "Stop starting" : "Stop show", run: () => startStop(view, show, "show.stop") }
      : { label: "Start show", run: () => startStop(view, show, "show.start") },
    { kind: "separator" },
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

/** The name, in place, as an input: Enter keeps it, Escape puts it back. */
export function rename(view, show) {
  const tab = view.list.querySelector(`[data-show="${CSS.escape(show.id)}"]`);
  const name = tab && tab.querySelector(".showtab-name");
  if (!name) return;
  const input = el("input.showtab-input", { type: "text", value: show.name, "aria-label": `New name for ${show.name}`, size: Math.max(6, show.name.length) });
  name.replaceWith(input);
  input.select();
  let done = false;
  const finish = async (keep) => {
    if (done) return;
    done = true;
    const next = input.value.trim();
    if (keep && next && next !== show.name) {
      try {
        await view.client.call("show.rename", { id: show.id, name: next });
      } catch (e) {
        errorToast(e, "Rename");
      }
    }
    await view.read();
    view.list.querySelector(`[data-show="${CSS.escape(show.id)}"]`)?.focus();
  };
  input.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Enter") finish(true);
    if (e.key === "Escape") finish(false);
  });
  input.addEventListener("blur", () => finish(true));
}
