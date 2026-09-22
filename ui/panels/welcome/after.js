// What comes after a welcome tile is picked: the preset's three steps with a
// button for each plugin still missing, and the note on importing from OBS.
//
// Out of `panel.js`, which every page loads, because a mixer that has been
// set up never shows either. They arrive with the first pick.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { modal } from "../../shell/modal.js";
import { pluginSourceFor, listPlugins, hasPlugin } from "../../client/kinds.js";

/** What to do next, from the preset's own manifest rather than from here. */
export function showSteps(client, choice, result) {
  const plan = (result && result.plan) || {};
  const steps = plan.steps || [];
  const missing = (plan.plugins || []).filter((p) => !p.installed);
  const body = el("div.col");

  body.appendChild(
    el("p", { text: `${choice.title} is set up. Three things left.`, style: { marginTop: "0" } })
  );
  const list = el("ol.welcome-steps");
  for (const step of steps) list.appendChild(el("li", { text: step }));
  if (!steps.length) list.appendChild(el("li", { text: "Add a source and press its tile." }));
  body.appendChild(list);

  for (const plugin of missing) body.appendChild(installRow(client, plugin));
  for (const note of (result && result.needs_restart) || []) {
    body.appendChild(el("p.sm.dim", { text: note }));
  }

  const m = modal({
    title: "Nearly there",
    body,
    footer: [el("button.btn.primary", { text: "Got it", onclick: () => m.close() })],
  });
}

/**
 * One missing plugin, and the button that installs it.
 *
 * This used to print `gmx plugin add camera` and stop there, which asks
 * somebody who has just picked Church service to go and find a terminal.
 * `plugin.add` is the same call that command makes, it works while the mixer
 * runs, and the listing is read again afterwards so the line says what
 * actually happened rather than what was hoped for.
 */
function installRow(client, plugin) {
  const note = el("span.sm.dim");
  const button = el("button.btn.primary", { text: `Install ${plugin.name} support` });
  const line = el("p.sm", {}, [
    el("span.dot.stalled"),
    " ",
    el("span", {
      text:
        `The ${plugin.name} plugin is not installed yet, so anything that needs it ` +
        `stays listed and does not start. `,
    }),
    button,
    note,
  ]);
  button.onclick = async () => {
    button.disabled = true;
    note.textContent = " Installing. This can take a minute.";
    try {
      const source = await pluginSourceFor(client, plugin.name);
      await client.call("plugin.add", { source });
    } catch (e) {
      errorToast(e, `Installing ${plugin.name}`);
      button.disabled = false;
      note.textContent = "";
      return;
    }
    const plugins = await listPlugins(client);
    note.textContent = hasPlugin(plugins, plugin.name)
      ? " Installed, and nothing restarted."
      : " Installed, but the mixer has not picked it up yet.";
    button.remove();
  };
  return line;
}

/** The OBS importer is `gmx import obs`; the page says where to point it. */
export function importFromObs() {
  const body = el("div.col");
  body.appendChild(
    el("p", {
      text:
        "GodwinMix reads an OBS scene collection and keeps your scenes, their items and " +
        "their positions. The importer runs on the machine that has OBS on it.",
      style: { marginTop: "0" },
    })
  );
  body.appendChild(el("p.sm.dim", { text: "In a terminal, with the path to the collection:" }));
  body.appendChild(
    el("pre.welcome-code", {
      text: "gmx import obs ~/.config/obs-studio/basic/scenes/Untitled.json \\\n  --out godwinmix.scenes.json",
    })
  );
  body.appendChild(
    el("p.sm.dim", {
      text:
        "On Windows the collections are in %APPDATA%\\obs-studio\\basic\\scenes, and on " +
        "macOS in ~/Library/Application Support/obs-studio/basic/scenes. " +
        "docs/how-to/import-from-obs.md has the rest, including what does not come across.",
    })
  );
  const m = modal({
    title: "Import from OBS",
    body,
    footer: [el("button.btn.primary", { text: "Close", onclick: () => m.close() })],
  });
}
