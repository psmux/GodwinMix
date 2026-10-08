// What a clip does at its end, on its tile.
//
// A clip's `params.at_end` is one of three words: `repeat`, `hold` or
// `leave`. The tile shows the one choice people make most, Repeat, as a
// toggle; which of the other two Repeat off means is set in the clip's
// settings drawer, from the kind's own schema. The mixer says the current
// word in the clip's status row as `at_end`, and `ended: true` while it is
// holding its last frame. A core from before 0.2.3 says neither, and the
// toggle stays hidden.
//
// Its own module, with nothing imported but the DOM helper, so the tests can
// reach it.

import { el } from "../../shell/dom.js";

/** What Repeat off went back to, per clip, for as long as the page is open. */
const stopped = new Map();

/** The `source.set` request for Repeat on or off. */
export function repeatRequest(source, on) {
  if (on && source.at_end && source.at_end !== "repeat") stopped.set(source.id, source.at_end);
  const at_end = on ? "repeat" : stopped.get(source.id) || "hold";
  return { id: source.id, params: { at_end } };
}

/** The toggle. `onRepeat(source, on)` is the panel's, and makes the call. */
export function repeatButton(source, onRepeat) {
  const button = el("button.btn.sm.repeat", {
    text: "Repeat",
    title: "Play the clip again from the start when it ends",
    "aria-pressed": "false",
    "data-nodrag": "",
    hidden: true,
  });
  button.onclick = (e) => {
    e.stopPropagation();
    onRepeat(button.source || source, button.getAttribute("aria-pressed") !== "true");
  };
  return button;
}

/** Write the clip's current choice into its toggle. */
export function syncRepeat(button, source) {
  button.source = source;
  button.hidden = !source.at_end;
  const on = source.at_end === "repeat";
  button.classList.toggle("on", on);
  button.setAttribute("aria-pressed", String(on));
  button.title = on
    ? "Repeating: the clip plays again from the start when it ends"
    : source.at_end === "leave"
      ? "Repeat is off: at its end the clip holds its last frame and the programme leaves the scene"
      : "Repeat is off: at its end the clip holds its last frame";
}

/** What the clip label says. */
export function playbackText(source) {
  if (!source.seekable) return "Continuous live source";
  return source.ended ? "Clip, ended" : "Clip";
}
