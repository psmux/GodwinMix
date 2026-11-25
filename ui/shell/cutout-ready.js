// Whether the cutouts are ready yet. A cutout's model takes seconds to open
// and the person is not drawn until then, so this asks the core
// (`filter.cutout`) while a cutout is new here, and stops once none prepares.
// A mixer restart opens every model again, so a page that has watched asks
// again each time its socket opens.

const EVERY_MS = 600;
// A new cutout may not be in the pipeline yet; trust "none" only after this.
const AT_LEAST_MS = 2500;
// Never asked for longer than this.
const AT_MOST_MS = 60000;
// After a reconnect the restarted mixer may still be building its scenes.
const AFTER_RECONNECT_MS = 10000;

let readiness = null;
let timer = null;
let since = 0;
let atLeast = AT_LEAST_MS;
const followed = new WeakSet();
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
  follow(client);
  since = Date.now();
  atLeast = AT_LEAST_MS;
  if (timer) return;
  // Said at once, so the words appear with the click rather than an answer later.
  readiness = Object.assign({ running: 0, error: null }, readiness, { preparing: Math.max(1, (readiness && readiness.preparing) || 0) });
  timer = setTimeout(() => ask(client), 0);
  tell();
}

/** Ask again whenever the socket opens anew, since the models open again too. */
function follow(client) {
  if (followed.has(client) || !client.on) return;
  followed.add(client);
  client.on("open", () => {
    since = Date.now();
    atLeast = AFTER_RECONNECT_MS;
    if (!timer) timer = setTimeout(() => ask(client), 0);
  });
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
  const done = (readiness.preparing === 0 && age >= atLeast) || age >= AT_MOST_MS;
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
  atLeast = AT_LEAST_MS;
}
