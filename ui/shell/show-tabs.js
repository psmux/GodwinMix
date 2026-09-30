// The show tabs in the top bar, one per show on this machine, the show this
// page talks to selected. Only the drawing is here, because it is on every
// page; whatever a person does to a tab is show-actions.js, fetched the first
// time they do it. A core without shows answers "no such method" and the
// row never appears.

import { el } from "./dom.js";

let acts = null;
export const actions = () => (acts ||= import("./show-actions.js"));
const WORD = { starting: "starting", stopped: "stopped", failed: "failed" };

export function showTabs(client) {
  const list = el("div.showtabs", { role: "tablist", "aria-label": "Shows" });
  const add = el("button.shows-add", { type: "button", text: "+", title: "New show", "aria-label": "New show", "data-act": "new" });
  const row = el("div.shows", { hidden: true }, [list, add]);
  const view = { client, row, list, shows: [], current: null };
  view.read = () => client.call("show.list", {}).then((a) => {
    view.shows = a.shows || [];
    view.current = new URLSearchParams(location.search).get("show") || a.current;
    draw(view);
  }, (e) => e && e.code === -32601 && stop());
  const held = client.listen ? client.listen("show.*") : () => {};
  const offs = [client.on("open", view.read), client.on("event", (e) => /^show\./.test(e.name) && view.read())];
  const stop = () => { held(); offs.forEach((f) => f()); row.remove(); };
  for (const type of ["click", "dblclick", "keydown", "contextmenu"]) {
    row.addEventListener(type, (e) => {
      // The page's own keys (F2 renames a scene, Delete removes one) are not these.
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
