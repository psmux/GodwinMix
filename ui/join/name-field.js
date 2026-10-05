// The name field on /join/ when the link names a channel and not a stream:
// the person holding the phone says what it is called, and that is the
// stream it publishes and the source it becomes. Left alone, the phone has a
// made up name of its own (`name.js`), the same one every time.

import { el } from "../shell/dom.js";
import { deviceName, rename } from "./name.js";

/**
 * `app` is the channel's application name, which the source id starts with.
 * Answers `{node, stream(), lock(on)}`: `stream()` is the name to publish
 * under now, `lock` stops it changing while live.
 */
export function nameField(app) {
  let current = deviceName();
  const input = el("input.pub-name", {
    type: "text",
    value: current.label,
    placeholder: "Ana's phone",
    autocomplete: "off",
    autocapitalize: "words",
    enterkeyhint: "done",
    maxLength: 40,
    "aria-label": "Name on the mixer",
  });
  const hint = el("div.sm.dim.pub-name-hint");
  const paint = () => {
    hint.textContent = `It becomes the source ${app}-${current.stream} on the mixer.`;
  };
  const read = () => {
    if (input.value.trim()) current = rename(input.value);
    paint();
    return current.stream;
  };
  input.addEventListener("input", read);
  paint();
  const node = el("div.pub-namebox.col", {}, [
    el("label.pub-field.row.sm", {}, [el("span.pub-label.dim", { text: "Name" }), input]),
    hint,
  ]);
  return { node, stream: read, lock: (on) => { input.disabled = !!on; } };
}
