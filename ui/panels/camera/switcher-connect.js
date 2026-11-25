// Adding an ATEM switcher, the way a destination is added: its address, a
// name people know it by, and how it works with GodwinMix. The plugin keeps
// trying to reach it until it is disconnected, so a switcher that is off
// when it is added connects when it comes on.

import { el } from "../../shell/dom.js";
import { errorToast, toast } from "../../shell/toast.js";
import { drawerHead, demoFoot } from "./look.js";
import { MODES } from "./words.js";

/** "Main switcher" is atem-main-switcher, unless that is taken. */
export function switcherSlug(name, taken) {
  const base = "atem-" + (String(name).toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").replace(/^atem-?/, "") || "switcher");
  let id = base.replace(/-$/, "");
  for (let n = 2; taken.has(id); n++) id = `${base}-${n}`;
  return id;
}

export function connectForm(hub, opened) {
  const head = drawerHead(hub, "Add a switcher", "ATEM");
  const address = el("input", { type: "text", placeholder: "192.0.2.10", "aria-label": "Address", spellcheck: "false" });
  const name = el("input", { type: "text", placeholder: "Main switcher", "aria-label": "Name" });
  let mode = "follow";
  const modes = el("div.cam-seg", { role: "radiogroup", "aria-label": "How it works with GodwinMix" },
    MODES.map(([v, t, tip]) => el("button.btn.sm", { type: "button", role: "radio", text: t, title: tip, "data-v": v, onclick: () => pick(v) })));
  const words = el("p.sm.dim.cam-mode-words");
  const pick = (v) => {
    mode = v;
    for (const b of modes.children) b.setAttribute("aria-checked", String(b.dataset.v === v));
    words.textContent = MODES.find((m) => m[0] === v)[2];
  };
  pick(mode);
  const go = el("button.btn.primary", { type: "button", text: "Connect" });
  go.onclick = async () => {
    const where = address.value.trim();
    if (!where) return toast({ text: "Type the switcher's address first. Its own setup utility shows it." });
    const called = name.value.trim() || "Switcher";
    const id = switcherSlug(called, new Set(hub.atems.map((a) => a.id)));
    go.disabled = true;
    try {
      await hub.api.atemConnect({ id, address: where, name: called, mode });
      await hub.refresh();
      opened(id);
    } catch (e) {
      errorToast(e, "Connect");
    } finally {
      go.disabled = false;
    }
  };
  return el("div.pad.col.cam-props.cam-switcher", {}, [
    head.root,
    el("label.sm.dim", { text: "Address" }, [address]),
    el("label.sm.dim", { text: "Name" }, [name]),
    el("div.cam-label", { text: "How it works with GodwinMix" }),
    modes,
    words,
    el("div.row", {}, [go]),
    el("p.sm.faint", { text: "Map the cameras to the switcher's inputs once it is connected, under Sources and inputs." }),
    demoFoot(hub),
  ]);
}
