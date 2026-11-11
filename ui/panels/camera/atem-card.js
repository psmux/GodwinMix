// The ATEM switchers in the Outputs panel, beside the destinations and in
// their style: a row per switcher with its link, its mode and what it has on
// programme and preview, Edit for switcher properties and Remove, and Add
// switcher. Shown only with the atem plugin installed or the demo on.

import { el } from "../../shell/dom.js";
import { confirmModal } from "../../shell/modal.js";
import { settings } from "../../shell/settings.js";
import { hubFor } from "./hub.js";
import { styled, actor } from "./look.js";
import { MODES, shortOf } from "./words.js";

const open = (client, id) => import("./switcher.js").then((m) => m.openSwitcher(client, id));

function row(hub, id, act) {
  const dot = el("span.dot");
  const name = el("strong");
  const label = el("span.sm.dim");
  const pgm = el("span.atem-bus.pgm");
  const pvw = el("span.atem-bus.pvw");
  const node = el("div.output-row.atem-row", { "data-switcher": id }, [el("div.row", {}, [
    dot, name, label, el("span.grow"), pgm, pvw,
    el("button.btn.icon", { type: "button", text: "Edit", onclick: () => open(hub.real, id) }),
    el("button.btn.icon.danger", { type: "button", text: "Remove", onclick: async () => {
      const ask = `Remove "${name.textContent}"? GodwinMix stops talking to it. The switcher itself keeps working.`;
      if (settings().confirmRemove && !(await confirmModal(ask, "Remove"))) return;
      act(() => hub.api.atemDisconnect(id));
    } }),
  ])]);
  return {
    node,
    show(listed, a) {
      const up = !!(a ? a.connected : listed.connected);
      dot.className = "dot " + (up ? "live" : "failed");
      name.textContent = (a || listed).name || id;
      const mode = MODES.find((m) => m[0] === ((a || listed).mode || "own"));
      label.textContent = up ? mode[1] : listed.why ? `Not connected: ${listed.why}` : "Not connected";
      const me = a && a.mes && a.mes[0];
      pgm.textContent = me ? shortOf(a, me.program) : "";
      pvw.textContent = me ? shortOf(a, me.preview) : "";
      pgm.hidden = pvw.hidden = !me;
    },
  };
}

/** Fill `host`, a section under the destinations, while Outputs is on the page. */
export function switcherCards(client, host) {
  styled();
  const hub = hubFor(client);
  const act = actor(hub);
  const list = el("div.col");
  const empty = el("p.dim.sm", { text: "No switcher yet. Add one to see what it has on air and to follow GodwinMix's takes.", style: { margin: 0 } });
  host.replaceChildren(el("div.row", {}, [
    el("strong", { text: "ATEM switchers" }), el("span.grow"),
    el("button.btn", { type: "button", text: "Add switcher", onclick: () => open(client, null) }),
  ]), list, empty);
  const rows = new Map();
  let off = null;
  const paint = () => {
    host.hidden = !hub.switchers;
    empty.hidden = hub.atems.length > 0;
    for (const [id, r] of rows) if (!hub.atems.some((a) => a.id === id)) (r.node.remove(), rows.delete(id));
    for (const a of hub.atems) {
      if (!rows.has(a.id)) rows.set(a.id, row(hub, a.id, act));
      const r = rows.get(a.id);
      if (r.node.parentNode !== list) list.append(r.node);
      r.show(a, hub.atemState.get(a.id));
    }
  };
  const offHub = hub.on((what) => what !== "people" && paint());
  paint();
  return {
    active(on) {
      if (on && !off) off = hub.want("atem");
      if (!on && off) (off(), (off = null));
    },
    destroy() {
      offHub();
      off?.();
    },
  };
}
