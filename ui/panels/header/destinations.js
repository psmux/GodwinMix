// The header's destinations pill: how many are live, and when none is, the
// worst thing about the rest.
//
// "Destinations connecting" used to cover a destination the server had been
// refusing for ten minutes as well as one dialling for the first second, so
// the pill said nothing was wrong while nothing was going out. A failure the
// core gave a reason for is "failing" here, and the hover says the reason.

import { failureOf } from "../outputs/failure.js";

/**
 * @param {object[]} outputs  the status's outputs
 * @returns {{text: string, title: string, kind: "none"|"live"|"failed"|"connecting"|"key"}}
 */
export function destinationsPill(outputs) {
  const all = (outputs || []).filter((o) => o.type !== "record/output");
  // A destination a person stopped is not connecting and is not failing.
  const streams = all.filter((o) => o.state !== "stopped");
  if (!streams.length && all.length) {
    const n = all.length;
    return { text: "Streaming stopped", title: `${n} destination${n === 1 ? " is" : "s are"} stopped, with the stream key kept. Start streaming on Outputs sends again.`, kind: "none" };
  }
  if (!streams.length) {
    return { text: "No destinations", title: "Nothing is sending the programme anywhere. Add a destination under Outputs.", kind: "none" };
  }
  const live = streams.filter((o) => o.state === "live").length;
  const failing = streams.filter((o) => failureOf(o));
  const reasons = failing.map((o) => `${o.id}: ${o.error.message}`).join("\n");
  if (live) {
    const text = `${live} destination${live === 1 ? "" : "s"} live` + (failing.length ? `, ${failing.length} failing` : "");
    // Still red and filled: something is on air, and that is what the
    // fill means. The count and the hover carry the rest.
    return { text, title: reasons, kind: "live" };
  }
  if (failing.length) {
    const text = failing.length === 1 ? `${failing[0].id} is not sending` : `${failing.length} destinations not sending`;
    return { text, title: reasons, kind: "failed" };
  }
  // One still waiting for its key is not connecting and never will be.
  const dialling = streams.some((o) => o.has_key !== false);
  return dialling
    ? { text: "Destinations connecting", title: "", kind: "connecting" }
    : { text: "Destinations need a key", title: "Add the stream key under Outputs, Add key.", kind: "key" };
}
