// What comes after a welcome tile is picked: the preset's steps as a checklist
// with a button for each, a row for each plugin still missing, and the OBS
// import.
//
// Out of `panel.js`, which every page loads, because a mixer that has been
// set up never shows either. They arrive with the first pick.

import { el } from "../../shell/dom.js";
import { toast } from "../../shell/toast.js";
import { modal } from "../../shell/modal.js";
import { checklist, stepsOf, newContext } from "./checklist.js";
import { checkRestart } from "../../shell/restart-bar.js";
import { installPlugin, installRow } from "./install.js";
import { leaves } from "../../shell/dock-model.js";

const COUNT = ["No", "One", "Two", "Three", "Four", "Five", "Six"];

/** A heading for however many steps there are, in words. */
export function leftLine(title, count) {
  if (!count) return `${title} is set up.`;
  const n = COUNT[count] || String(count);
  return `${title} is set up. ${n} thing${count === 1 ? "" : "s"} left.`;
}

/** What to do next, from the preset's own manifest rather than from here. */
export function showSteps(client, choice, result) {
  const plan = (result && result.plan) || {};
  const steps = stepsOf(plan);
  const ctx = newContext(client.state, plan);
  const body = el("div.col");
  body.appendChild(el("p", { text: leftLine(choice.title, steps.length), style: { marginTop: "0" } }));

  let m = null;
  const hooks = {
    install: (name, button, note) =>
      installPlugin(client, name, button, note).then((ok) => ok && ctx.installed.add(name) && list.render(client.state)),
    showPanel: (id) => showPanel(m, id),
  };
  const list = steps.length
    ? checklist(client, steps, ctx, hooks)
    : { node: el("ol.welcome-steps", {}, [el("li", { text: "Add a source and press its tile." })]), render() {} };
  body.appendChild(list.node);

  // A plugin a step already installs has its button in that step.
  const covered = new Set(steps.filter((s) => s.does === "install-plugin").map((s) => s.target));
  for (const plugin of (plan.plugins || []).filter((p) => !p.installed && !covered.has(p.name))) {
    body.appendChild(installRow(client, plugin));
  }
  const notes = notesWorthShowing(plan, result);
  if (notes.length) body.appendChild(el("ul.welcome-notes.sm.dim", {}, notes.map((note) => el("li", { text: note }))));

  const off = client.onRender ? client.onRender((state) => list.render(state)) : () => {};
  m = modal({
    title: "Nearly there",
    body,
    footer: [el("button.btn.primary", { text: "Got it", onclick: () => m.close() })],
    onClose: off,
  });
  // The keys the preset wrote wait for a restart. The bar at the top of the
  // window says so and offers the restart, so this dialog does not.
  checkRestart(client);
  return m;
}

/**
 * The core's `needs_restart` notes, less the two this page already answers:
 * a source waiting on a plugin (the install row above) and the config keys
 * waiting on a restart (the bar). What is left is what did not start, which
 * nothing else on screen says.
 */
export function notesWorthShowing(plan, result) {
  const additions = (plan.sources || []).concat(plan.outputs || []);
  return ((result && result.needs_restart) || []).filter((note) => {
    const first = String(note).split(" ")[0];
    const addition = additions.find((a) => a.id === first);
    return addition && !addition.needs_plugin;
  });
}

/**
 * Bring a panel into view: make it the open tab of its group (or put it back
 * if it was closed), lift the checklist's scrim for a moment so it can be
 * seen, and offer the way back.
 *
 * @returns {boolean} whether there was a panel to show
 */
function showPanel(m, id) {
  const ws = document.querySelector("gmx-shell")?.workspace;
  if (ws) {
    const group = leaves(ws.state.tree).find((g) => g.tabs.includes(id));
    if (group) ws.activate(group, id);
    else ws.show(id);
  }
  const panel = document.querySelector(`[data-dock-panel="${CSS.escape(id)}"], [data-panel="${CSS.escape(id)}"]`);
  if (!panel) {
    toast({ text: "That panel is not on this page. Put it back from Panels and layout." });
    return false;
  }
  const scrim = m && m.el.parentElement;
  if (scrim) scrim.hidden = true;
  panel.scrollIntoView({ block: "nearest" });
  panel.classList.add("flash");
  setTimeout(() => panel.classList.remove("flash"), 2400);
  toast({ text: "Here it is.", action: { label: "Back to the list", run: () => scrim && (scrim.hidden = false) }, ms: 20000 });
  return true;
}

/** The OBS tile: a drop zone, in its own module because few ever press it. */
export async function importFromObs(client) {
  return (await import("./obs-import.js")).openObsImport(client, { installRow });
}
