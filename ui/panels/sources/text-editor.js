// The settings of a text or a ticker, applied on air as they change.
//
// Each change is one `source.set` with the params it touches, a fifth of a
// second after the last keystroke. The core renders the words again once and
// swaps the picture in place: no rebuild, no gap, so there is no Apply button
// to forget. Opened by the gear in place of the generic settings form.

import { el } from "../../shell/dom.js";
import { shell } from "../../shell/shell.js";
import { nameOf } from "./local.js";
import { WEIGHTS, current, fieldsFor, paramsFor, splitAlpha, withAlpha } from "./text-fields.js";

const SETTLE_MS = 200;

export function openTextEditor(panel, source) {
  const client = panel.client;
  const type = source.type;
  const values = current(type, source.params);
  const status = el("p.sm.dim", { role: "status", text: "Changes go on air as you make them." });
  const pending = {};
  let timer = null;
  const send = () => {
    timer = null;
    const params = Object.assign({}, ...Object.entries(pending).map(([k, v]) => paramsFor(type, k, v)));
    for (const key of Object.keys(pending)) delete pending[key];
    client.call("source.set", { id: source.id, params }).then(
      () => { status.textContent = "On air."; },
      (e) => { status.textContent = e.message || String(e); }
    );
  };
  const change = (key, value) => {
    pending[key] = value;
    status.textContent = "Applying.";
    clearTimeout(timer);
    timer = setTimeout(send, SETTLE_MS);
  };
  const rows = fieldsFor(type).map((f) => el("label.col.sm", {}, [el("span.dim", { text: f.label }), control(f, values[f.key], (v) => change(f.key, v))]));
  shell.drawer(
    el("div.pad.col.text-editor", {}, [
      el("div.row", {}, [el("strong.grow", { text: nameOf(source) }), el("button.btn.icon", { text: "×", "aria-label": "Close", onclick: () => shell.drawer(null) })]),
      ...rows,
      status,
      el("button.btn.live-data", {
        type: "button",
        text: "Fill from live data…",
        title: "Keep these words current from an RSS, JSON or CSV feed",
        onclick: () => import("../data/dialog.js").then((m) => m.openLiveData(client, { source })),
      }),
    ])
  );
}

/** One control for one field, calling `set` with the value it now holds. */
function control(f, value, set) {
  switch (f.kind) {
    case "words": {
      const area = el("textarea", { rows: 4, "aria-label": f.label });
      area.value = value || "";
      area.oninput = () => set(area.value);
      return area;
    }
    case "number": {
      const input = el("input", { type: "number", min: f.min, max: f.max, value, "aria-label": f.label });
      input.oninput = () => input.value !== "" && set(Number(input.value));
      return el("div.row", {}, [input, el("span.dim", { text: f.unit || "" })]);
    }
    case "check": {
      const box = el("input", { type: "checkbox", "aria-label": f.label });
      box.checked = !!value;
      box.onchange = () => set(box.checked);
      return box;
    }
    case "colour": {
      const pick = el("input", { type: "color", value: splitAlpha(value).hex, "aria-label": f.label });
      pick.oninput = () => set(pick.value);
      return pick;
    }
    case "weight":
    case "choice": {
      const options = f.kind === "weight" ? WEIGHTS : f.options;
      const select = el("select", { "aria-label": f.label }, options.map(([v, label]) => el("option", { value: String(v), text: label })));
      select.value = String(value);
      select.onchange = () => set(f.kind === "weight" ? Number(select.value) : select.value);
      return select;
    }
    case "optional-colour":
      return optional(f, value, set);
    case "box":
      return boxControl(f, value, set);
    default: {
      const input = el("input", { type: "text", value: value || "", "aria-label": f.label });
      input.oninput = () => set(input.value);
      return input;
    }
  }
}

/** A colour that can be switched off, which writes an empty string. */
function optional(f, value, set) {
  const on = el("input", { type: "checkbox", "aria-label": `${f.label} on` });
  on.checked = !!value;
  const pick = el("input", { type: "color", value: splitAlpha(value || f.off).hex, "aria-label": `${f.label} colour` });
  const write = () => set(on.checked ? pick.value : "");
  on.onchange = write;
  pick.oninput = () => { on.checked = true; write(); };
  return el("div.row", {}, [on, pick]);
}

/** The box behind the words: on or off, a colour, and how see-through. */
function boxControl(f, value, set) {
  const { hex, opacity } = splitAlpha(value || "#000000b3");
  const on = el("input", { type: "checkbox", "aria-label": "Box on" });
  on.checked = !!value;
  const pick = el("input", { type: "color", value: hex, "aria-label": "Box colour" });
  const solid = el("input", { type: "range", min: 0, max: 1, step: 0.05, value: opacity, "aria-label": "Box opacity" });
  const write = () => set(on.checked ? withAlpha(pick.value, Number(solid.value)) : "");
  on.onchange = write;
  pick.oninput = () => { on.checked = true; write(); };
  solid.oninput = () => { on.checked = true; write(); };
  return el("div.row", {}, [on, pick, solid]);
}
