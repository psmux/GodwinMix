// Demo on or off: the window moves to the pretend mixer in demo/, or back
// to the real one, and carries on reading if it was on screen.

import { save } from "./head.js";

export async function switchClient(view, on) {
  save({ demo: on });
  let client = view.real;
  if (on) {
    const { demoClient } = await import("./demo/client.js");
    client = view.demoClient || (view.demoClient = demoClient());
  }
  // Read after the import: the panel may have come on screen meanwhile.
  const was = view.running;
  view.stop();
  view.client = client;
  view.offMix?.();
  view.model.setClient(client);
  view.strip.reset();
  view.offMix = client.onRender ? client.onRender((s) => ((view.model.mix = s), view.later())) : null;
  if (was) view.start();
  view.later();
}
