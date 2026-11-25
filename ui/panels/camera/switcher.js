// Switcher properties, in the side drawer, for one ATEM switcher: whether
// the link is up, how it moves with GodwinMix (on its own, following
// GodwinMix's takes, or leading them), its programme and preview buses with
// Cut and Auto, and folded below them the source map, the transition, the
// keyers and fade to black, the aux outputs, the macros and Disconnect.
// Opened from the switcher's chip in the Programme header or its row in
// Outputs. A switcher not added yet gets the connect form instead.

import { el } from "../../shell/dom.js";
import { shell } from "../../shell/shell.js";
import { hubFor } from "./hub.js";
import { bus, plainInputs } from "./bus.js";
import { styled, actor, drawerHead, demoFoot } from "./look.js";
import { MODES } from "./words.js";
import { switcherMore } from "./switcher-more.js";

export async function openSwitcher(client, id) {
  styled();
  const hub = hubFor(client);
  await hub.ready;
  if (!id) return import("./switcher-connect.js").then((m) => shell.drawer(m.connectForm(hub, (made) => openSwitcher(client, made))));
  const off = hub.want("atem");
  await hub.refresh();
  shell.drawer(switcherProps(hub, id, off));
}

function switcherProps(hub, id, offAtem) {
  const act = actor(hub);
  const listed = () => hub.atems.find((a) => a.id === id) || { id, name: id };
  const head = drawerHead(hub, listed().name || id, "Switcher");
  const dot = el("span.dot");
  const line = el("span.sm.dim.grow");
  const modes = el("div.cam-seg", { role: "radiogroup", "aria-label": "How it works with GodwinMix" },
    MODES.map(([v, t, tip]) => el("button.btn.sm", { type: "button", role: "radio", text: t, title: tip, "data-v": v, onclick: () => act(() => hub.api.atemMode(id, v)) })));
  const modeWords = el("p.sm.dim.cam-mode-words");
  const pgm = bus("Programme", "pgm", (i) => act(() => hub.api.atemProgram(id, i)));
  const pvw = bus("Preview", "pvw", (i) => act(() => hub.api.atemPreview(id, i)));
  const cut = el("button.btn", { type: "button", text: "Cut", title: "Swap programme and preview at once", onclick: () => act(() => hub.api.atemCut(id)) });
  const auto = el("button.btn.primary", { type: "button", text: "Auto", title: "Run the transition below at its rate", onclick: () => act(() => hub.api.atemAuto(id)) });
  const more = switcherMore(hub, id, act);
  const remove = el("button.btn.danger", { type: "button", text: "Disconnect", onclick: () => act(async () => {
    await hub.api.atemDisconnect(id);
    shell.drawer(null);
  }) });
  const root = el("div.pad.col.cam-props.cam-switcher", { "data-switcher": id }, [
    head.root,
    el("div.row", {}, [dot, line]),
    el("div.cam-label", { text: "How it works with GodwinMix" }),
    modes,
    modeWords,
    pgm.root,
    pvw.root,
    el("div.row", {}, [cut, auto]),
    ...more.sections,
    el("div.row", {}, [el("span.grow"), remove]),
    demoFoot(hub),
  ]);

  function sync() {
    if (!root.isConnected && born) return gone();
    born = born || root.isConnected;
    const a = hub.atemState.get(id);
    const l = listed();
    const up = !!(a ? a.connected : l.connected);
    dot.className = "dot " + (up ? "live" : "failed");
    line.textContent = up ? [a && a.model, a && a.firmware && `firmware ${a.firmware}`].filter(Boolean).join(", ") || "Connected"
      : `Not connected${l.why ? `: ${l.why}` : ". Check its network cable and address."}`;
    const mode = (a || l).mode || "own";
    for (const b of modes.children) b.setAttribute("aria-checked", String(b.dataset.v === mode));
    modeWords.textContent = (MODES.find((m) => m[0] === mode) || [])[2] || "";
    const me = a && a.mes && a.mes[0];
    for (const n of [pgm.root, pvw.root, cut, auto]) n.hidden = !me;
    if (!me) return more.sync(null);
    pgm.show(plainInputs(a), me.program);
    pvw.show(plainInputs(a), me.preview);
    auto.classList.toggle("on", !!me.running);
    more.sync(a);
  }

  let born = false;
  const offHub = hub.on((what) => what !== "people" && sync());
  function gone() {
    offHub();
    offAtem();
  }
  sync();
  return root;
}
