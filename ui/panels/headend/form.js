// The small form pieces the headend's setup dialogs share: a labelled
// field, a text box, a number box, a choice, a tick box and a refusal
// box. Plain elements, styled by setup.css, so each dialog reads its own
// answers straight off them.

import { el } from "../../shell/dom.js";

export const field = (label, input, hint, cls = "") => el(`label.field.hs-field${cls}`, {}, [el("span.lbl", { text: label }), input, hint ? el("span.hint", { text: hint }) : null]);

export const text = (value, placeholder, label, extra = {}) => el("input", { type: "text", value: value ?? "", placeholder: placeholder || "", "aria-label": label, spellcheck: "false", autocomplete: "off", ...extra });

export const num = (value, placeholder, label, extra = {}) => el("input", { type: "number", value: value ?? "", placeholder: placeholder || "", "aria-label": label, ...extra });

/** A select of [value, words] rows. */
export function choice(rows, value, label) {
  const s = el("select", { "aria-label": label }, rows.map(([v, t]) => el("option", { value: v, text: t })));
  s.value = value ?? (rows[0] && rows[0][0]);
  return s;
}

export const tick = (checked, words, label) => {
  const box = el("input", { type: "checkbox", "aria-label": label || words });
  box.checked = !!checked;
  return { box, node: el("label.hs-tick", {}, [box, el("span", { text: words })]) };
};

/** A group of fields under a heading. */
export const group = (title, children, cls = "") => el(`fieldset.hs-group${cls}`, {}, [el("legend", { text: title }), ...children]);

/** Where the station's refusals are said, in the dialog, by the button. */
export function whyBox() {
  const node = el("div.hs-why", { role: "alert", hidden: true });
  return {
    node,
    say(lines) {
      const list = (Array.isArray(lines) ? lines : [lines]).filter(Boolean);
      node.hidden = !list.length;
      node.replaceChildren(...list.map((l) => el("p", { text: l })));
    },
  };
}

/** The message of an error, as the station wrote it. */
export const said = (e) => (e && e.message) || String(e);
