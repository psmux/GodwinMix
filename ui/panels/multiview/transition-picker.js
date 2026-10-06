// The transition beside Take, the way a vision mixer's panel has it: one
// control that says what the next Take does ("Wipe left 0.5 s") and opens the
// picker, and at most three quick picks, the ones this desk takes with most.
// Everything else is in the picker (transition-sheet.js), which is only built
// while it is open.
//
// The list starts with the built in transitions and grows with whatever
// `program.transitions` adds (a collection's own names, a plugin's, the fx
// library's imported transitions), asked once.

import { el } from "../../shell/dom.js";
import { TransitionState, TYPES } from "./transition-state.js";
import { iconSvg } from "./transition-icons.js";
import { transitionSheet } from "./transition-sheet.js";
import { popover } from "./studio-popover.js";

export { TYPES };

/** The control and its picker. `armedScene()` names the scene For assigns to. */
export function picker({ client = null, armedScene = () => null } = {}) {
  const state = new TransitionState();
  const icon = el("span.transition-icon");
  const name = el("span.tp-name.ellipsis");
  const length = el("span.tp-length");
  const current = el("button.btn.tp-current", {
    type: "button", "aria-haspopup": "dialog",
    title: "The transition Take uses. Opens the list of every transition.",
  }, [icon, el("span.tp-text", {}, [name, length]), el("span.tp-caret", { text: "▾", "aria-hidden": "true" })]);
  const picks = el("div.tp-picks", { role: "group", "aria-label": "Quick transitions" });
  const sheet = transitionSheet(state, { client, armedScene, done: () => pop.close() });
  const pop = popover(sheet.el, current, { onOpen: () => sheet.fill(), onClose: () => sheet.empty() });

  function paint() {
    const k = state.kind();
    icon.innerHTML = iconSvg(state.type);
    name.textContent = state.describe().replace(` ${state.lengthText()}`, "");
    length.textContent = state.lengthText();
    current.setAttribute("aria-label", `Transition: ${state.describe()}. Choose another`);
    picks.replaceChildren(...state.picks().map((type) => {
      const label = state.kind(type).label || type;
      return el("button.btn.tp-pick", {
        type: "button", title: `Take with ${label}`, "aria-label": label,
        "aria-pressed": String(type === state.type),
        onclick: () => state.set({ type }),
      }, [el("span.tp-art", { html: iconSvg(type) }), el("small.ellipsis", { text: label })]);
    }));
    current.dataset.origin = k.origin || "built-in";
    pop.place();
  }
  state.onChange(paint);
  paint();

  return {
    el: current,
    picks,
    sheet: sheet.el,
    state,
    ms: () => state.ms(),
    describe: () => state.describe(),
    request: (durationMs) => state.request(durationMs),
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
        paint();
      } catch {
        /* an older core: the built in list is what it has */
      }
    },
  };
}
