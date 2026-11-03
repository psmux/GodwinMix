// Whether the cutouts are ready yet. A cutout's model takes seconds to open
// and the camera shows whole until then, so this asks the core
// (`filter.cutout`) while a cutout is new here, and stops once none prepares.

const EVERY_MS = 600;
// A new cutout may not be in the pipeline yet; trust "none" only after this.
const AT_LEAST_MS = 2500;
// Never asked for longer than this.
const AT_MOST_MS = 60000;

let readiness = null;
let timer = null;
let since = 0;
const listeners = new Set();

/** The last answer: `{preparing, running, error}`, or null before any. */
export function cutoutReadiness() {
  return readiness;
}

/** True while this page knows a cutout is still opening its model. */
export function cutoutPreparing() {
  return !!(readiness && readiness.preparing > 0 && timer);
}

/** Told on every answer that changed something. Returns the unsubscribe. */
export function onCutoutReadiness(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Does this item record carry a cutout that is switched on? */
export function hasCutout(record) {
  return !!(record && (record.filters || []).some((f) => f.type === "matte/filter" && f.enabled !== false));
}

/**
 * Start asking, or keep asking for longer. Called when a cutout is added or
 * switched on here, and when Preview starts showing a scene that has one.
 */
export function watchCutouts(client) {
  if (!client || !client.call) return;
  since = Date.now();
  if (timer) return;
  // Said at once, so the words appear with the click rather than an answer later.
  readiness = Object.assign({ running: 0, error: null }, readiness, { preparing: Math.max(1, (readiness && readiness.preparing) || 0) });
  timer = setTimeout(() => ask(client), 0);
  tell();
}

async function ask(client) {
  let answer = null;
  try {
    answer = await client.call("filter.cutout", {});
  } catch {
    // An older core has no such method: say nothing rather than guess.
    answer = { preparing: 0, running: 0 };
  }
  const was = JSON.stringify(readiness);
  readiness = { preparing: answer.preparing || 0, running: answer.running || 0, error: answer.error || null };
  const age = Date.now() - since;
  const done = (readiness.preparing === 0 && age >= AT_LEAST_MS) || age >= AT_MOST_MS;
  timer = done ? null : setTimeout(() => ask(client), EVERY_MS);
  if (done || JSON.stringify(readiness) !== was) tell();
}

function tell() {
  for (const fn of listeners) {
    try {
      fn(readiness);
    } catch (e) {
      console.error("a cutout listener threw", e);
    }
  }
}

/** For the tests: forget everything. */
export function resetCutoutReadiness() {
  clearTimeout(timer);
  timer = null;
  readiness = null;
  since = 0;
}
