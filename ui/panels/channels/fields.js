// The few form fields the channel dialogs use: a labelled box, a key box
// that hides what is typed until asked, a switch, and a choice of stream.
// Each hands back its node and a way to read it.

import { el } from "../../shell/dom.js";

let n = 0;
const nextId = () => `chn-f${++n}`;

export function field(label, opts = {}) {
  const id = nextId();
  const input = el("input" + (opts.mono ? ".chn-mono" : ""), { id, type: "text", value: opts.value || "", placeholder: opts.placeholder || "", autocomplete: "off", spellcheck: "false" });
  const node = el("div.chn-field", {}, [el("label", { for: id, text: label }), input, opts.note ? el("small.chn-dim", { text: opts.note }) : null]);
  input.addEventListener("input", () => node.classList.remove("bad"));
  return { node, input, value: () => input.value, focus: () => input.focus(), bad: () => { node.classList.add("bad"); input.focus(); } };
}

/**
 * A stream key. Hidden as it is typed, with Show beside it. `locked` is the
 * edit form's: the box stays shut until Replace key is pressed, so a stray
 * keystroke on the way to another field cannot change a live key.
 */
export function keyField(label, placeholder, locked = false) {
  const f = field(label, { placeholder, mono: true });
  f.input.type = "password";
  f.input.autocomplete = "new-password";
  const show = el("button.chn-show", { type: "button", text: "Show", onclick: () => {
    const hidden = f.input.type === "password";
    f.input.type = hidden ? "text" : "password";
    show.textContent = hidden ? "Hide" : "Show";
  } });
  const row = el("div.chn-keyrow", {}, [f.input, show]);
  f.node.insertBefore(row, f.node.children[1] || null);
  if (locked) {
    f.input.disabled = true;
    show.hidden = true;
    const replace = el("button.btn.sm", { type: "button", text: "Replace key", onclick: () => {
      f.input.disabled = false;
      f.input.placeholder = "Paste the new key";
      show.hidden = false;
      replace.remove();
      f.input.focus();
    } });
    row.appendChild(replace);
  }
  return f;
}

/** An on and off switch with its words beside it. */
export function toggle(label, checked, note) {
  const box = el("input", { type: "checkbox", checked: !!checked });
  const node = el("label.chn-toggle", {}, [
    el("span.chn-switch", {}, [box, el("span.chn-knob")]),
    el("span", {}, [el("strong", { text: label }), note ? el("small.chn-dim", { text: note }) : null]),
  ]);
  return { node, box, value: () => box.checked };
}

/**
 * Which stream a destination sends, when the channel has more than one to
 * choose from. `*` is whichever is live first, which is right for nearly
 * everybody, so with one stream or none there is nothing to ask.
 */
export function streamChoice(channel, current) {
  const names = [...new Set((channel.streams || []).map((s) => s.name))];
  if (current && current !== "*" && !names.includes(current)) names.push(current);
  if (names.length < 2 && (!current || current === "*")) return null;
  const id = nextId();
  const select = el("select", { id }, [
    el("option", { value: "*", text: "Whichever stream is live first" }),
    ...names.map((s) => el("option", { value: s, text: s })),
  ]);
  select.value = current || "*";
  return { node: el("div.chn-field", {}, [el("label", { for: id, text: "Which stream" }), select]), value: () => select.value };
}
