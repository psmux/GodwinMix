// Setting up a piece the mixer carries but has not set up yet: a first party
// plugin, or the web page renderer. `setup.start` answers at once and
// `setup.changed` follows; this waits for the end and says what happened.
//
// A piece the mixer does not carry (a third party plugin) is not a setup
// piece: `setup.start` refuses it as not found, and the caller installs it
// with `plugin.add` as before.

import { CODES } from "./errors.js";

const LONGEST_MS = 20 * 60 * 1000;

/**
 * Resolve once `piece` is ready; reject with the core's own sentence when it
 * did not finish. `say` hears each plain progress sentence.
 */
export function setUp(client, piece, say = () => {}) {
  return new Promise((resolve, reject) => {
    let settled = false;
    const finish = (s) => {
      if (settled || !s || s.piece !== piece) return;
      if (s.message) say(s.message);
      if (s.state === "running" || s.state === "missing") return;
      settled = true;
      off();
      clearTimeout(timer);
      if (s.state === "ready") resolve(s);
      else reject(Object.assign(new Error(s.message), { setup: s, action: s.action }));
    };
    const off = client.on("setup", finish);
    const timer = setTimeout(() => finish({ piece, state: "failed", message: "Setting this up is taking longer than it should. Press Try again." }), LONGEST_MS);
    client.call("setup.start", { piece }).then(finish, (e) => {
      settled = true;
      off();
      clearTimeout(timer);
      reject(e);
    });
  });
}

/**
 * True when the core does not set `piece` up itself: a plugin it does not
 * carry, or a core older than `setup.start`.
 */
export function notASetupPiece(e) {
  return !!(e && (e.code === CODES.NOT_FOUND || e.code === CODES.NO_METHOD));
}
