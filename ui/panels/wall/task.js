// A switch of compositing answers at once with a task, because moving the
// outputs can take half a minute and no call may take more than five
// seconds. This waits on it the way the welcome panel waits on plugin.add.

const LONGEST_MS = 120000;

/**
 * The answer `show.set` would have given, once its task has one. An answer
 * with no task is already that answer.
 *
 * @returns {Promise<object>} the task's result
 */
export async function followTask(client, answer) {
  const id = answer && answer.task_id;
  if (!id) return answer;
  const every = Math.min(2000, Math.max(250, answer.poll_interval_ms || 1000));
  for (let waited = 0; waited < LONGEST_MS; waited += every) {
    await new Promise((r) => setTimeout(r, every));
    const task = await client.call("task.get", { task_id: id });
    if (task.state === "completed") return task.result || {};
    if (task.state === "failed" || task.state === "cancelled") {
      throw Object.assign(new Error(task.error || `the switch was ${task.state}`), { data: { task_id: id } });
    }
  }
  throw Object.assign(new Error("the switch is still going after two minutes. The wall shows the show as it lands."), { data: { task_id: id } });
}

/**
 * show.set for the wall, with the row changed at once and put back if
 * refused. A switch of compositing answers with a task: the row says it is
 * switching until the task lands, and `started` hears the handle as soon as
 * it comes. The row is looked up again at every step, because a
 * show.changed event replaces it while the switch runs.
 */
export async function setShow(data, id, patch, started) {
  const row = (change) => {
    const show = data.find(id);
    if (show) change(show);
    data.changed();
  };
  const before = data.find(id) && { ...data.find(id) };
  row((s) => Object.assign(s, patch));
  try {
    let got = await data.client.call("show.set", { id, ...patch });
    if (got && got.task_id) {
      row((s) => { s.switching = true; });
      if (started) started(got);
      got = await followTask(data.client, got).finally(() => row((s) => { delete s.switching; }));
    }
    if (got && got.id === id) row((s) => Object.assign(s, got));
    return got;
  } catch (e) {
    if (before) row((s) => Object.assign(s, before));
    throw e;
  }
}
