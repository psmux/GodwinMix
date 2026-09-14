// The UI schema: where the mode is declared rather than inferred.
//
// JSON Schema says presentation is out of scope, and it is right: a number
// between 0 and 1 might be a slider, a spin box or a dial, and no amount of
// staring at the data schema tells you which. So a plugin ships a second, small
// document (JSON Forms' split) and this file merges the two into a layout.
//
// The vocabulary is fifteen typed selectors plus a `template.*` tier the host
// provides, because Blender, Grafana and VS Code all needed that escape hatch:
//
//   text  textarea  number  slider  integer  boolean  select  multiselect
//   radio  colour  file  font  source  unit  json
//   template.colour  template.curve  template.chroma  template.meter
//
// Containers are `vertical`, `horizontal`, `group` and `tabs`. A leaf is a
// `control` with a `scope` pointing at a property of the data schema. A `rule`
// shows, hides, enables or disables an element from another property's value.
//
// Anything a client does not know is rendered by whatever it does know: an
// unknown control falls back to the data schema's own default widget, and an
// unknown container is laid out vertically. A UI schema from a newer plugin
// therefore renders on an older client, which is the whole reason it is data.

/** Every control name this vocabulary defines. */
export const CONTROLS = [
  "text",
  "textarea",
  "number",
  "slider",
  "integer",
  "boolean",
  "select",
  "multiselect",
  "radio",
  "colour",
  "file",
  "font",
  "source",
  "unit",
  "json",
  "template.colour",
  "template.curve",
  "template.chroma",
  "template.meter",
];

/** What a field gets when nothing declares a control for it. */
const BY_KIND = {
  text: "text",
  secret: "text",
  url: "text",
  number: "number",
  integer: "integer",
  boolean: "boolean",
  choice: "select",
  lines: "textarea",
  json: "json",
};

/** The control a field ends up with: what was declared, or what fits. */
export function controlFor(field, declared) {
  if (declared && CONTROLS.includes(declared)) return declared;
  return BY_KIND[field.kind] || "text";
}

/** `#/properties/gain` names the field `gain`. A bare name is accepted too. */
export function fieldOfScope(scope) {
  if (!scope) return null;
  const text = String(scope);
  const at = text.lastIndexOf("/");
  return at >= 0 ? text.slice(at + 1) : text;
}

/**
 * Merge a data schema description with a UI schema into a layout tree.
 *
 * Every field of the data schema appears exactly once. A field the UI schema
 * forgot is appended in a section of its own rather than dropped, because a
 * plugin author who adds a property and forgets the UI schema should end up
 * with an ugly form, never an unreachable setting.
 *
 * @returns {{kind: "group"|"row"|"control"|"tabs", ...}}
 */
export function layoutFor(form, ui) {
  // An array is not a UI schema. Walking one as a container spec would produce
  // a layout with no controls in it and no error to say why.
  if (!ui || typeof ui !== "object" || Array.isArray(ui)) return defaultLayout(form);
  const used = new Set();
  const root = node(ui, form, used);
  const left = form.fields.filter((f) => !used.has(f.name));
  if (left.length) {
    root.elements = (root.elements || []).concat([
      {
        kind: "group",
        label: root.elements && root.elements.length ? "More" : "",
        elements: left.map((f) => leaf(f, null, null)),
      },
    ]);
  }
  return root;
}

/** A layout from the data schema alone: the groups it declared, in order. */
export function defaultLayout(form) {
  const elements = [];
  for (const group of form.groups) {
    const fields = form.fields.filter((f) => (f.group || "") === group);
    if (!fields.length) continue;
    elements.push({ kind: "group", label: group, elements: fields.map((f) => leaf(f, null, null)) });
  }
  return { kind: "group", label: "", elements };
}

function node(spec, form, used) {
  const type = String(spec.type || (spec.control || spec.scope ? "control" : "vertical"));
  if (type === "control" || spec.scope) {
    const name = fieldOfScope(spec.scope);
    const field = form.fields.find((f) => f.name === name);
    if (field) used.add(field.name);
    // A control naming a property the data schema does not have still renders,
    // as a text box over nothing, rather than producing a half built field that
    // every reader of the layout then has to guard against.
    return leaf(field || stubField(name), spec.control, spec);
  }
  const kind = type === "horizontal" ? "row" : type === "tabs" ? "tabs" : "group";
  return {
    kind,
    label: spec.label ? String(spec.label) : "",
    rule: ruleOf(spec),
    elements: (spec.elements || []).map((child) => node(child, form, used)),
  };
}

/** The shape of a form field, with nothing in it. Never null, never partial. */
function stubField(name) {
  return {
    name: name || "",
    label: name || "",
    kind: "text",
    group: "",
    required: false,
    value: undefined,
    visible: true,
  };
}

function leaf(field, control, spec) {
  return {
    kind: "control",
    field,
    control: controlFor(field, control || (spec && spec.control)),
    label: (spec && spec.label) || field.label || field.name,
    options: (spec && spec.options) || {},
    rule: ruleOf(spec),
  };
}

function ruleOf(spec) {
  const rule = spec && spec.rule;
  if (!rule || !rule.condition) return null;
  const c = rule.condition;
  return {
    effect: String(rule.effect || "show").toLowerCase(),
    field: fieldOfScope(c.scope),
    equals: c.equals !== undefined ? c.equals : c.const,
    oneOf: Array.isArray(c.oneOf) ? c.oneOf : Array.isArray(c.enum) ? c.enum : undefined,
  };
}

/**
 * Walk the layout and mark each node visible and enabled, from the rules and
 * from the data schema's own `if`/`then` conditions.
 */
export function applyRules(layout, values) {
  const visit = (n) => {
    const state = holds(n.rule, values);
    n.visible = state.visible && (n.kind !== "control" || n.field.visible !== false);
    n.enabled = state.enabled;
    for (const child of n.elements || []) visit(child);
  };
  visit(layout);
  return layout;
}

function holds(rule, values) {
  if (!rule || !rule.field) return { visible: true, enabled: true };
  const value = values[rule.field];
  let met = true;
  if (rule.oneOf !== undefined) met = rule.oneOf.includes(value);
  else if (rule.equals !== undefined) met = value === rule.equals;
  else met = value !== undefined && value !== null && value !== "";
  switch (rule.effect) {
    case "hide":
      return { visible: !met, enabled: true };
    case "enable":
      return { visible: true, enabled: met };
    case "disable":
      return { visible: true, enabled: !met };
    default:
      return { visible: met, enabled: true };
  }
}

/** Every control node in a layout, in draw order. */
export function controlsOf(layout) {
  const out = [];
  const walk = (n) => {
    if (n.kind === "control") out.push(n);
    for (const child of n.elements || []) walk(child);
  };
  walk(layout);
  return out;
}
