// The transition beside Take, the way a vision mixer's panel has it: one
// control that says what the next Take does ("Wipe left 0.5 s") and opens the
// picker, and at most three quick picks, the ones this desk takes with most.
// Everything else is in the picker (transition-sheet.js), which is only built
// while it is open.
//
// The list starts with the built in transitions and grows with whatever
// `program.transitions` adds (a collection's own names, a plugin's, the fx
// library's imported transitions), asked once. Until the person picks one
// here, the control shows the core's default from `fx.assign` and Take names
// it (transition-default.js).

import { el } from "../../shell/dom.js";
import { TransitionState, TYPES } from "./transition-state.js";
import { iconSvg } from "./transition-icons.js";
import { transitionSheet } from "./transition-sheet.js";
import { popover } from "./studio-popover.js";
import { follow, DEFAULT_MS } from "./transition-default.js";

export { TYPES };

/** The control and its picker. `armedScene()` names the scene For assigns to. */
export function picker({ client = null, armedScene = () => null } = {}) {
  const state = new TransitionState();
  let assigned = {};
  let defaultMs = DEFAULT_MS;
  // The armed scene can have its own assignment, so it is read again each time.
  const current = () => (follow(state, assigned, armedScene(), defaultMs), state);
  const adopt = (a) => {
    assigned = a || {};
    paint();
  };
  const icon = el("span.transition-icon");
  const name = el("span.tp-name.ellipsis");
  const length = el("span.tp-length");
  const control = el("button.btn.tp-current", {
    type: "button", "aria-haspopup": "dialog",
    title: "The transition Take uses. Opens the list of every transition.",
  }, [icon, el("span.tp-text", {}, [name, length]), el("span.tp-caret", { text: "▾", "aria-hidden": "true" })]);
  const picks = el("div.tp-picks", { role: "group", "aria-label": "Quick transitions" });
  const sheet = transitionSheet(state, { client, armedScene, done: () => pop.close(), onAssigned: adopt });
  const pop = popover(sheet.el, control, { onOpen: () => sheet.fill(), onClose: () => sheet.empty() });

  function paint() {
    current();
    const k = state.kind();
    icon.innerHTML = iconSvg(state.type);
    name.textContent = state.describe().replace(` ${state.lengthText()}`, "");
    length.textContent = state.lengthText();
    control.setAttribute("aria-label", `Transition: ${state.describe()}. Choose another`);
    picks.replaceChildren(...state.picks().map((type) => {
      const label = state.kind(type).label || type;
      return el("button.btn.tp-pick", {
        type: "button", title: `Take with ${label}`, "aria-label": label,
        "aria-pressed": String(type === state.type),
        onclick: () => state.set({ type }),
      }, [el("span.tp-art", { html: iconSvg(type) }), el("small.ellipsis", { text: label })]);
    }));
    control.dataset.origin = k.origin || "built-in";
    control.dataset.picked = String(state.picked);
    pop.place();
  }
  state.onChange(paint);
  paint();
  // A new armed scene may have its own transition; the panel names it first.
  if (client && client.on) client.on("event", (e) => e && e.name === "preview.changed" && setTimeout(paint, 0));

  return {
    el: control,
    picks,
    sheet: sheet.el,
    state,
    ms: () => current().ms(),
    describe: () => current().describe(),
    request: (durationMs) => current().request(durationMs),
    /** Show the core's assignments, as `fx.list` answers them in `assigned`. */
    adopt,
    /** Count a take, so the quick picks follow what this desk uses. */
    used: () => {
      state.used();
      paint();
    },
    /** Choose a transition by name. */
    choose(type) {
      if (!TYPES.some((t) => t.type === type) && !state.extra.has(type)) state.learn([{ name: type, origin: "collection" }]);
      state.set({ type });
    },
    close: () => pop.close(),
    /** Add the collection's, the plugins' and the fx library's names, once. */
    async load(from = client) {
      try {
        const answer = await from.call("program.transitions", {});
        state.learn((answer && answer.transitions) || []);
        defaultMs = (answer && answer.default_duration_ms) || defaultMs;
        paint();
      } catch {
        /* an older core: the built in list is what it has */
      }
      try {
        adopt(((await from.call("fx.list", {})) || {}).assigned);
      } catch {
        /* no fx library: nothing is assigned, and Take keeps the page's own */
      }
    },
  };
}
