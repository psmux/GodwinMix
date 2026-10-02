// Setting up what a preset needs, from the "Nearly there" dialog and the OBS
// import: one row per missing piece, and the call behind its button. A piece
// the mixer carries is set up with `setup.start`; anything else is installed
// with `plugin.add`, the call `gmx plugin add` makes.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { pluginSourceFor, listPlugins, hasPlugin } from "../../client/kinds.js";
import { firstSentence } from "../../shell/restart-bar.js";
import { setUp, notASetupPiece } from "../../client/setup.js";

/**
 * Set one plugin up, then read the listing again so the line says what
 * actually happened.
 *
 * @returns {Promise<boolean>} whether it is loaded now
 */
export async function installPlugin(client, name, button, note) {
  button.disabled = true;
  note.textContent = " Setting it up. This happens once and can take a minute.";
  let failed = "";
  try {
    await setUp(client, name, (said) => (note.textContent = " " + said));
  } catch (e) {
    failed = notASetupPiece(e) ? await addByName(client, name) : e.message || " ";
  }
  if (failed) {
    button.disabled = false;
    note.textContent = failed.trim() ? ` It did not install: ${firstSentence(failed)}` : "";
    return false;
  }
  const loaded = hasPlugin(await listPlugins(client), name);
  note.textContent = loaded ? " Ready, and nothing restarted." : " Set up, but the mixer has not picked it up yet. Restart it when the show allows.";
  button.remove();
  return loaded;
}

/** A plugin the mixer does not carry, installed from a marketplace. */
async function addByName(client, name) {
  try {
    const source = await pluginSourceFor(client, name);
    return await finished(client, await client.call("plugin.add", { source }));
  } catch (e) {
    errorToast(e, "Installing an add on this setup needs");
    return " ";
  }
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
  const button = el("button.btn.primary", { text: "Set it up" });
  button.onclick = () => installPlugin(client, plugin.name, button, note);
  return el("p.sm", {}, [
    el("span.dot.stalled"),
    " ",
    el("span", {
      text: "Something this needs is not set up on this mixer yet, so anything that uses it waits until it is. ",
    }),
    button,
    note,
  ]);
}
