// Why a destination is not sending, in the words the core sends with it.
//
// The core reads the sink's own error ("Connection refused", "Socket I/O
// timed out", "NetStream.Publish.Denied") into `error` on the output's status:
// a `reason` to decide things by and a `message` that says what to do. Before
// that, a destination YouTube never heard from said "Reconnecting, attempt 3"
// and nothing else, and a person could not tell a wrong key from a firewall.
//
// Kept out of panel.js so the header can say the same thing without loading
// the panel's rows.

/** A word or two for each reason, short enough to sit beside the buttons. */
const SHORT = {
  refused: "Refused",
  "timed-out": "No answer",
  "not-found": "Server not found",
  unreachable: "Unreachable",
  rejected: "Key turned away",
  closed: "Hung up",
  stalled: "Stream not taken",
  other: "Failed",
};

/** The failure on a destination that is not live, or null. */
export function failureOf(output) {
  if (!output || output.state === "live" || output.has_key === false) return null;
  const e = output.error;
  return e && e.message ? e : null;
}

/** "Refused, trying again (3)", or null when there is no failure to name. */
export function failureLabel(output) {
  const e = failureOf(output);
  if (!e) return null;
  const word = SHORT[e.reason] || SHORT.other;
  return output.reconnects ? `${word}, trying again (${output.reconnects})` : `${word}, trying again`;
}

/** The sentence with the next step, or "". */
export function failureAdvice(output) {
  const e = failureOf(output);
  return e ? e.message : "";
}

/** How long a new destination is watched for its first answer. */
export const FOLLOW_MS = 60000;

/**
 * After an add or an edit: say so when the destination goes live. A failure
 * needs nothing here, because the core raises an alert with the reason in it,
 * and that is already a toast with a button to the Outputs panel.
 *
 * "Sending started." used to go up the moment the core accepted the address,
 * before anything had dialled anything, and stayed the last word on the
 * matter while YouTube said "No data".
 *
 * @returns {() => void} stops following early
 */
export function followStart(client, id, say) {
  let off = () => {};
  let done = false;
  const stop = () => {
    done = true;
    clearTimeout(timer);
    off();
  };
  const timer = setTimeout(stop, FOLLOW_MS);
  const look = (s) => {
    const o = ((s && s.outputs) || []).find((x) => x.id === id);
    if (!o || done) return;
    if (o.state === "live") {
      stop();
      say(`${id} is live.`);
    } else if (failureOf(o)) {
      stop();
    }
  };
  off = client.onRender(look);
  if (done) off();
  return stop;
}
