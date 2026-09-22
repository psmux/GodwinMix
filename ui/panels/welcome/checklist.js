// The "Nearly there" checklist: one row per preset step, with a button that
// does the step and a tick once the mixer's own state says it is done.
//
// A step is `{text, does, target}` (crates/godwinmix-core/src/preset/step.rs).
// One with no `does`, or a `does` this page does not know, is drawn as its
// sentence. No new method: Add key is the Outputs panel's own form, add source
// the picker, install `plugin.add`, take `program.take`.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { openPicker } from "../../shell/picker-loader.js";

/** Label, and what makes it done, for each thing a step can do. */
const ACTIONS = {
  "add-key": { label: "Add key", done: (s, st, ctx) => keyed(s.target, st, ctx) },
  "add-source": { label: "Add", done: (s, st, ctx) => addedSince(st, ctx) },
  "install-plugin": { label: "Install", done: (s, st, ctx) => ctx.installed.has(s.target) },
  "open-panel": { label: "Show me", done: (s, st, ctx) => ctx.looked.has(s.target) },
  take: { label: "Put on air", done: (s, st) => onAir(s.target, st) },
};

/** The steps a plan carries, as `{text, does?, target?}`, whichever shape it sent. */
export function stepsOf(plan) {
  if (Array.isArray(plan.checklist) && plan.checklist.length) return plan.checklist;
  return (plan.steps || []).map((text) => (typeof text === "string" ? { text } : text));
}

/** Whether this page can do the step, rather than only print it. */
export function actionable(step) {
  return !!(step && step.does && step.target && ACTIONS[step.does]);
}

/** The state when the list opened, so "added" means since then, and what was done from it. */
export function newContext(state, plan) {
  const outputs = (state && state.outputs) || [];
  return {
    sourcesAtOpen: new Set(((state && state.sources) || []).map((s) => s.id)),
    keyedAtOpen: new Set(outputs.filter((o) => o.has_key !== false).map((o) => o.id)),
    saved: new Set(),
    looked: new Set(),
    installed: new Set(((plan && plan.plugins) || []).filter((p) => p.installed).map((p) => p.name)),
  };
}

/** Done, read from the state. A prose step has no test. */
export function stepDone(step, state, ctx) {
  if (!actionable(step)) return false;
  return !!ACTIONS[step.does].done(step, state || {}, ctx);
}

/**
 * An output counts as keyed once it has a key it did not have when the list
 * opened, or once the person saved its form here. An output that opened with
 * a key (a local test server, an SRT address with nothing to replace) is not
 * ticked on sight, because the step is asking for their own destination.
 */
function keyed(id, state, ctx) {
  const out = (state.outputs || []).find((o) => o.id === id);
  if (!out || out.has_key === false) return false;
  return !ctx.keyedAtOpen.has(id) || ctx.saved.has(id);
}

function addedSince(state, ctx) {
  return (state.sources || []).some((s) => !ctx.sourcesAtOpen.has(s.id));
}

function onAir(target, state) {
  return state.program === target || state.scene === target || state.sceneName === target;
}

/**
 * The list. `hooks` carries what the rows need from the dialog around them:
 * `install(name, button, note)` and `showPanel(id)`.
 *
 * @returns {{node: HTMLElement, render: (state) => void}}
 */
export function checklist(client, steps, ctx, hooks) {
  const list = el("ol.welcome-steps.checklist");
  const rows = steps.map((step) => stepRow(client, step, ctx, hooks, () => render(client.state)));
  for (const row of rows) list.appendChild(row.node);
  function render(state) {
    for (const row of rows) row.sync(state);
  }
  render(client.state);
  return { node: list, render };
}

function stepRow(client, step, ctx, hooks, rerender) {
  if (!actionable(step)) {
    return { node: el("li", { text: step.text }), sync: () => {} };
  }
  const tick = el("span.step-tick", { "aria-hidden": "true" });
  const note = el("span.sm.dim");
  const button = el("button.btn.primary.sm", { type: "button", text: ACTIONS[step.does].label });
  const node = el("li.step", { "data-does": step.does, "data-target": step.target }, [
    tick,
    el("span.grow", { text: step.text }),
    note,
    button,
  ]);
  button.onclick = () => act(client, step, ctx, hooks, { button, note, rerender });
  const sync = (state) => {
    const done = stepDone(step, state, ctx);
    node.classList.toggle("done", done);
    tick.textContent = done ? "✓" : "";
    button.hidden = done && step.does !== "add-key";
    if (step.does === "add-key") button.textContent = done ? "Change" : ACTIONS["add-key"].label;
  };
  return { node, sync };
}

/** Press a row's button. */
async function act(client, step, ctx, hooks, ui) {
  const target = step.target;
  try {
    if (step.does === "add-key") return await addKey(client, target, ctx, ui);
    if (step.does === "add-source") {
      return await openPicker(client, "source", { category: target, onAdded: ui.rerender });
    }
    if (step.does === "install-plugin") return await hooks.install(target, ui.button, ui.note);
    if (step.does === "open-panel") {
      if (hooks.showPanel(target)) ctx.looked.add(target);
      return ui.rerender();
    }
    if (step.does === "take") return await take(client, target, ui);
  } catch (e) {
    errorToast(e, step.text);
  }
}

async function addKey(client, id, ctx, ui) {
  const output = (client.state.outputs || []).find((o) => o.id === id);
  if (!output) {
    ui.note.textContent = ` ${id} is not on this mixer. Add it from Outputs.`;
    return;
  }
  const { editDestination } = await import("../outputs/destination.js");
  editDestination(client, output, {
    onDone: () => {
      ctx.saved.add(id);
      ui.rerender();
    },
  });
}

async function take(client, target, ui) {
  const isSource = (client.state.sources || []).some((s) => s.id === target);
  await client.call("program.take", isSource ? { source: target } : { scene: target });
  ui.rerender();
}
