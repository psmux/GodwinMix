// Data schema to a form description, for the browser.
//
// The same reader as `describeForm` in `@godwinmix/client` and
// `godwinmix.schema.describe_form` in the Python library, and the fixtures in
// `ui/kits/fixtures.json` are replayed by all three so they cannot drift. A
// description is data, not widgets: the renderer in render.js turns it into
// DOM, the Tkinter kit turns the same thing into ttk widgets, and a plugin
// author writes neither.
//
// Covered: objects, scalars, enums, arrays of scalars, `if`/`then` visibility,
// `format: "secret"`, `x-gmx-unit` and `x-gmx-group`. Not covered, on purpose:
// `$ref` beyond `#/$defs/`, `oneOf` discrimination, tuple arrays. A plugin
// needing those ships its own editor, which is the last link of the fallback
// chain and the reason it exists.

/** @typedef {"text"|"secret"|"url"|"number"|"integer"|"boolean"|"choice"|"lines"|"json"} ControlKind */

export function describeForm(schema, value = {}) {
  const root = schema || {};
  const props = root.properties || {};
  const required = new Set(root.required || []);
  const fields = [];
  const groups = [];

  for (const [name, raw] of Object.entries(props)) {
    const sub = resolve(root, raw);
    const group = String(sub["x-gmx-group"] || "");
    if (!groups.includes(group)) groups.push(group);
    fields.push({
      name,
      label: String(sub.title || name),
      kind: kindOf(sub),
      description: sub.description ? String(sub.description) : undefined,
      unit: sub["x-gmx-unit"] ? String(sub["x-gmx-unit"]) : undefined,
      group,
      required: required.has(name),
      value: value[name] !== undefined ? value[name] : sub.default,
      choices: choicesOf(sub),
      min: numberOr(sub.minimum),
      max: numberOr(sub.maximum),
      step: sub.type === "integer" ? 1 : numberOr(sub.multipleOf),
      placeholder: Array.isArray(sub.examples) && sub.examples.length ? String(sub.examples[0]) : undefined,
      itemKind: sub.type === "array" ? kindOf(resolve(root, sub.items || {})) : undefined,
      showWhen: undefined,
      visible: true,
    });
  }

  const form = {
    title: root.title ? String(root.title) : undefined,
    description: root.description ? String(root.description) : undefined,
    fields,
    groups: groups.slice().sort((a, b) => (a === "" ? -1 : b === "" ? 1 : 0)),
  };
  attachConditions(root, form);
  return applyConditions(form, valuesOf(form));
}

/** Every field's value, hidden ones included. What `if` is tested against. */
export function valuesOf(form) {
  const out = {};
  for (const f of form.fields) if (f.value !== undefined) out[f.name] = f.value;
  return out;
}

/** Which fields apply, given what is filled in now. */
export function applyConditions(form, values) {
  for (const field of form.fields) {
    if (!field.showWhen || !field.showWhen.length) continue;
    field.visible = field.showWhen.every((c) => holds(c, values));
  }
  return form;
}

/**
 * The object to send. Hidden fields out, empty strings out rather than sent as
 * "", and a secret nobody retyped left alone rather than blanked.
 */
export function readForm(form, values, touchedSecrets = new Set()) {
  const out = {};
  for (const field of form.fields) {
    if (!field.visible) continue;
    if (field.kind === "secret" && !touchedSecrets.has(field.name)) continue;
    const value = coerce(values[field.name], field);
    if (value === undefined) continue;
    out[field.name] = value;
  }
  return out;
}

/** Required fields that are visible and still empty. */
export function missing(form, values) {
  const got = readForm(form, values, new Set(form.fields.map((f) => f.name)));
  return form.fields
    .filter((f) => f.required && f.visible)
    .filter((f) => got[f.name] === undefined || got[f.name] === "")
    .map((f) => f.name);
}

// ---------------------------------------------------------------- internals

function resolve(root, node) {
  if (node && typeof node.$ref === "string" && node.$ref.startsWith("#/$defs/")) {
    const key = node.$ref.slice("#/$defs/".length);
    const copy = Object.assign({}, node);
    delete copy.$ref;
    return Object.assign({}, (root.$defs || {})[key] || {}, copy);
  }
  return node || {};
}

