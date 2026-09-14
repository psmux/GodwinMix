// JSON Schema draft 2020-12 to a form description.
//
// Not a UI. A plain data structure a UI renders however it likes: web
// components, a table of <input>s, a terminal form, a Godot VBox. The same
// shape comes out of `godwinmix.schema.describe_form` in the Python library, so
// a plugin's settings look the same in every surface.
//
// A UI never hardcodes a plugin's settings. It asks `plugin.describe` for the
// schema and renders what comes back, which is what makes a plugin usable from
// a UI its author has never seen.
//
// Covered: objects, scalars, enums, arrays of scalars, `if`/`then` visibility,
// `format: "secret"`, `x-gmx-unit` (a suffix beside the control) and
// `x-gmx-group` (a section, so common fields sit above advanced ones without a
// second schema). Not covered, on purpose: $ref beyond `#/$defs/...`, oneOf
// discrimination, tuple arrays. A plugin needing those ships its own editor.

export type ControlKind =
  | "text"
  | "secret"
  | "url"
  | "number"
  | "integer"
  | "boolean"
  | "choice"
  | "lines"
  | "json";

export interface Choice {
  value: unknown;
  label: string;
}

/** One condition on a field's visibility, from an `if`/`then` pair. */
export interface ShowWhen {
  field: string;
  equals?: unknown;
  oneOf?: unknown[];
  present?: boolean;
  /** Set on the `else` branch: the same test, the other way round. */
  negate?: boolean;
}

export interface FormField {
  name: string;
  label: string;
  kind: ControlKind;
  description?: string;
  /** `x-gmx-unit`: dB, ms, kbit/s. Shown beside the control, not in the value. */
  unit?: string;
  /** `x-gmx-group`: the section this belongs in, or "" for the top. */
  group: string;
  required: boolean;
  /** The current value, or the schema's default when there is none. */
  value: unknown;
  choices?: Choice[];
  min?: number;
  max?: number;
  step?: number;
  placeholder?: string;
  /** For `lines`: what each line is coerced to. */
  itemKind?: ControlKind;
  showWhen?: ShowWhen[];
  /** False when an `if`/`then` says this field does not apply right now. */
  visible: boolean;
}

export interface FormDescription {
  title?: string;
  description?: string;
  fields: FormField[];
  /** Group names in the order the schema introduced them, "" first. */
  groups: string[];
}

type Schema = Record<string, any>;

/** Read a schema and the current settings into something a UI can lay out. */
export function describeForm(schema: Schema, value: Record<string, unknown> = {}): FormDescription {
  const root = schema || {};
  const props: Schema = root.properties || {};
  const required = new Set<string>(root.required || []);
  const fields: FormField[] = [];
  const groups: string[] = [];

  for (const [name, raw] of Object.entries(props)) {
    const sub = resolve(root, raw as Schema);
    const group = String(sub["x-gmx-group"] || "");
    if (!groups.includes(group)) groups.push(group);
    const current = value[name] !== undefined ? value[name] : sub.default;
    fields.push({
      name,
      label: String(sub.title || name),
      kind: kindOf(sub),
      description: sub.description ? String(sub.description) : undefined,
      unit: sub["x-gmx-unit"] ? String(sub["x-gmx-unit"]) : undefined,
      group,
      required: required.has(name),
      value: current,
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

  const form: FormDescription = {
    title: root.title ? String(root.title) : undefined,
    description: root.description ? String(root.description) : undefined,
    fields,
    groups: groups.sort((a, b) => (a === "" ? -1 : b === "" ? 1 : 0)),
  };
  attachConditions(root, form);
  return applyConditions(form, valuesOf(form));
}

/** The value of every field, hidden ones included. What `if` is tested against. */
export function valuesOf(form: FormDescription): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const f of form.fields) if (f.value !== undefined) out[f.name] = f.value;
  return out;
}

/**
 * Work out which fields apply, given what is filled in now.
 *
 * Called once by `describeForm` and again by the UI on every edit, which is how
 * "codec: h264" makes the h264 fields appear.
 */
export function applyConditions(form: FormDescription, values: Record<string, unknown>): FormDescription {
  for (const field of form.fields) {
    if (!field.showWhen || field.showWhen.length === 0) continue;
    field.visible = field.showWhen.every((c) => holds(c, values));
  }
  return form;
}

/**
 * The object to send to the core.
 *
 * Hidden fields are left out, empty strings are left out rather than sent as
 * "", and a secret the operator did not retype is left out rather than blanked.
 * `touchedSecrets` is the set of secret field names the operator typed into.
 */
export function readForm(
  form: FormDescription,
  values: Record<string, unknown>,
  touchedSecrets: Set<string> = new Set(),
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const field of form.fields) {
    if (!field.visible) continue;
    if (field.kind === "secret" && !touchedSecrets.has(field.name)) continue;
    const raw = values[field.name];
    const value = coerce(raw, field);
    if (value === undefined) continue;
    out[field.name] = value;
  }
  return out;
}

