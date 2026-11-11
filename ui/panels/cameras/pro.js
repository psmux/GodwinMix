// Pro view, for the person who knows what an M/E is: the transition style
// and rate with its bar, the upstream keyers, every aux output, the macros
// and the switcher's mode. The ATEM bar above it shows every input on both
// buses while this is open.

import { el } from "../../shell/dom.js";

const STYLES = [["mix", "Mix"], ["dip", "Dip"], ["wipe", "Wipe"]];
const MODES = [["own", "On its own"], ["follow", "Follows GodwinMix"], ["lead", "Leads GodwinMix"]];

const group = (title, ...kids) => el("div.cam-pro-group", {}, [el("div.cam-label", { text: title }), ...kids]);
const seg = (items, onPick) => el("div.cam-seg", { role: "radiogroup" }, items.map(([v, t]) => el("button", { type: "button", role: "radio", text: t, "data-v": v, onclick: () => onPick(v) })));
const check = (node, v) => [...node.children].forEach((b) => b.setAttribute("aria-checked", String(b.dataset.v === v)));

export function proView(model, act) {
  const sw = () => model.atemId;
  const style = seg(STYLES, (s) => act(() => model.api.atemTransition(sw(), { style: s })));
  const rate = el("input.cam-rate.num", { type: "number", min: "1", max: "250", "aria-label": "Transition rate in frames" });
  rate.addEventListener("change", () => act(() => model.api.atemTransition(sw(), { rate: Number(rate.value) })));
  const bar = el("div.cam-tbar", { "aria-hidden": "true" }, [el("div.cam-tbar-fill")]);
  const auto = el("button.cam-key", { type: "button", text: "Auto", onclick: () => act(() => model.api.atemAuto(sw())) });
  const cut = el("button.cam-key", { type: "button", text: "Cut", onclick: () => act(() => model.api.atemCut(sw())) });
  const usks = el("div.cam-row-wrap");
  const auxes = el("div.cam-pro-aux");
  const macros = el("div.cam-row-wrap");
  const mode = seg(MODES, (m) => act(() => model.api.atemMode(sw(), m)));
  const root = el("section.cam-pro", { "aria-label": "Pro view" }, [
    group("Transition", el("div.cam-row-wrap", {}, [style, el("label.cam-field", {}, [el("span", { text: "Rate, frames" }), rate]), cut, auto]), bar),
    group("Upstream keys", usks),
    group("Aux outputs", auxes),
    group("Macros", macros),
    group("How it works with GodwinMix", mode),
  ]);
  const keys = { usk: "", aux: "", mac: "" };

  function rebuild(atem) {
    const k = (atem.usks || []).length + sw();
    if (k !== keys.usk) {
      keys.usk = k;
      usks.replaceChildren(...(atem.usks || []).map((_, i) =>
        el("button.cam-key", { type: "button", text: `Key ${i + 1}`, onclick: () => act(() => model.api.atemUsk(sw(), i, !model.atem.usks[i].on)) })));
    }
    const a = (atem.auxes || []).length + sw() + atem.inputs.length;
    if (a !== keys.aux) {
      keys.aux = a;
      auxes.replaceChildren(...(atem.auxes || []).map((_, i) => {
        const s = el("select", { "aria-label": `Aux ${i + 1}`, onchange: () => act(() => model.api.atemAux(sw(), i, Number(s.value))) },
          atem.inputs.map((inp) => el("option", { value: String(inp.id), text: inp.name })));
        return el("label.cam-field", {}, [el("span", { text: `Aux ${i + 1}` }), s]);
      }));
    }
    const m = (atem.macros || []).map((x) => x.name).join() + sw();
    if (m !== keys.mac) {
      keys.mac = m;
      macros.replaceChildren(...(atem.macros || []).map((x) => el("button.cam-key.macro", { type: "button", text: x.name, onclick: () => act(() => model.api.atemMacro(sw(), x.index)) })));
      if (!(atem.macros || []).length) macros.append(el("span.dim", { text: "This switcher has no macros saved." }));
    }
  }

  return {
    root,
    sync() {
      const atem = model.atem;
      root.hidden = !model.pro || !atem || !atem.mes;
      if (root.hidden) return;
      rebuild(atem);
      const me = atem.mes[0];
      check(style, me.style);
      check(mode, atem.mode);
      if (document.activeElement !== rate) rate.value = String(me.rate);
      bar.firstChild.style.width = `${Math.round((me.position || 0) * 100)}%`;
      [...usks.children].forEach((b, i) => b.classList.toggle("on", !!atem.usks[i].on));
      [...auxes.querySelectorAll("select")].forEach((s, i) => document.activeElement !== s && (s.value = String(atem.auxes[i].source)));
      [...macros.children].forEach((b, i) => b.classList && b.classList.toggle("on", atem.lastMacro === (atem.macros[i] || {}).index));
    },
  };
}
