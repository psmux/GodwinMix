// Install, Update and Remove: the calls `gmx plugin add`, `gmx plugin update`
// and `gmx plugin remove` make, with what is happening and what went wrong
// said in plain words. An install answers with a task at once and carries on;
// this waits for the task, so the card says what actually happened.

import { firstSentence } from "../../shell/restart-bar.js";

const DOING = {
  add: (p) => (p.bundled ? "Installing. It comes with the mixer, so this is quick." : "Downloading and installing. This can take a minute."),
  update: () => "Updating. The one you have keeps working until the new one starts.",
  remove: () => "Removing.",
};

const DONE = { add: "Installed. Nothing restarted.", update: "Updated.", remove: "Removed." };

const FAILED = { add: "It did not install", update: "It did not update", remove: "It was not removed" };

/**
 * Do one of the three to one plugin.
 *
 * @param {(text: string) => void} say  where the progress line goes
 * @returns {Promise<{ok: boolean, text: string}>}
 */
export async function perform(client, p, act, say = () => {}) {
  say(DOING[act](p));
  try {
    if (act === "add") await finished(client, await client.call("plugin.add", { source: p.install || p.name }));
    else if (act === "update") await finished(client, await client.call("plugin.update", { id: p.name }));
    else if (act === "remove") await client.call("plugin.remove", { id: p.name });
    else throw new Error(`there is no ${act}`);
    return { ok: true, text: DONE[act] };
  } catch (e) {
    return { ok: false, text: plainly(act, e) };
  }
}

/** "It did not install: <the first sentence of why>", with no code marks. */
export function plainly(act, e) {
  const why = firstSentence((e && e.message) || String(e || "")).replace(/`/g, "");
  return `${FAILED[act] || "It did not work"}: ${why || "the mixer did not say why."}`;
}

/** Wait for a task to end. Throws with the task's own error when it failed. */
async function finished(client, answer) {
  const id = answer && answer.task_id;
  if (!id) return answer;
  const every = Math.max(250, Math.min(answer.poll_interval_ms || 1000, 2000));
  for (let waited = 0; waited < 300000; waited += every) {
    await new Promise((r) => setTimeout(r, every));
    const task = await client.call("task.get", { task_id: id });
    if (task.state === "failed" || task.state === "cancelled") throw new Error(task.error || `it was ${task.state}.`);
    if (task.state !== "running" && task.state !== "queued") return task;
  }
  throw new Error("it is still going after five minutes. Look again in a while.");
}
