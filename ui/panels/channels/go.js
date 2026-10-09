// What the palette's channel commands do once the panel's code is here: bring
// the Channels tab forward, wait for its view to read the channels, then show
// it as rows or open the paste dialog. Loaded only when one is chosen.

import * as panel from "./panel.js";
import { pasteAddresses } from "./paste.js";

/** The view on screen, once it has its first answer, or null after two seconds. */
async function shown() {
  panel.bringForward();
  for (let i = 0; i < 40; i++) {
    const view = panel.current;
    if (view && view.opened) return view;
    await new Promise((r) => setTimeout(r, 50));
  }
  return null;
}

/** Open Channels, as cards or rows, or as it was left when `mode` is not given. */
export async function openChannels(mode) {
  const view = await shown();
  if (view && mode) view.layout.choose(mode);
  return view;
}

/** Paste several addresses, asking which channel when there is more than one. */
export async function pasteSeveral() {
  const view = await shown();
  if (!view) return null;
  const list = view.model.list();
  return pasteAddresses(view, list.length === 1 ? list[0] : null);
}
