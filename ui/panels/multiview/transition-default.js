// The transition a take uses when it names none is the core's: `fx.assign`
// keeps one for every take and one per scene, in fx-assign.json. Until the
// person picks a transition on this page, the control beside Take shows that
// one and Take names it, so what the control says is what goes on air. No
// elements here, so the tests reach it without a page.

/** `DEFAULT_TRANSITION_MS` in the protocol: a named transition with no length. */
export const DEFAULT_MS = 300;

/** What a fresh page has always taken with when the core assigns nothing. */
const FRESH = { type: "fade", ms: 500 };

/** The name `fx.assign` set for a take of `scene`: the scene's, then the default. */
export function assignedFor(assigned, scene) {
  const a = assigned || {};
  return (scene && a.scenes && a.scenes[scene]) || a.default || null;
}

/**
 * How long a take that names `name` alone runs, as the core plans it: an fx
 * item and a collection's transition run for their own length, anything else
 * for the protocol's default.
 */
export function naturalMs(state, name, defaultMs = DEFAULT_MS) {
  if (name === "cut") return 0;
  const k = state.extra.get(name);
  return (k && (k.ownMs || k.ms)) || defaultMs;
}

/**
 * Show the core's choice on a state nobody picked on, and mark it
 * `fromCore` when there is one. Nothing is written to this browser, so a
 * later change to the assignment is followed too. Answers whether anything
 * moved.
 */
export function follow(state, assigned, scene, defaultMs = DEFAULT_MS) {
  if (state.picked) return false;
  const name = assignedFor(assigned, scene);
  state.fromCore = Boolean(name);
  const type = name || FRESH.type;
  const ms = name ? naturalMs(state, name, defaultMs) : FRESH.ms;
  if (type === state.type && ms === state.length) return false;
  state.type = type;
  const pairs = state.options();
  state.option = pairs.length ? pairs[0][0] : "";
  state.easing = "ease-in-out";
  state.length = ms;
  return true;
}
