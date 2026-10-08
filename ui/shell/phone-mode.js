// Which shell a screen gets: the dock on a desk, the phone deck on a phone.
//
// Width alone decides, at the same 760px every narrow rule in the themes
// uses, so a tablet held upright is a phone and turned sideways is a desk.
// The deck and its stylesheet are fetched the first time a narrow screen asks
// for them. A desk never downloads either.

export const NARROW = "(max-width: 760px)";
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
  loading ||= import("./phone.js").then((m) => m.ready());
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
