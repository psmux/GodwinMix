// Install, Update and Remove: the calls `gmx plugin add`, `gmx plugin update`
// and `gmx plugin remove` make, with what is happening and what went wrong
// said in plain words. An install answers with a task at once and carries on
// in the core, off the programme's threads; this waits for the task, reading
// its stage and progress as it goes, so the button can say what is happening
// and the card says what actually happened.

import { firstSentence } from "../../shell/restart-bar.js";

const DOING = {
  add: (p) => (p.bundled ? "Installing. It comes with the mixer, so this is quick." : "Downloading and installing. This can take a minute."),
  update: () => "Updating. The one you have keeps working until the new one starts.",
  remove: () => "Removing.",
};

const DONE = { add: "Installed and ready. Nothing restarted.", update: "Updated.", remove: "Removed." };

const FAILED = { add: "It did not install", update: "It did not update", remove: "It was not removed" };

const AGAIN = {
  add: "Press Try again. If it fails the same way, open the plugin's page and tell its author.",
  update: "The version you had is still installed and working. Press Update to try again.",
  remove: "Press Remove to try again, or stop what is using it first.",
};

/**
 * Do one of the three to one plugin.
 *
 * @param {(text: string) => void} say  where the progress line goes
 * @param {(task: object) => void} watch  every reading of the task, while it runs
 * @returns {Promise<{ok: boolean, text: string, next?: string}>}
 */
export async function perform(client, p, act, say = () => {}, watch = () => {}) {
  say(DOING[act] ? DOING[act](p) : "");
  try {
    if (act === "add") await finished(client, await client.call("plugin.add", { source: p.install || p.name }), watch);
    else if (act === "update") await finished(client, await client.call("plugin.update", { id: p.name }), watch);
    else if (act === "remove") await client.call("plugin.remove", { id: p.name });
    else throw new Error(`there is no ${act}`);
    return { ok: true, text: DONE[act] };
  } catch (e) {
    return { ok: false, text: plainly(act, e), next: nextStep(act, e) };
  }
}

/** "It did not install: <the first sentence of why>", with no code marks. */
export function plainly(act, e) {
  const why = firstSentence(message(e)).replace(/`/g, "");
  return `${FAILED[act] || "It did not work"}: ${why || "the mixer did not say why."}`;
}

/**
 * What to do about it: the sentence after the first when the core wrote one,
 * since core errors name the next step, else the usual one for the act.
 */
export function nextStep(act, e) {
  const text = message(e).replace(/`/g, "").trim();
  const rest = text.slice(firstSentence(text).length).trim();
  const said = rest.split(/(?<=\.)\s/)[0] || "";
  if (/\b(reach|network|offline|timed out|connect)/i.test(text) && !said) {
    return "Check the mixer can reach the internet, then press Try again.";
  }
  return said || AGAIN[act] || "";
}

function message(e) {
  return String((e && e.message) || e || "");
}

/** Wait for a task to end. Throws with the task's own error when it failed. */
async function finished(client, answer, watch) {
  const id = answer && answer.task_id;
  if (!id) return answer;
  // Often enough for a progress bar to move, rarely enough to cost nothing.
  const every = Math.max(250, Math.min(answer.poll_interval_ms || 400, 400));
  for (let waited = 0; waited < 300000; waited += every) {
    await new Promise((r) => setTimeout(r, every));
    const task = await client.call("task.get", { task_id: id });
    if (task.state === "failed" || task.state === "cancelled") throw new Error(task.error || `it was ${task.state}.`);
    if (task.state !== "running" && task.state !== "queued") return task;
    watch(task);
  }
  throw new Error("it is still going after five minutes. Look again in a while.");
}
