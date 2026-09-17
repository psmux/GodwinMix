// The Installed tab: every plugin on this mixer, and the four things an
// operator does to one.
//
// On, off, settings, update, remove. All four were API methods with no button
// anywhere, so the only way to turn a plugin off was a terminal. The numbers a
// plugin costs stay in `plugin.list` and in the CLI, because this panel is for
// deciding what is on the machine rather than for watching it run.

import { el, clear } from "../../shell/dom.js";
import { confirmModal, modal } from "../../shell/modal.js";
import { runTask, showProblem } from "./task.js";

/** -1, 0 or 1. Dotted numbers only; a `-rc1` tail is not compared. */
export function compareVersions(a, b) {
  const nums = (v) => (String(v || "").split("-")[0].match(/\d+/g) || []).map(Number);
  const x = nums(a);
  const y = nums(b);
  for (let i = 0; i < Math.max(x.length, y.length); i += 1) {
    const d = (x[i] || 0) - (y[i] || 0);
    if (d) return d < 0 ? -1 : 1;
  }
  return 0;
}

/** The newer version a marketplace lists for this plugin, or "". */
export function updateFor(plugin, results) {
  const listed = (results || []).find((r) => r.name === plugin.name);
  if (!listed || !listed.version || !plugin.version) return "";
  return compareVersions(listed.version, plugin.version) > 0 ? listed.version : "";
}

/**
 * One form from a plugin's per provide schemas.
 *
 * Settings are per plugin and not per provide: a stream key declared on one
 * provide is the same key whichever of them reads it, and
 * `plugin.settings.set` takes one flat table. So the properties are merged and
 * the operator gets one form rather than three that write to the same place.
 */
export function mergeSchemas(schemas) {
  const out = { type: "object", properties: {}, required: [] };
  for (const schema of Object.values(schemas || {})) {
    if (!schema || typeof schema !== "object") continue;
    Object.assign(out.properties, schema.properties || {});
    for (const name of schema.required || []) {
      if (!out.required.includes(name)) out.required.push(name);
    }
    if (schema.$defs) out.$defs = Object.assign(out.$defs || {}, schema.$defs);
  }
  return out;
}

