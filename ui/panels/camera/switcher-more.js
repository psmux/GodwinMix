// The folded sections of switcher properties: which ATEM input shows each
// GodwinMix source, the transition style and rate, the downstream and
// upstream keyers with fade to black, the aux outputs and the macros. Each
// section is rebuilt only when what it lists changes, and written into on
// every read after that.

import { el } from "../../shell/dom.js";

const STYLES = [["mix", "Mix"], ["dip", "Dip"], ["wipe", "Wipe"]];

const section = (title, ...kids) => el("details.panel-section.cam-more", {}, [el("summary", { text: title }), el("div.col.pad", {}, kids)]);
const keyBtn = (text, title, onclick) => el("button.btn.sm.cam-key", { type: "button", text, title, onclick });

/** A select of the switcher's inputs, with `none` first when given. */
function inputSelect(label, inputs, none, onchange) {
  const s = el("select", { "aria-label": label, onchange: () => onchange(s.value === "" ? null : Number(s.value)) },
    [none ? el("option", { value: "", text: none }) : null, ...inputs.map((i) => el("option", { value: String(i.id), text: i.name }))]);
  return s;
}

export function switcherMore(hub, id, act) {
  const map = el("div.col.cam-map");
  const style = el("div.cam-seg", { role: "radiogroup", "aria-label": "Transition style" },
    STYLES.map(([v, t]) => el("button.btn.sm", { type: "button", role: "radio", text: t, "data-v": v, onclick: () => act(() => hub.api.atemTransition(id, { style: v })) })));
  const rate = el("input.num.cam-rate", { type: "number", min: "1", max: "250", "aria-label": "Rate in frames" });
  rate.onchange = () => act(() => hub.api.atemTransition(id, { rate: Number(rate.value) }));
  const bar = el("div.cam-tbar", { "aria-hidden": "true" }, [el("div.cam-tbar-fill")]);
  const keys = el("div.cam-row-wrap");
  const auxes = el("div.col");
  const macros = el("div.cam-row-wrap");
  const sections = [
    section("Sources and inputs", el("p.sm.dim", { text: "Which switcher input shows each GodwinMix source. Following, leading and tally go by this." }), map),
    section("Transition", el("div.row", {}, [style, el("label.cam-field", {}, [el("span", { text: "Rate, frames" }), rate])]), bar),
    section("Keys and fade to black", keys),
    section("Aux outputs", auxes),
    section("Macros", macros),
  ];
  const built = { map: "", keys: "", aux: "", mac: "" };

  function rebuild(a) {
    const sources = ((hub.real.state && hub.real.state.sources) || []).filter((s) => s.has_video !== false);
    const plain = a.inputs.filter((i) => i.id > 0 && i.id < 1000);
    const k1 = sources.map((s) => s.id + s.name).join() + plain.map((i) => i.name).join();
    if (k1 !== built.map) {
      built.map = k1;
      map.replaceChildren(...sources.map((s) => el("label.row.cam-maprow", {}, [el("span.grow.ellipsis", { text: s.name || s.id }),
        inputSelect(`Input for ${s.name || s.id}`, plain, "Not on the switcher", (input) => act(() => hub.api.atemMap(id, s.id, input)))])));
      if (!sources.length) map.append(el("span.sm.dim", { text: "This mixer has no video sources yet." }));
    }
    const k2 = `${a.dsks.length}/${a.usks.length}`;
    if (k2 !== built.keys) {
      built.keys = k2;
      keys.replaceChildren(
        ...a.dsks.map((_, i) => keyBtn(`DSK ${i + 1}`, `Downstream key ${i + 1}, on or off with its own transition`, () => act(() => hub.api.atemDsk(id, i, "auto")))),
        ...a.usks.map((_, i) => keyBtn(`Key ${i + 1}`, `Upstream key ${i + 1} on or off`, () => act(() => hub.api.atemUsk(id, i, !(hub.atemState.get(id) || a).usks[i].on)))),
        keyBtn("FTB", "Fade to black, or back from it", () => act(() => hub.api.atemFtb(id))));
      keys.lastChild.classList.add("ftb");
    }
    const k3 = a.auxes.length + "/" + a.inputs.map((i) => i.id).join();
    if (k3 !== built.aux) {
      built.aux = k3;
      auxes.replaceChildren(...a.auxes.map((_, i) => el("label.row.cam-maprow", {}, [el("span.grow", { text: `Aux ${i + 1}` }),
        inputSelect(`Aux ${i + 1}`, a.inputs, null, (input) => act(() => hub.api.atemAux(id, i, input)))])));
      if (!a.auxes.length) auxes.append(el("span.sm.dim", { text: "This switcher has no aux outputs." }));
    }
    const k4 = a.macros.map((m) => m.name).join();
    if (k4 !== built.mac || !macros.firstChild) {
      built.mac = k4;
      macros.replaceChildren(...a.macros.map((m) => keyBtn(m.name, `Run macro ${m.index + 1}`, () => act(() => hub.api.atemMacro(id, m.index)))));
      if (!a.macros.length) macros.append(el("span.sm.dim", { text: "This switcher has no macros saved." }));
    }
  }

  const write = (sel, v) => document.activeElement !== sel && (sel.value = v === null || v === undefined ? "" : String(v));

  return {
    sections,
    sync(a) {
      for (const s of sections) s.hidden = !a;
      if (!a || !a.mes || !a.mes[0]) return;
      rebuild(a);
      const me = a.mes[0];
      for (const b of style.children) b.setAttribute("aria-checked", String(b.dataset.v === me.style));
      if (document.activeElement !== rate) rate.value = me.rate ?? "";
      bar.firstChild.style.width = `${Math.round((me.position || 0) * 100)}%`;
      map.querySelectorAll("select").forEach((s, i) => {
        const src = ((hub.real.state && hub.real.state.sources) || []).filter((x) => x.has_video !== false)[i];
        if (src) write(s, a.map[src.id]);
      });
      const btns = [...keys.children];
      a.dsks.forEach((d, i) => btns[i].classList.toggle("on", d.on));
      a.usks.forEach((u, i) => btns[a.dsks.length + i].classList.toggle("on", u.on));
      btns[btns.length - 1].classList.toggle("on", !!a.ftb.on);
      auxes.querySelectorAll("select").forEach((s, i) => write(s, a.auxes[i].source));
      [...macros.children].forEach((b, i) => b.classList && b.classList.toggle("on", a.lastMacro === (a.macros[i] || {}).index));
    },
  };
}
