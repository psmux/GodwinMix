// The reading half of Mixer Settings, with no DOM in it so the tests can run
// it against a stub schema: the layout the schema kit draws, the values it
// starts from, which keys a Save sends, which field a refusal belongs to, and
// what each key's answer means to a person.

/** Section to heading, in the order the dialog shows them. */
export const GROUPS = [
  ["canvas", "Picture"],
  ["program", "Programme stream"],
  ["hardware", "Hardware"],
  ["multiview", "Multiview"],
  ["snapshot", "Snapshots"],
  ["media", "Files"],
  ["control", "This mixer"],
  ["security", "Security"],
  ["safety", "Safety"],
  ["browser", "Web pages"],
  ["stall", "When a source stalls"],
  ["nodes", "Other machines"],
  ["plugins", "Plugins"],
];

/** The group a key belongs to: `x-gmx-group`, or the part before the dot. */
export function groupOf(key, prop) {
  return String((prop && prop["x-gmx-group"]) || key.split(".")[0]);
}

/** A UI schema for `ui/kits/schema`: one group per section, known ones first. */
export function layoutFromSchema(schema) {
  const props = (schema && schema.properties) || {};
  const bySection = new Map();
  for (const key of Object.keys(props)) {
    const g = groupOf(key, props[key]);
    if (!bySection.has(g)) bySection.set(g, []);
    bySection.get(g).push(key);
  }
  const known = GROUPS.map(([g]) => g);
  const order = known.filter((g) => bySection.has(g)).concat([...bySection.keys()].filter((g) => !known.includes(g)));
  const title = (g) => (GROUPS.find(([id]) => id === g) || [g, g[0].toUpperCase() + g.slice(1)])[1];
  return {
    type: "vertical",
    elements: order.map((g) => ({
      type: "group",
      label: title(g),
      elements: bySection.get(g).map((key) => ({ type: "control", scope: key })),
    })),
  };
}

/** `config.get` keys as the value object the form starts from. Secrets start empty. */
export function valuesFrom(got) {
  const out = {};
  for (const k of (got && got.keys) || []) if (!k.secret && k.value !== null) out[k.key] = k.value;
  return out;
}

/** Only what moved since the dialog opened. A cleared box is not a change; Reset is. */
export function changedKeys(before, read) {
  const out = {};
  for (const [key, value] of Object.entries(read || {})) {
    if (JSON.stringify(value) !== JSON.stringify(before[key])) out[key] = value;
  }
  return out;
}

/** The field a refusal is about, when it names one the form has. */
export function refusalField(error, keys) {
  const data = (error && error.data) || {};
  const named = [data.key].concat(Array.isArray(data.keys) ? data.keys : []);
  return named.find((k) => typeof k === "string" && keys.includes(k)) || null;
}

export const APPLIES = {
  live: "In force now",
  next_source: "Used by the next source added",
  restart: "Waits for the mixer to restart",
};

/** Key to status line, from a `config.set` or `config.reset` answer. */
export function statusFrom(answer) {
  const out = {};
  for (const key of answer.applied || []) out[key] = APPLIES.live;
  for (const key of answer.next_source || []) out[key] = APPLIES.next_source;
  for (const key of answer.needs_restart || []) out[key] = APPLIES.restart;
  for (const c of answer.changed || []) if (c.note) out[c.key] = `${out[c.key] || ""} ${c.note}`.trim();
  return out;
}

/** Key to status line for what `config.get` says is still waiting. */
export function pendingFrom(got) {
  const out = {};
  for (const key of (got && got.needs_restart) || []) out[key] = APPLIES.restart;
  for (const k of (got && got.keys) || []) if (k.overridden_by) out[k.key] = `Overruled by ${k.overridden_by} while it runs.`;
  return out;
}

/** A new token: 32 random bytes as hex, from the browser's own generator. */
export function newToken() {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
}
