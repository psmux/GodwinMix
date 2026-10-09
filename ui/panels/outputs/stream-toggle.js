// Stop streaming and Start streaming on a destination's row.
//
// Remove was the only button that stopped a stream, and it forgot the stream
// key as well. Stop keeps the destination and its key (`output.stop`), asks
// once while it is live, and Start sends to it again (`output.start`).

import { errorToast, toast } from "../../shell/toast.js";

/** What the row's button does: start a stopped destination, stop any other. */
export function toggle(client, output) {
  return output.state === "stopped" ? start(client, output) : stop(client, output);
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
