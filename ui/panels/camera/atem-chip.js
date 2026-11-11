// A chip per connected ATEM switcher in the Programme monitor's header: a dot
// for the link, its name, and what it has on programme (red) and preview
// (green) by the inputs' short names, so the operator sees it followed a
// take. A press opens switcher properties in the side drawer. The Programme
// panel fetches this after it draws; with no atem plugin and no demo it
// shows nothing.

import { el } from "../../shell/dom.js";
import { hubFor } from "./hub.js";
import { styled } from "./look.js";
import { shortOf } from "./words.js";

const open = (client, id) => import("./switcher.js").then((m) => m.openSwitcher(client, id));

function chip(client, id) {
  const dot = el("span.dot");
  const name = el("span.ellipsis");
  const pgm = el("span.atem-bus.pgm");
  const pvw = el("span.atem-bus.pvw");
  const root = el("button.atem-chip", { type: "button", onclick: () => open(client, id) }, [dot, name, pgm, pvw]);
  return {
    root,
    show(listed, state) {
      const up = !!(state ? state.connected : listed.connected);
      dot.className = "dot " + (up ? "live" : "failed");
      name.textContent = (state || listed).name || listed.id;
      const me = state && state.mes && state.mes[0];
      pgm.textContent = me ? shortOf(state, me.program) : "";
      pvw.textContent = me ? shortOf(state, me.preview) : "";
      pgm.hidden = pvw.hidden = !me;
      root.title = up
        ? `${name.textContent}: ${pgm.textContent || "nothing"} on programme, ${pvw.textContent || "nothing"} in preview. Press for its properties.`
        : `${name.textContent} is not connected${listed.why ? `: ${listed.why}` : ""}. Press for its properties.`;
    },
  };
}

/** Chips after the programme's name in `panel`'s header, while it is on the page. */
export function atemChips(panel) {
  styled();
  const client = panel.client;
  const host = el("span.atem-chips", { hidden: true });
  panel.programName.after(host);
  const hub = hubFor(client);
  const chips = new Map();
  let off = null;
  const paint = () => {
    host.hidden = !hub.atems.length;
    for (const [id, c] of chips) if (!hub.atems.some((a) => a.id === id)) (c.root.remove(), chips.delete(id));
    for (const a of hub.atems) {
      if (!chips.has(a.id)) chips.set(a.id, chip(client, a.id));
      const c = chips.get(a.id);
      if (c.root.parentNode !== host) host.append(c.root);
      c.show(a, hub.atemState.get(a.id));
    }
  };
  const parts = {
    active(on) {
      if (on && !off) off = hub.want("atem");
      if (!on && off) (off(), (off = null));
    },
  };
  parts.active(panel.isConnected && panel.workspaceActive !== false);
  hub.on((what) => what !== "people" && paint());
  return parts;
}
