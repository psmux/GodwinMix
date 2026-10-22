// The input groups strip: each address two or more channels read (a
// multicast group carrying several programs, usually), with its rate over
// two minutes, its continuity errors in the last minute, and a dot per
// channel it feeds. A group with nothing arriving goes red as one fault,
// so nine dark tiles read as the one cause they are. Clicking a group
// shows only its channels; clicking it again shows them all.

import { el } from "../../shell/dom.js";
import { kbps } from "../wall/model.js";
import { spark } from "./charts.js";
import { numbered, stateOf } from "./model.js";

function chip(key) {
  const s = spark("grp", 90, 22);
  const name = el("span.he-gname");
  const rate = el("span.he-v");
  const sub = el("span.he-gsub");
  const dots = el("span.he-dots");
  const node = el("button.he-group", { type: "button", "data-group": key }, [el("span.he-gtop", {}, [name, rate]), s.node, el("span.he-gbot", {}, [dots, sub])]);
  return { node, s, name, rate, sub, dots };
}

export function groupStrip() {
  const chips = new Map();
  const list = el("div.he-groups");
  const single = el("span.he-single");
  const node = el("section.he-strip.he-groupstrip", { "aria-label": "Input groups" }, [el("h3", { text: "Inputs" }), list, single]);
  return {
    node,
    /** `g` is model.groups(); `stats` the show.stats map; `pick` the group filtered on. */
    draw(g, stats, pick) {
      node.hidden = !g.shared.length;
      for (const grp of g.shared) {
        if (!chips.has(grp.key)) chips.set(grp.key, chip(grp.key));
        const c = chips.get(grp.key);
        list.append(c.node);
        c.node.classList.toggle("dead", !!grp.dead);
        c.node.classList.toggle("picked", pick === grp.key);
        c.node.setAttribute("aria-pressed", String(pick === grp.key));
        c.name.textContent = grp.key;
        c.rate.textContent = grp.dead ? "nothing arriving" : kbps(grp.kbps);
        c.sub.textContent = [`${grp.live}/${grp.members.length} live`, grp.cc ? `${grp.cc} CC/min` : ""].filter(Boolean).join(" · ");
        c.sub.classList.toggle("bad", !!grp.cc);
        c.s.set(grp.hist.slice(-60));
        drawDots(c.dots, grp, stats);
        c.node.title = `${grp.transport} ${grp.key}: ${grp.members.map((s) => numbered(s).num || s.name).join(", ")}`;
      }
      for (const [k, c] of chips) if (!g.shared.some((x) => x.key === k)) c.node.remove(), chips.delete(k);
      single.textContent = g.single ? `and ${g.single} single ${g.single === 1 ? "feed" : "feeds"}` : "";
    },
  };
}

function drawDots(dots, grp, stats) {
  const states = grp.members.map((s) => stateOf(s, stats.get(s.id)));
  const key = states.join(",");
  if (dots.dataset.key === key) return;
  dots.dataset.key = key;
  dots.replaceChildren(...states.map((st) => el(`span.he-gdot.s-${st}`)));
}
