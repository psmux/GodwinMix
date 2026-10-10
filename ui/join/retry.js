// When a publish that dropped is tried again, and when it is not.
//
// A phone on Wi-Fi loses its network for a second, walks out of range onto
// cellular, or is put in a pocket with the page in the background. None of
// that is a reason to stop: the page publishes again, for as long as the
// person leaves it open, until somebody presses Stop. The only refusals
// that end it are the ones waiting cannot cure: a key the channel does not
// have (401, 403), a request this page made wrong (415), and a browser that
// can send no codec the mixer takes (0, raised here before anything is
// sent). A 404 is not one of them: a mixer or a station that is restarting
// answers 404 for a moment before its channels are back.

import { WhipError } from "./whip.js";

/** Refusals that publishing again cannot fix. Everything else is retried. */
const FINAL = new Set([0, 401, 403, 415]);

/** 1, 2, 4, 8 seconds, then every 15. */
export function backoff(failures) {
  return Math.min(15000, 1000 * 2 ** Math.max(0, failures));
}

export function isFinal(error) {
  return error instanceof WhipError && FINAL.has(error.status);
}

/**
 * How long a "disconnected" connection is given to come back by itself
 * before the page offers again from scratch. The mixer keeps its end of a
 * disconnected session for fifteen seconds (`plugins/ingest/src/whip/
 * session/watch.rs`), longer than this, so it never drops a session the
 * page still expects back; and when the page does offer again, the new
 * session takes over the old one at once.
 */
export const GRACE_MS = 5000;

/**
 * After the network changes (Wi-Fi to cellular, a new access point), a
 * connection that reads "disconnected" within this long is not waited for:
 * its path went with the old network, and only a new offer finds the new one.
 */
export const MOVED_MS = 15000;
