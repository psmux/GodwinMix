// The events, newest first: play one at the speed picked, take it on air,
// save it to the media folder, rename it or forget it.

import { el, fmtDuration } from "../../shell/dom.js";
import { toast } from "../../shell/toast.js";
import { call, play } from "./panel.js";

export function events(changed) {
  const list = el("div.col.replay-events");
  const root = el("div.col", {}, [el("strong.sm", { text: "Events" }), list]);
  let shown = "";

  function save(e) {
    toast({ text: `Saving ${e.name}` });
    call("replay.save", { event: e.id }).then((s) => toast({ text: `Saved to ${s.name}` }), () => {});
  }

  function rename(e) {
    const name = window.prompt("Name this replay", e.name);
    if (name && name.trim()) call("replay.event.rename", { id: e.id, name: name.trim() }).then(changed, () => {});
  }

  function row(e, now) {
    const open = !e.out_ms;
    const length = open ? `open, ${fmtDuration((now - e.in_ms) / 1000)}` : fmtDuration((e.out_ms - e.in_ms) / 1000);
    return el("div.row.replay-event" + (open ? ".open" : ""), { "data-event": e.id }, [
      el("button.btn.sm.replay-name", { text: e.name, title: "Rename", onclick: () => rename(e) }),
      el("span.grow.sm.dim.num", { text: length }),
      el("button.btn.sm", { text: "Play", disabled: open, onclick: () => play(e.id) }),
      el("button.btn.sm.primary", { text: "Air", title: "Play it on the programme", disabled: open, onclick: () => play(e.id, true) }),
      el("button.btn.sm", { text: "Save", disabled: open, onclick: () => save(e) }),
      el("button.btn.sm.danger", { text: "×", title: "Forget this event", "aria-label": "Forget " + e.name, onclick: () => call("replay.event.delete", { id: e.id }).then(changed, () => {}) }),
    ]);
  }

  function paint(status) {
    root.hidden = !status.on;
    const open = status.events.some((e) => !e.out_ms);
    const key = JSON.stringify(status.events) + (open ? Math.floor(status.now_ms / 1000) : "");
    if (key === shown) return;
    shown = key;
    const rows = [...status.events].reverse().map((e) => row(e, status.now_ms));
    list.replaceChildren(...(rows.length ? rows : [el("span.sm.dim", { text: "Mark in and out around a moment, or press Last 10 s" })]));
  }

  return { root, paint };
}
