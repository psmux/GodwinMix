// Mixer Settings: the mixer's own configuration, as opposed to Settings, which
// is this page in this browser.
//
// The form is drawn from `config.schema` by the schema kit, grouped by
// section, so a core with more settings grows this dialog by itself. Save
// sends `config.set` with only the keys that moved, puts a refusal under the
// field it names, and says per key whether the change is in force, waits for
// the next source, or waits for a restart. Loaded with `import()` from the
// Settings dialog and the palette.

import { el } from "./dom.js";
import { modal } from "./modal.js";
import { CODES } from "../client/errors.js";
import { layoutFromSchema, valuesFrom, changedKeys, refusalField, statusFrom, pendingFrom, APPLIES } from "./mixer-form.js";
import { fieldWraps, decorate, putValue } from "./mixer-fields.js";
import { announceRestart } from "./mixer-config.js";

export async function openMixerSettings(client) {
  let schema, got;
  try {
    [schema, got] = await Promise.all([client.call("config.schema", {}), client.call("config.get", {})]);
  } catch (e) {
    return refusedDialog(e);
  }
  const { SchemaInspector } = await import("../kits/schema/index.js");
  let before = valuesFrom(got);
  const entries = new Map((got.keys || []).map((k) => [k.key, k]));
  const inspector = new SchemaInspector({ schema, ui: layoutFromSchema(schema), value: before });
  const banner = el("p.mixer-banner", { role: "status", hidden: true });
  const summary = el("p.sm", { role: "status" });
  const wraps = fieldWraps(inspector);
  const fields = new Map();
  for (const [key, wrap] of wraps) fields.set(key, decorate(client, key, wrap, entries.get(key), (k) => reset(k)));
  for (const [key, text] of Object.entries(pendingFrom(got))) fields.get(key)?.status(text);
  // Everything open is a very long page. The first section is open, and a
  // section with a key waiting for a restart, so the reason for the banner shows.
  const details = [...inspector.el.querySelectorAll("details")];
  details.forEach((d, i) => { d.open = i === 0 || !!d.querySelector(".field-status:not(:empty)"); });
  showBanner(got.needs_restart);

  function showBanner(keys) {
    const n = (keys || []).length;
    banner.hidden = !n;
    banner.textContent = n === 1
      ? "One change is saved and waits for the mixer to restart."
      : `${n} changes are saved and wait for the mixer to restart.`;
  }

  function clearErrors() {
    for (const f of fields.values()) f.error("");
  }

  function showAnswer(answer, keys) {
    const status = statusFrom(answer);
    for (const key of keys) {
      fields.get(key)?.status(status[key] || "");
      fields.get(key)?.written(!answer.dry_run);
    }
    if (keys.includes("control.token")) {
      fields.get("control.token")?.status(`${APPLIES.restart}. After that the mixer asks every page for the new token.`);
    }
    showBanner(answer.needs_restart);
    announceRestart(answer.needs_restart);
  }

  function refused(e) {
    const key = refusalField(e, [...wraps.keys()]);
    if (key) {
      fields.get(key).error(e.message);
      const d = wraps.get(key).closest("details");
      if (d) d.open = true;
      wraps.get(key).scrollIntoView({ block: "nearest" });
      summary.textContent = "Nothing was saved. The field marked below says why.";
    } else {
      summary.textContent = e.message || String(e);
    }
  }

  async function save() {
    clearErrors();
    const values = changedKeys(before, inspector.read());
    const keys = Object.keys(values);
    if (!keys.length) {
      summary.textContent = "Nothing has changed since this opened.";
      return;
    }
    saveButton.disabled = true;
    try {
      const answer = await client.call("config.set", { values });
      before = Object.assign({}, before, values);
      for (const key of keys) if (entries.get(key)?.secret) delete before[key];
      showAnswer(answer, keys);
      summary.textContent = keys.length === 1 ? "Saved one setting." : `Saved ${keys.length} settings.`;
    } catch (e) {
      refused(e);
    }
    saveButton.disabled = false;
  }

  async function reset(key) {
    clearErrors();
    try {
      const answer = await client.call("config.reset", { keys: [key] });
      const fallback = schema.properties[key] ? schema.properties[key].default : undefined;
      if (!entries.get(key)?.secret) putValue(wraps.get(key), fallback);
      before[key] = fallback;
      showAnswer(answer, [key]);
      fields.get(key).written(false);
      summary.textContent = `${key} is back to its default.`;
    } catch (e) {
      refused(e);
    }
  }

  const saveButton = el("button.btn.primary", { text: "Save", onclick: save });
  const m = modal({
    title: "Mixer settings",
    wide: true,
    body: el("div.col", {}, [
      el("p.dim", { style: { marginTop: "0" }, text: `These are the mixer's own settings, written to ${got.path}. They apply to every page and every operator. Each field says when a change takes effect.` }),
      banner,
      inspector.el,
    ]),
    footer: [summary, el("span.grow"), el("button.btn", { text: "Close", onclick: () => m.close() }), saveButton],
  });
  return m;
}

function refusedDialog(e) {
  const text = e && e.code === CODES.NO_SCOPE
    ? "This page's token can use the mixer but not change its settings, which needs admin scope. Ask whoever runs this mixer for an admin token, or ask them to make the change."
    : (e && e.message) || String(e);
  const m = modal({
    title: "Mixer settings",
    body: el("p", { text }),
    footer: [el("button.btn.primary", { text: "Close", onclick: () => m.close() })],
  });
  return m;
}