/** Required fields that are visible and still empty. */
export function missing(form: FormDescription, values: Record<string, unknown>): string[] {
  const got = readForm(form, values, new Set(form.fields.map((f) => f.name)));
  return form.fields
    .filter((f) => f.required && f.visible)
    .filter((f) => got[f.name] === undefined || got[f.name] === "")
    .map((f) => f.name);
}

// ------------------------------------------------------------------ internals

function resolve(root: Schema, node: Schema): Schema {
  if (node && typeof node.$ref === "string" && node.$ref.startsWith("#/$defs/")) {
    const key = node.$ref.slice("#/$defs/".length);
    const defs = root.$defs || {};
    const copy = { ...node };
    delete copy.$ref;
    return Object.assign({}, defs[key] || {}, copy);
  }
  return node || {};
}

function typeOf(sub: Schema): string {
  return Array.isArray(sub.type) ? sub.type.find((t: string) => t !== "null") || "string" : sub.type || "string";
}

function kindOf(sub: Schema): ControlKind {
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

function choicesOf(sub: Schema): Choice[] | undefined {
  if (Array.isArray(sub.enum)) {
    return sub.enum.map((v: unknown) => ({ value: v, label: String(v) }));
  }
  if (Array.isArray(sub.oneOf)) {
    const consts = sub.oneOf.filter((o: Schema) => o && o.const !== undefined);
    if (consts.length === sub.oneOf.length) {
      return consts.map((o: Schema) => ({ value: o.const, label: String(o.title || o.const) }));
    }
  }
  return undefined;
}

function numberOr(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

/**
 * `if`/`then` visibility, in the one shape a settings schema uses: an `if` that
 * tests properties with `const` or `enum`, and a `then` that names further
 * properties. Anything under a failing `if` is hidden and left out of `read`.
 */
function attachConditions(root: Schema, form: FormDescription): void {
  const blocks: Schema[] = [];
  if (root.if) blocks.push(root);
  for (const block of root.allOf || []) if (block && block.if) blocks.push(block);

  for (const block of blocks) {
    const test: ShowWhen[] = [];
    for (const [name, cond] of Object.entries((block.if.properties || {}) as Schema)) {
      const c = cond as Schema;
      if (c.const !== undefined) test.push({ field: name, equals: c.const });
      else if (Array.isArray(c.enum)) test.push({ field: name, oneOf: c.enum });
    }
    for (const name of block.if.required || []) test.push({ field: name, present: true });

    for (const name of Object.keys((block.then && block.then.properties) || {})) {
      const field = form.fields.find((f) => f.name === name);
      if (field) field.showWhen = (field.showWhen || []).concat(test);
    }
    // The `else` branch applies when the `if` did not match. With one test
    // that is the test negated; with several it is "not all of them", which is
    // more than this reader promises, so only the single test case is honoured
    // and a multi test `else` shows its fields.
    if (test.length === 1) {
      for (const name of Object.keys((block.else && block.else.properties) || {})) {
        const field = form.fields.find((f) => f.name === name);
        if (field) field.showWhen = (field.showWhen || []).concat([negate(test[0]!)]);
      }
    }
  }
}

function negate(c: ShowWhen): ShowWhen {
  return { ...c, negate: !c.negate };
}

function holds(c: ShowWhen, values: Record<string, unknown>): boolean {
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

function coerce(raw: unknown, field: FormField): unknown {
  if (raw === undefined || raw === null) return undefined;
  switch (field.kind) {
    case "boolean":
      return Boolean(raw);
    case "integer": {
      if (raw === "") return undefined;
      const n = parseInt(String(raw), 10);
      return Number.isFinite(n) ? n : undefined;
    }
    case "number": {
      if (raw === "") return undefined;
      const n = Number(raw);
      return Number.isFinite(n) ? n : undefined;
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

/** A <select> hands back a string even when the schema's values are numbers. */
function coerceChoice(raw: unknown, field: FormField): unknown {
  const match = (field.choices || []).find((c) => String(c.value) === String(raw));
  return match ? match.value : raw;
}