export function installedTab(client) {
  const node = el("div.col");
  const problems = el("div");
  const list = el("div.col");
  node.append(problems, list);
  let listed = [];
  const configurable = new Set();

  async function refresh() {
    problems.textContent = "";
    let answer;
    try {
      answer = await client.call("plugin.list", {});
    } catch (e) {
      showProblem(problems, e);
      return;
    }
    // What the marketplaces list, so a row can offer an update. A mixer with
    // no marketplace answers nothing here and no row offers one, which is
    // right: there is nowhere to update from.
    try {
      const found = await client.call("plugin.search", { term: "" });
      listed = (found && found.results) || [];
    } catch {
      listed = [];
    }
    const plugins = (answer && answer.plugins) || [];
    // Which of them have a settings schema to fill in. A local read each, and
    // the answer decides whether the row gets a Settings button at all: a
    // button that opens a form with no fields in it is a button that lies.
    configurable.clear();
    await Promise.all(plugins.map((p) => noteSchema(p)));
    draw(plugins);
  }

  async function noteSchema(plugin) {
    if (plugin.problem || String(plugin.root || "").startsWith("node:")) return;
    try {
      const got = await client.call("plugin.settings.get", { id: plugin.name });
      const schema = mergeSchemas(got && got.schemas);
      if (Object.keys(schema.properties).length) configurable.add(plugin.name);
    } catch {
      /* A plugin whose schema cannot be read gets no button, which is true. */
    }
  }

  function draw(plugins) {
    clear(list);
    if (!plugins.length) {
      list.appendChild(
        el("p.faint.sm", {
          text: "No plugins yet. Get more is the other tab, and it can add the GodwinMix marketplace for you.",
        })
      );
      return;
    }
    for (const plugin of plugins) list.appendChild(row(plugin));
  }

  function row(plugin) {
    const say = el("div.plugin-say.sm.dim");
    const buttons = el("div.row.sm");
    const remote = String(plugin.root || "").startsWith("node:");
    buttons.append(
      remote ? el("span.sm.faint", { text: "on " + plugin.root.slice(5) }) : toggle(plugin, say),
      configurable.has(plugin.name) ? settingsButton(plugin) : null,
      remote ? null : updateButton(plugin, say),
      remote ? null : removeButton(plugin, say)
    );
    return el("div.plugin-row", { "data-plugin": plugin.name }, [
      el("div.row", {}, [
        el("span.dot" + (plugin.problem ? ".failed" : plugin.enabled ? "" : ".stalled")),
        el("strong.ellipsis", { text: plugin.name }),
        el("span.num.faint", { text: plugin.version }),
        el("span.grow"),
        el("span.sm.faint", { text: plugin.trust || "" }),
      ]),
      plugin.description ? el("div.sm.dim", { text: plugin.description }) : null,
      el("div.sm.faint.ellipsis", {
        text: (plugin.provides || []).length ? "provides " + plugin.provides.join(", ") : "provides nothing",
      }),
      plugin.problem ? el("div.sm", { text: plugin.problem }) : null,
      buttons,
      say,
    ]);
  }

  function toggle(plugin, say) {
    const box = el("input", { type: "checkbox", checked: !!plugin.enabled, disabled: !!plugin.problem });
    box.onchange = async () => {
      const wanted = box.checked;
      box.disabled = true;
      say.textContent = wanted ? "Turning it on." : "Turning it off. Nothing on air is cut.";
      try {
        await client.call(wanted ? "plugin.enable" : "plugin.disable", { id: plugin.name });
      } catch (e) {
        box.checked = !wanted;
        box.disabled = false;
        say.textContent = "";
        showProblem(problems, e);
        return;
      }
      await refresh();
    };
    return el("label.row.sm", { title: "Off means it registers nothing and runs no process." }, [
      box,
      el("span", { text: "Enabled" }),
    ]);
  }

  function updateButton(plugin, say) {
    const to = updateFor(plugin, listed);
    if (!to) return null;
    const button = el("button.btn.sm", { text: `Update to ${to}` });
    button.onclick = async () => {
      button.disabled = true;
      try {
        await runTask(client, "plugin.update", { id: plugin.name }, (words) => {
          say.textContent = words;
        });
      } catch (e) {
        button.disabled = false;
        say.textContent = "";
        showProblem(problems, e);
        return;
      }
      await refresh();
    };
    return button;
  }

  function removeButton(plugin, say) {
    const button = el("button.btn.sm", { text: "Remove" });
    button.onclick = async () => {
      const sure = await confirmModal(
        `Remove ${plugin.name}? Everything it added goes with it: ${
          (plugin.provides || []).join(", ") || "nothing"
        }. Sources using one of those types will say so by name.`,
        "Remove"
      );
      if (!sure) return;
      button.disabled = true;
      say.textContent = "Removing.";
      try {
        await client.call("plugin.remove", { id: plugin.name });
      } catch (e) {
        button.disabled = false;
        say.textContent = "";
        showProblem(problems, e);
        return;
      }
      await refresh();
    };
    return button;
  }

  function settingsButton(plugin) {
    const button = el("button.btn.sm", { text: "Settings" });
    button.onclick = () => openSettings(client, plugin, problems).catch((e) => showProblem(problems, e));
    return button;
  }

  return { node, refresh, destroy() {} };
}

/**
 * A plugin's own settings, from its own schema.
 *
 * Nothing about any plugin is hardcoded here: the schema comes from
 * `plugin.settings.get` and the form is built from it, which is the same rule
 * the add picker follows. A plugin with no schema has no button.
 */
async function openSettings(client, plugin, problems) {
  let current;
  try {
    current = await client.call("plugin.settings.get", { id: plugin.name });
  } catch (e) {
    showProblem(problems, e);
    return;
  }
  const schema = mergeSchemas(current.schemas);
  const { SchemaForm } = await import("../../client/schema-form.js");
  const form = new SchemaForm(schema, current.settings || {});
  const note = el("div.sm.dim");
  const save = el("button.btn.primary", { text: "Save" });
  const m = modal({
    title: `${plugin.name} settings`,
    body: el("div.col", {}, [form.el, note]),
    footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), save],
  });
  save.onclick = async () => {
    if (!form.validate()) {
      note.textContent = "Fill in the fields marked before saving.";
      return;
    }
    save.disabled = true;
    note.textContent = "Saving.";
    try {
      await client.call("plugin.settings.set", { id: plugin.name, settings: form.read() });
    } catch (e) {
      save.disabled = false;
      showProblem(note, e);
      return;
    }
    m.close();
  };
  form.focusFirst();
}
