// What comes after a welcome tile is picked: the preset's steps as a checklist
// with a button for each, a row for each plugin still missing, and the OBS
// import.
//
// Out of `panel.js`, which every page loads, because a mixer that has been
// set up never shows either. They arrive with the first pick.

import { el } from "../../shell/dom.js";
import { errorToast, toast } from "../../shell/toast.js";
import { modal } from "../../shell/modal.js";
import { pluginSourceFor, listPlugins, hasPlugin } from "../../client/kinds.js";
import { checklist, stepsOf, newContext } from "./checklist.js";
import { checkRestart } from "../../shell/restart-bar.js";

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
  for (const note of notesWorthShowing(plan, result)) body.appendChild(el("p.sm.dim", { text: note }));

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
 * Bring a panel into view: close the checklist's scrim for a moment so the
 * panel can be seen, open it if it is folded, and offer the way back.
 */
function showPanel(m, id) {
  const panel = document.querySelector(`[data-panel="${CSS.escape(id)}"]`);
  if (!panel) {
    toast({ text: "That panel is not on this page. Add it back from the workspace menu." });
    return;
  }
  const scrim = m && m.el.parentElement;
  if (scrim) scrim.hidden = true;
  const section = panel.closest("details");
  if (section) section.open = true;
  panel.scrollIntoView({ block: "center" });
  panel.classList.add("flash");
  setTimeout(() => panel.classList.remove("flash"), 2400);
  toast({ text: "Here it is.", action: { label: "Back to the list", run: () => scrim && (scrim.hidden = false) }, ms: 20000 });
}

/**
 * Install one plugin with `plugin.add`, the call `gmx plugin add` makes, then
 * read the listing again so the line says what actually happened.
 *
 * @returns {Promise<boolean>} whether it is loaded now
 */
async function installPlugin(client, name, button, note) {
  button.disabled = true;
  note.textContent = " Installing. This can take a minute.";
  try {
    const source = await pluginSourceFor(client, name);
    await client.call("plugin.add", { source });
  } catch (e) {
    errorToast(e, `Installing ${name}`);
    button.disabled = false;
    note.textContent = "";
    return false;
  }
  const loaded = hasPlugin(await listPlugins(client), name);
  note.textContent = loaded ? " Installed, and nothing restarted." : " Installed, but the mixer has not picked it up yet.";
  button.remove();
  return loaded;
}

/** One missing plugin, and the button that installs it. */
export function installRow(client, plugin) {
  const note = el("span.sm.dim");
  const button = el("button.btn.primary", { text: `Install ${plugin.name} support` });
  button.onclick = () => installPlugin(client, plugin.name, button, note);
  return el("p.sm", {}, [
    el("span.dot.stalled"),
    " ",
    el("span", {
      text: `The ${plugin.name} plugin is not installed yet, so anything that needs it stays listed and does not start. `,
    }),
    button,
    note,
  ]);
}

/** The OBS tile: a drop zone, in its own module because few ever press it. */
export async function importFromObs(client) {
  return (await import("./obs-import.js")).openObsImport(client, { installRow });
}
