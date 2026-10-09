// Stop streaming and Start streaming on a destination's row.
//
// Remove was the only button that stopped a stream, and it forgot the stream
// key as well. Stop keeps the destination and its key (`output.stop`), asks
// once while it is live, and Start sends to it again (`output.start`).

import { el } from "../../shell/dom.js";
import { errorToast, toast } from "../../shell/toast.js";

/** The button for a row, acting on whatever `current()` is when pressed. */
export function streamToggle(client, current) {
  const stopped = current().state === "stopped";
  return el("button.btn.icon" + (stopped ? ".primary" : ".danger"), {
    text: stopped ? "Start streaming" : "Stop streaming",
    onclick: () => (stopped ? start(client, current()) : stop(client, current())),
  });
}

async function start(client, output) {
  try {
    await client.call("output.start", { id: output.id });
    toast({ text: `Starting ${output.id}. It connects as a new destination does.` });
  } catch (e) {
    errorToast(e, "Start streaming");
  }
}

async function stop(client, output) {
  const [{ stopItem }, { runningThings }] = await Promise.all([import("../header/stop.js"), import("../header/running-model.js")]);
  const item = runningThings({ outputs: [output] })[0];
  if (item) return stopItem(client, item);
  // Not live and not dialling, a destination still waiting for its key say:
  // nothing to ask about.
  try {
    await client.call("output.stop", { id: output.id });
  } catch (e) {
    errorToast(e, "Stop streaming");
  }
}
