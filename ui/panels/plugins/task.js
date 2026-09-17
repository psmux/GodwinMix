// Long running work, watched rather than waited on, and the way a failure is
// put on screen.
//
// `plugin.add`, `plugin.update` and `marketplace.add` answer with a handle and
// carry on in the background, because fetching a release over a venue's
// connection is not a five second job. So the panel polls `task.get` and says
// what the work is doing. A spinner with no words is the thing this exists to
// avoid: an install pulling forty megabytes looks exactly like one that hung.

import { el } from "../../shell/dom.js";
import { asRpcError, RpcError, CODES } from "../../client/errors.js";

/** What to show beside a button while a task runs. Pure, so it is tested. */
export function taskWords(task) {
  if (!task) return "";
  const pct = Number.isFinite(task.progress) ? ` ${Math.round(task.progress * 100)}%` : "";
  const age = Number.isFinite(task.age_secs) && task.age_secs >= 3 ? `, ${Math.round(task.age_secs)}s so far` : "";
  switch (task.state) {
    case "running":
      return `Working${pct}${age}. It is fetched, its signature and api level are checked, and only then is anything copied.`;
    case "completed":
      return "Done.";
    case "failed":
      return task.error || "It stopped without saying why. The mixer is untouched.";
    case "cancelled":
      return "Cancelled. Nothing was installed.";
    default:
      return task.state ? String(task.state) : "";
  }
}

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

/**
 * Call a method that may answer with a task handle, and follow it to the end.
 *
 * `say` is called with a sentence every time there is news. Answers with the
 * body the method would have returned, and throws an RpcError when the work
 * failed, so every caller has one thing to catch.
 */
export async function runTask(client, method, params, say) {
  let handle;
  try {
    handle = await client.call(method, params);
  } catch (e) {
    throw asRpcError(e);
  }
  // A core quick enough to answer outright, and every method that never
  // spawned a task in the first place.
  if (!handle || !handle.task_id) return handle;
  let wait = Number(handle.poll_interval_ms) || 1000;
  say(taskWords({ state: "running" }));
  for (;;) {
    await sleep(wait);
    let task;
    try {
      task = await client.call("task.get", { task_id: handle.task_id });
    } catch (e) {
      throw asRpcError(e);
    }
    say(taskWords(task));
    if (task.state === "completed") return task.result;
    if (task.state === "failed" || task.state === "cancelled") {
      throw new RpcError(CODES.INTERNAL, taskWords(task), { task_id: handle.task_id });
    }
    wait = Number(task.poll_interval_ms) || wait;
  }
}

/**
 * A failure, in the panel, next to the thing that failed.
 *
 * The core's message already names the state and the next step, so it is shown
 * as it came. What is never shown is a command to type: this panel is the
 * whole point of not sending anybody to a terminal.
 */
export function problemNode(e) {
  const err = asRpcError(e);
  const next = err.nextStep;
  const body = next && next !== err.message ? String(err.message).slice(0, -next.length).trim() : err.message;
  return el("div.plugin-problem", { role: "alert" }, [
    el("strong", { text: err.title }),
    el("span", { text: body }),
    next && next !== err.message ? el("span.sm", { text: next }) : null,
  ]);
}

/** Put one problem under a node, replacing whatever was there. */
export function showProblem(host, e) {
  host.textContent = "";
  host.appendChild(problemNode(e));
}
