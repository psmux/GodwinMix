// What is left after a preset, as a checklist somebody finishes here.
//
// The rule this exists for: in no part of the software does a person have to
// go and edit the configuration file. The old dialog printed the preset's
// three sentences, and the church one read "put your stream keys into the two
// [[outputs]] blocks of godwinmix.toml", which is a GUI telling a volunteer
// that the answer is in a text editor.
//
// So every row here is a control. A destination whose address still carries
// the placeholder a preset wrote gets a box and a Save, and the row turns
// green when the core says `has_key` has flipped. A plugin that is missing
// gets the Install button. Something that genuinely cannot happen until the
// next start says so in a sentence, and offers to restart only when the core
// publishes a method for it: a GUI never prints a command to type.
//
// The rows come from two places on purpose. The preset says what it wants
// done, typed, in `next`; the core says what is actually true right now, in
// `output.list` and in the plan's plugin list. Where they disagree the core
// wins, because a preset cannot know that this operator already had a key in.

import { el, on } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { modal } from "../../shell/modal.js";
import { openPicker } from "../../shell/picker.js";
import { pluginSourceFor, listPlugins, hasPlugin, platformOfHost, joinKey } from "../../client/kinds.js";

/**
 * The dialog after a preset was applied.
 *
 * @param {object} client
 * @param {{title: string}} choice the tile that was picked
 * @param {object} result what `preset.apply` answered
 */
export async function showChecklist(client, choice, result) {
  const plan = (result && result.plan) || {};
  const next = (result && result.next) || plan.next || [];
  const pending = (result && result.needs_restart) || [];
  const [outputs, canRestart] = await Promise.all([keyless(client), restartable(client)]);

  const body = el("div.col.wizard");
  const lead = el("p", { style: { marginTop: "0" } });
  const count = el("span.wizard-count");
  body.appendChild(el("div.row", {}, [lead, el("span.grow"), count]));
  const list = el("ol.wizard-list");
  body.appendChild(list);

  const rows = [];
  const track = (row) => {
    rows.push(row);
    list.appendChild(row.el);
  };
  const progress = () => {
    const todo = rows.filter((r) => r.counts);
    const done = todo.filter((r) => r.done()).length;
    count.textContent = todo.length ? `${done} of ${todo.length} done` : "";
    count.classList.toggle("all-done", todo.length > 0 && done === todo.length);
  };

  for (const output of outputs) track(keyRow(client, output, textFor(next, "stream_key", "output", output.id), progress));
  for (const plugin of missingPlugins(plan)) {
    track(installRow(client, plugin, textFor(next, "install_plugin", "name", plugin.name), progress));
  }
  for (const item of next.filter((n) => n.action === "add_source")) track(sourceRow(client, item));
  for (const item of next.filter((n) => n.action === "take" || n.action === "note")) {
    track(plainRow(item.text || "", item.action === "take" ? "Then" : ""));
  }
  for (const item of pending.filter((p) => p.reason === "restart")) {
    track(restartRow(client, item, canRestart));
  }
  for (const item of pending.filter((p) => p.reason === "refused")) {
    track(plainRow(item.message, "Would not start"));
  }

  lead.textContent = rows.some((r) => r.counts)
    ? `${choice.title} is set up. Finish it here.`
    : `${choice.title} is set up.`;
  progress();

  const m = modal({
    title: "Nearly there",
    body,
    footer: [el("button.btn.primary", { text: "Go to the mixer", onclick: () => m.close() })],
  });
  return m;
}

/** The sentence the preset wrote about one entry, matched on its own field. */
function textFor(next, action, field, value) {
  const hit = next.find((n) => n.action === action && n[field] === value);
  return (hit && hit.text) || "";
}

function missingPlugins(plan) {
  return (plan.plugins || []).filter((p) => !p.installed);
}

/**
 * The destinations still holding a placeholder.
 *
 * `has_key` is the core's own answer and the key never comes back out of it,
 * so this is the only way a page can know a stream key is still wanted
 * without ever being told what the key is.
 */
async function keyless(client) {
  try {
    const answer = await client.call("output.list", {});
    const list = Array.isArray(answer) ? answer : (answer && answer.outputs) || [];
    return list.filter((o) => o && o.has_key === false);
  } catch {
    return [];
  }
}

/** Whether this core publishes a way to restart itself. */
async function restartable(client) {
  try {
    const api = await client.call("core.api", {});
    return ((api && api.methods) || []).some((m) => m.name === "core.restart");
  } catch {
    return false;
  }
}

// ---------------------------------------------------------------- the rows

/** The shell every row shares: a dot, a title, a line, and whatever it needs. */
function shell(kind, title, note, controls) {
  const dot = el("span.dot." + kind);
  const body = el("div.grow", {}, [el("strong", { text: title })]);
  // Always in the tree, even when it starts empty: a row that succeeds
  // replaces this line with what happened, and a node that was never
  // appended says nothing to anybody.
  const line = el("div.sm.dim", { text: note || "" });
  body.appendChild(line);
  for (const c of controls || []) body.appendChild(c);
  return { el: el("li.wizard-row", {}, [dot, body]), dot, line };
}

/**
 * One destination that wants a stream key.
 *
 * The key is write only from here on: `output.set` takes the whole address and
 * no method hands one back, so the box is emptied the moment it is sent and
 * the row asks the core whether it worked rather than assuming it did.
 */
