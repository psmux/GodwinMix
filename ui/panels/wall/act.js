// What a click or a key does on the wall: sort, filter, group, rows or
// tiles, move the cursor, open a show, acknowledge its alarms, switch its
// compositing. The choices are this browser's and are remembered.

import { errorToast, toast } from "../../shell/toast.js";
import { alarmKey } from "./cells.js";
import { healthOf, mixed } from "./model.js";

const KEY = "gmx.wall";

export function prefs() {
  try {
    return { sort: "alarms", dir: 1, mode: "rows", group: false, ...JSON.parse(localStorage.getItem(KEY) || "{}"), text: "", alarm: "" };
  } catch {
    return { sort: "alarms", dir: 1, mode: "rows", group: false, text: "", alarm: "" };
  }
}

export function savePrefs(o) {
  try {
    localStorage.setItem(KEY, JSON.stringify({ sort: o.sort, dir: o.dir, mode: o.mode, group: o.group }));
  } catch { /* a private window keeps nothing, and needs nothing */ }
}

export function press(view, what, value) {
  const o = view.opts;
  if (what === "close") return view.close();
  if (what === "add") return import("./bulk.js").then((m) => m.bulkAdd(view.client, { onDone: () => view.data.read() }));
  if (what === "group") o.group = !o.group;
  else if (what === "rows" || what === "tiles") o.mode = what;
  else if (what === "text") o.text = value;
  else if (what === "alarm") o.alarm = value;
  else if (what === "click") return clicked(view, value);
  savePrefs(o);
  view.draw();
}

function clicked(view, e) {
  const sort = e.target.closest("[data-sort]");
  if (sort) {
    const o = view.opts;
    o.dir = o.sort === sort.dataset.sort ? -o.dir : 1;
    o.sort = sort.dataset.sort;
    savePrefs(o);
    return view.draw();
  }
  const mix = e.target.closest("[data-act=mix]");
  if (mix) return compositing(view, mix.dataset.id);
  const at = e.target.closest("[data-id]");
  if (!at) return;
  view.cursor = at.dataset.id;
  openShow(view, at.dataset.id);
}

/** Turn mixing on or off. The station refuses when it cannot, and says why. */
export async function compositing(view, id) {
  const show = view.data.find(id);
  if (!show) return;
  if (show.switching) return;
  const on = !mixed(show);
  const moving = () => toast({ text: `Switching ${show.name} to ${on ? "mixed" : "direct"}. Its outputs move over in the next half minute.` });
  try {
    await view.data.set(id, { compositing: on }, moving);
    toast({ text: on ? `${show.name} is mixed now: it has scenes and its own programme encode.` : `${show.name} is direct now: its input goes straight to its outputs.` });
  } catch (e) {
    errorToast(e, `${show.name} stays ${on ? "direct" : "mixed"}`);
  }
}

/** A mixed show opens in the mixer; a direct one opens its detail. */
export async function openShow(view, id) {
  const show = view.data.find(id);
  if (!show) return;
  if (mixed(show)) return (await import("../../shell/show-actions.js")).switchTo(id);
  const { showDetail } = await import("./detail.js");
  showDetail(view.client, show, { data: view.data });
}

/** Silence the flashing of every alarm the show has now, until each clears. */
export function acknowledge(view, id) {
  const show = view.data.find(id);
  if (!show) return;
  for (const a of healthOf(show).alarms) view.acked.add(alarmKey(id, a));
  view.draw();
}

const typing = (t) => t && (t.closest("input, select, textarea") || t.isContentEditable);

export function keyed(view, e) {
  if (document.querySelector(".scrim") || !view.root.isConnected) return;
  if (e.key === "Escape") {
    if (typing(e.target) && e.target.value) return;
    e.preventDefault();
    return view.close();
  }
  if (typing(e.target) || !view.root.contains(document.activeElement)) return;
  const ids = view.items.filter((it) => it.kind === "show").map((it) => it.show.id);
  if (!ids.length) return;
  const at = Math.max(0, ids.indexOf(view.cursor));
  const step = view.opts.mode === "tiles" ? view.cols : 1;
  const to = { ArrowDown: at + step, ArrowUp: at - step, Home: 0, End: ids.length - 1, PageDown: at + 10 * step, PageUp: at - 10 * step };
  if (view.opts.mode === "tiles") Object.assign(to, { ArrowRight: at + 1, ArrowLeft: at - 1 });
  if (e.key in to) {
    e.preventDefault();
    return move(view, ids, view.cursor ? to[e.key] : 0);
  }
  if (e.key === "Enter" && view.cursor) return e.preventDefault(), openShow(view, view.cursor);
  if ((e.key === "a" || e.key === "A") && !e.ctrlKey && !e.metaKey && view.cursor) return e.preventDefault(), acknowledge(view, view.cursor);
}

function move(view, ids, i) {
  view.cursor = ids[Math.min(ids.length - 1, Math.max(0, i))];
  const index = view.list.items.findIndex((it) => (it.show && it.show.id === view.cursor) || (it.shows && it.shows.some((s) => s.id === view.cursor)));
  view.list.reveal(index);
  view.draw();
  view.scroll.setAttribute("aria-activedescendant", `wl-${view.cursor}`);
}
