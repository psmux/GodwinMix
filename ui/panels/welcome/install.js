// Installing a plugin a preset needs, from the "Nearly there" dialog and the
// OBS import: one row per plugin, and the call behind its button.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { pluginSourceFor, listPlugins, hasPlugin } from "../../client/kinds.js";
import { firstSentence } from "../../shell/restart-bar.js";

/**
 * Install one plugin with `plugin.add`, the call `gmx plugin add` makes, then
 * read the listing again so the line says what actually happened.
 *
 * @returns {Promise<boolean>} whether it is loaded now
 */
export async function installPlugin(client, name, button, note) {
  button.disabled = true;
  note.textContent = " Installing. This can take a minute.";
  let failed = "";
  try {
    const source = await pluginSourceFor(client, name);
    failed = await finished(client, await client.call("plugin.add", { source }));
  } catch (e) {
    errorToast(e, `Installing ${name}`);
    failed = " ";
  }
  if (failed) {
    button.disabled = false;
    note.textContent = failed.trim() ? ` It did not install: ${firstSentence(failed)}` : "";
    return false;
  }
  const loaded = hasPlugin(await listPlugins(client), name);
  note.textContent = loaded ? " Installed, and nothing restarted." : " Installed, but the mixer has not picked it up yet.";
  button.remove();
  return loaded;
}

/**
 * `plugin.add` answers at once with a task when the install takes a while,
 * which it nearly always does. Wait for it, so the row says what happened
 * rather than reading the plugin list before the download has started.
 *
 * @returns {Promise<string>} the task's error, or "" when it finished
 */
async function finished(client, answer) {
  const id = answer && answer.task_id;
  if (!id) return "";
  const every = Math.max(500, answer.poll_interval_ms || 1000);
  for (let waited = 0; waited < 180000; waited += every) {
    await new Promise((r) => setTimeout(r, every));
    const task = await client.call("task.get", { task_id: id });
    if (task.state === "failed" || task.state === "cancelled") return task.error || task.state;
    if (task.state !== "running" && task.state !== "queued") return "";
  }
  return "it is still going after three minutes";
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