function keyRow(client, output, note, changed) {
  const platform = platformOfHost(output.uri_host);
  const whole = !platform || !platform.fixed;
  const input = el("input", {
    type: "password",
    autocomplete: "off",
    placeholder: whole ? "Paste the whole address" : "Paste the stream key",
  });
  const save = el("button.btn.primary", { text: "Save" });
  const form = el("div.row.wizard-form", {}, [input, save]);
  const title = (platform && platform.title) || output.id;
  const where = note || (platform && platform.where) || "";
  const view = shell("stalled", `${title}: needs a stream key`, where, [form]);
  let done = false;

  save.onclick = async () => {
    const typed = input.value.trim();
    if (!typed) return;
    save.disabled = true;
    input.disabled = true;
    const uri = whole ? typed : joinKey(platform.server, typed);
    try {
      await client.call("output.set", { id: output.id, uri });
    } catch (e) {
      errorToast(e, `Saving the key for ${output.id}`);
      save.disabled = false;
      input.disabled = false;
      return;
    }
    input.value = "";
    const list = await keyless(client);
    done = !list.some((o) => o.id === output.id);
    form.remove();
    view.dot.className = "dot " + (done ? "live" : "stalled");
    view.el.classList.toggle("done", done);
    view.el.querySelector("strong").textContent = done
      ? `${title}: key saved`
      : `${title}: still needs a stream key`;
    view.line.textContent = done
      ? "It reconnects on its own. The Destinations panel says when it is up."
      : "The mixer still reads that address as a placeholder. Try the whole address instead.";
    if (!done) view.el.appendChild(form);
    save.disabled = false;
    input.disabled = false;
    changed();
  };
  on(input, "keydown", (e) => {
    if (e.key === "Enter") save.click();
  });

  return { el: view.el, counts: true, done: () => done };
}

/**
 * One missing plugin, and the button that installs it.
 *
 * `plugin.add` with an explicit source is the same call the command line
 * makes, it works while the mixer runs, and the listing is read again
 * afterwards so the row says what happened rather than what was hoped for.
 */
function installRow(client, plugin, note, changed) {
  const button = el("button.btn.primary", { text: `Install ${plugin.name} support` });
  const view = shell("stalled", `The ${plugin.name} plugin is not installed`, note, [
    el("div.row.wizard-form", {}, [button]),
  ]);
  let done = false;

  button.onclick = async () => {
    button.disabled = true;
    view.line.textContent = "Installing. This can take a minute.";
    try {
      const source = await pluginSourceFor(client, plugin.name);
      await client.call("plugin.add", { source });
    } catch (e) {
      errorToast(e, `Installing ${plugin.name}`);
      button.disabled = false;
      view.line.textContent = note;
      return;
    }
    const plugins = await listPlugins(client);
    done = hasPlugin(plugins, plugin.name);
    button.remove();
    view.dot.className = "dot " + (done ? "live" : "stalled");
    view.el.classList.toggle("done", done);
    view.el.querySelector("strong").textContent = done
      ? `The ${plugin.name} plugin is installed`
      : `The ${plugin.name} plugin was installed`;
    view.line.textContent = done
      ? "Nothing restarted, and anything waiting for it can start now."
      : "The mixer has not picked it up yet.";
    changed();
  };

  return { el: view.el, counts: true, done: () => done };
}

/** A source the preset wants and cannot add for you, because only you know it. */
function sourceRow(client, item) {
  const kind = item.kind || "";
  const button = el("button.btn", {
    text: "Add a source",
    onclick: () => openPicker(client, "source", { kind }),
  });
  const view = shell("idle", "Add your own source", item.text || "", [
    el("div.row.wizard-form", {}, [button]),
  ]);
  return { el: view.el, counts: false, done: () => true };
}

/** Something to read. No control, because there is nothing here to press. */
function plainRow(text, title) {
  const view = title
    ? shell("idle", title, text, [])
    : { el: el("li.wizard-row.plain", {}, [el("span.dot.idle"), el("div.grow.sm", { text })]) };
  return { el: view.el, counts: false, done: () => true };
}

/**
 * Something the core wrote down and did not bring up.
 *
 * The button appears only when `core.api` lists a restart method. A core that
 * has none gets a sentence and nothing else: printing a command for somebody
 * to type is the thing this whole dialog exists to stop.
 */
function restartRow(client, item, canRestart) {
  const controls = [];
  const button = canRestart
    ? el("button.btn", {
        text: "Restart now",
        onclick: async () => {
          button.disabled = true;
          try {
            await client.call("core.restart", {});
          } catch (e) {
            errorToast(e, "Restarting the mixer");
            button.disabled = false;
          }
        },
      })
    : null;
  if (button) controls.push(el("div.row.wizard-form", {}, [button]));
  const view = shell("idle", "Starts next time the mixer does", sentence(item), controls);
  return { el: view.el, counts: false, done: () => true };
}

/**
 * The same fact as `item.message`, without the command line in it.
 *
 * The core's prose is written for a terminal, where `gmx` is the thing the
 * reader just typed. Here it is not, so the id is named and the rest dropped.
 */
function sentence(item) {
  if (item.id && !item.id.includes("/") && !item.id.includes(".")) {
    return `${item.id} is set up and will start the next time this mixer does.`;
  }
  return "The settings the preset wrote take effect the next time this mixer starts.";
}
