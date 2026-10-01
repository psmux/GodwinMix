// The show tabs: drawn here, on every page; whatever is done to one is
// show-actions.js, fetched on first use. No show.* on the core, no row.

import { el } from "./dom.js";

let acts = null;
export const actions = () => (acts ||= import("./show-actions.js"));
const WORD = { starting: "starting", stopped: "stopped", failed: "failed" };

export function showTabs(client) {
  const list = el("div.showtabs", { role: "tablist", "aria-label": "Shows" });
  const add = el("button.shows-add", { type: "button", text: "+", title: "New show", "aria-label": "New show", "data-act": "new" });
  const wall = el("button.shows-add", { type: "button", text: "▦", title: "Every show at once: the monitoring wall", "aria-label": "Monitoring wall", "data-act": "wall" });
  const row = el("div.shows", { hidden: true }, [list, add, wall]);
  const view = { client, row, list, shows: [], current: null };
  view.read = () => client.call("show.list", {}).then((a) => {
    view.shows = a.shows || [];
    view.current = new URLSearchParams(location.search).get("show") || a.current;
    // This page's show was removed, here or elsewhere: go to the first one.
    const has = (id) => view.shows.some((s) => s.id === id);
    if (view.shows.length && !has(view.current)) return actions().then((m) => m.switchTo(has(a.current) ? a.current : view.shows[0].id));
    draw(view);
  }, (e) => e && e.code === -32601 && stop());
  const held = client.listen ? client.listen("show.*") : () => {};
  const offs = [client.on("open", view.read), client.on("event", (e) => /^show\./.test(e.name) && view.read())];
  const stop = () => { held(); offs.forEach((f) => f()); row.remove(); };
  for (const type of ["click", "dblclick", "keydown", "contextmenu"]) {
    row.addEventListener(type, (e) => {
      // Not the page's F2 or Delete, which act on scenes.
      if (type === "contextmenu" || /^(F2|Arrow|Home|End|ContextMenu|Delete)/.test(e.key)) e.preventDefault(), e.stopPropagation();
      if (type !== "keydown" || e.key !== "Tab") actions().then((m) => m.handle(view, e));
    });
  }
  view.read();
  return row;
}

export function draw(view) {
  const { shows, current } = view;
  view.row.hidden = !shows.length;
  view.row.classList.toggle("one", shows.length === 1);
  view.list.replaceChildren(...shows.map((s) => {
    const on = s.id === current;
    const title = `${s.name}${s.on_air ? `, on air: ${s.on_air}` : WORD[s.state] ? `, ${s.state}` : ""}${s.error ? `. ${s.error}` : ""}`;
    return el("button.showtab", { role: "tab", type: "button", "aria-selected": String(on), tabindex: on ? "0" : "-1", "data-show": s.id, title }, [
      s.on_air ? el("span.dot.onair", { "aria-label": "on air" }) : null,
      el("span.showtab-name", { text: s.name }),
      WORD[s.state] ? el(`span.showtab-state.${s.state}`, { text: WORD[s.state] }) : null,
    ]);
  }));
}
