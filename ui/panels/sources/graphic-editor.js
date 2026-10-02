// A graphic's fields, in the drawer, applied on air as they are typed.
//
// Opens on what `template.fields` says each field shows now. Each change is
// one `source.set` with `params.fields` holding just the fields touched, a
// fifth of a second after the last keystroke. The mixer draws the SVG again
// once and swaps the picture in on the next frame: no rebuild, no gap, and
// no Apply button. Default puts a field back with `null`.

import { el } from "../../shell/dom.js";
import { shell } from "../../shell/shell.js";
import { errorToast } from "../../shell/toast.js";
import { nameOf } from "./local.js";
import { colourInput, ordered, setRequest } from "./graphic-fields.js";
import { input } from "./graphic-pick.js";

const SETTLE_MS = 200;

export async function openGraphicEditor(panel, source) {
  const client = panel.client;
  let answer;
  try {
    answer = await client.call("template.fields", { id: source.id });
  } catch (e) {
    errorToast(e, `Open ${nameOf(source)}`);
    return;
  }
  const status = el("p.sm.dim", { role: "status", text: "Changes go on air as you make them." });
  const pending = {};
  let timer = null;
  const send = () => {
    timer = null;
    const fields = { ...pending };
    for (const key of Object.keys(pending)) delete pending[key];
    client.call("source.set", setRequest(source.id, fields)).then(
      () => { status.textContent = "On air."; },
      (e) => { status.textContent = e.message || String(e); }
    );
  };
  const change = (name, value) => {
    pending[name] = value;
    status.textContent = "Applying.";
    clearTimeout(timer);
    timer = setTimeout(send, SETTLE_MS);
  };
  const rows = ordered(answer.fields).map((field) => row(field, change));
  shell.drawer(
    el("div.pad.col.graphic-editor", {}, [
      el("div.row", {}, [el("strong.grow", { text: nameOf(source) }), el("button.btn.icon", { text: "×", "aria-label": "Close", onclick: () => shell.drawer(null) })]),
      el("div.sm.dim", { text: `Template ${answer.template}` }),
      ...rows,
      status,
    ])
  );
}

/** One field: its control and a Default button that clears the source's value. */
function row(field, change) {
  const value = field.type === "color" ? colourInput(field.value) : String(field.value ?? "");
  const control = input(field, value, (v) => change(field.name, v));
  const reset = el("button.btn", { text: "Default", title: `Put ${field.label || field.name} back to its default`, "aria-label": `Default ${field.label || field.name}` });
  reset.onclick = () => {
    const back = field.type === "color" ? colourInput(field.default) : String(field.default ?? "");
    control.value = back;
    change(field.name, null);
  };
  return el("label.col.sm", {}, [el("span.dim", { text: field.label || field.name }), el("div.row", {}, [control, reset])]);
}