function typeOf(sub) {
  return Array.isArray(sub.type) ? sub.type.find((t) => t !== "null") || "string" : sub.type || "string";
}

export function kindOf(sub) {
  if (Array.isArray(sub.enum) || Array.isArray(sub.oneOf)) return "choice";
  const type = typeOf(sub);
  if (type === "boolean") return "boolean";
  if (type === "integer") return "integer";
  if (type === "number") return "number";
  if (type === "array") return "lines";
  if (type === "object") return "json";
  if (sub.format === "secret" || sub.format === "password") return "secret";
  if (sub.format === "uri" || sub.format === "url") return "url";
  return "text";
}

function choicesOf(sub) {
  if (Array.isArray(sub.enum)) return sub.enum.map((v) => ({ value: v, label: String(v) }));
  if (Array.isArray(sub.oneOf)) {
    const consts = sub.oneOf.filter((o) => o && o.const !== undefined);
    if (consts.length === sub.oneOf.length) {
      return consts.map((o) => ({ value: o.const, label: String(o.title || o.const) }));
    }
  }
  return undefined;
}

function numberOr(value) {
  return typeof value === "number" && isFinite(value) ? value : undefined;
}

function attachConditions(root, form) {
  const blocks = [];
  if (root.if) blocks.push(root);
  for (const block of root.allOf || []) if (block && block.if) blocks.push(block);

  for (const block of blocks) {
    const test = [];
    for (const [name, cond] of Object.entries((block.if && block.if.properties) || {})) {
      if (cond && cond.const !== undefined) test.push({ field: name, equals: cond.const });
      else if (cond && Array.isArray(cond.enum)) test.push({ field: name, oneOf: cond.enum });
    }
    for (const name of (block.if && block.if.required) || []) test.push({ field: name, present: true });

    for (const name of Object.keys((block.then && block.then.properties) || {})) {
      const field = form.fields.find((f) => f.name === name);
      if (field) field.showWhen = (field.showWhen || []).concat(test);
    }
    // One test negates cleanly; several would be "not all of them", which is
    // more than this reader promises, so a multi test `else` shows its fields.
    if (test.length === 1) {
      for (const name of Object.keys((block.else && block.else.properties) || {})) {
        const field = form.fields.find((f) => f.name === name);
        if (field) field.showWhen = (field.showWhen || []).concat([Object.assign({}, test[0], { negate: true })]);
      }
    }
  }
}

function holds(c, values) {
  const value = values[c.field];
  let met = true;
  if (c.present !== undefined) {
    const there = value !== undefined && value !== null && value !== "";
    met = c.present ? there : !there;
  } else if (c.oneOf !== undefined) {
    met = c.oneOf.includes(value);
  } else if ("equals" in c) {
    met = value === c.equals;
  }
  return c.negate ? !met : met;
}

function coerce(raw, field) {
  if (raw === undefined || raw === null) return undefined;
  switch (field.kind) {
    case "boolean":
      return Boolean(raw);
    case "integer": {
      if (raw === "") return undefined;
      const n = parseInt(String(raw), 10);
      return isFinite(n) ? n : undefined;
    }
    case "number": {
      if (raw === "") return undefined;
      const n = Number(raw);
      return isFinite(n) ? n : undefined;
    }
    case "lines": {
      const lines = Array.isArray(raw)
        ? raw.map(String)
        : String(raw)
            .split("\n")
            .map((s) => s.trim())
            .filter(Boolean);
      if (field.itemKind === "integer") return lines.map((s) => parseInt(s, 10));
      if (field.itemKind === "number") return lines.map(Number);
      return lines;
    }
    case "json": {
      if (typeof raw === "object") return raw;
      const text = String(raw).trim();
      if (!text) return undefined;
      try {
        return JSON.parse(text);
      } catch {
        return undefined;
      }
    }
    case "choice":
      return raw === "" ? undefined : coerceChoice(raw, field);
    default:
      return raw === "" ? undefined : raw;
  }
}

/** A select hands back a string even when the schema's values are numbers. */
function coerceChoice(raw, field) {
  const match = (field.choices || []).find((c) => String(c.value) === String(raw));
  return match ? match.value : raw;
}
