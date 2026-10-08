// Which shell a screen gets: the dock on a desk, the phone deck on a phone.
//
// A screen 760px wide or less is a phone, the same line every narrow rule in
// the themes uses, so a tablet held upright is a phone and turned sideways is
// a desk. A phone turned sideways is wider than that but has a finger and
// under 500px of height, which no desk has, so it stays a phone. The deck and
// its stylesheet are fetched the first time such a screen asks for them. A
// desk never downloads either.

export const NARROW = "(max-width: 760px), (max-height: 500px) and (pointer: coarse)";
let loading = null;

/** True when the phone deck owns the workspace and the dock should draw nothing. */
export function phoneOwns(workspace) {
  if (!matchMedia(NARROW).matches) {
    workspace.phoneMode = false;
    if (workspace.phone) {
      workspace.phone.destroy();
      workspace.phone = null;
    }
    return false;
  }
  workspace.phoneMode = true;
  if (workspace.phone) {
    workspace.phone.render();
    return true;
  }
  dropFrames(workspace);
  loading ||= Promise.all([import("./phone.js"), sheet()]).then(([m]) => m);
  loading.then((m) => {
    if (workspace.phone || !matchMedia(NARROW).matches) return;
    workspace.phone = new m.PhoneDeck(workspace);
    workspace.phone.render();
  });
  return true;
}

/** Turning a tablet across the line swaps one shell for the other. */
export function watchWidth(workspace) {
  matchMedia(NARROW).addEventListener("change", () => workspace.render());
}

/** The dock's frames and splitters, let go before the deck builds its own. */
function dropFrames(w) {
  for (const frame of w.frames.values()) {
    frame.made.destroy();
    frame.element.remove();
  }
  w.frames.clear();
  for (const split of w.splits.values()) split.element.remove();
  w.splits.clear();
}

/** The deck's stylesheet, waited for so its first paint is already styled. */
function sheet() {
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = new URL("../themes/phone.css", import.meta.url).href;
  document.head.appendChild(link);
  return new Promise((done) => {
    link.addEventListener("load", done, { once: true });
    link.addEventListener("error", done, { once: true });
    setTimeout(done, 1500);
  });
}
