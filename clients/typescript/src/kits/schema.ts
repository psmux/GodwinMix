// The schema kit: the UI schema layer and the ranked renderer registry.
//
// The data schema reader is not duplicated here. `describeForm`, `valuesOf`,
// `applyConditions`, `readForm` and `missing` already live in
// ../schema-form.ts with the same semantics as ui/kits/schema/describe.js, so
// this file imports them and re-exports them under the kit's own name. One
// reader, one set of fixtures, no second copy to drift.
//
// What is added here is a typed port of ui/kits/schema/ui-schema.js and
// ui/kits/schema/registry.js. ui/kits/schema/render.js is not ported: it builds
// DOM widgets and belongs to the browser, and nothing below touches a document.

import type { FormDescription, FormField } from "../schema-form.ts";

export {
  applyConditions,
  describeForm,
  missing,
  readForm,
  valuesOf,
} from "../schema-form.ts";
export type { Choice, ControlKind, FormDescription, FormField, ShowWhen } from "../schema-form.ts";

// ------------------------------------------------------------- the UI schema
//
// JSON Schema says presentation is out of scope, and it is right: a number
// between 0 and 1 might be a slider, a spin box or a dial, and no amount of
// staring at the data schema tells you which. So a plugin ships a second, small
// document (JSON Forms' split) and this file merges the two into a layout.
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
const BY_KIND: Record<string, string> = {
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

/** One condition, as a UI schema writes it. */
export interface UiCondition {
  scope?: string;
  equals?: unknown;
  const?: unknown;
  oneOf?: unknown[];
  enum?: unknown[];
}

export interface UiRule {
  effect?: string;
  condition?: UiCondition;
}

/** A node of a plugin's UI schema, as it arrives. */
export interface UiNode {
  type?: string;
  label?: string;
  /** `#/properties/gain`, or a bare property name. */
  scope?: string;
  control?: string;
  options?: Record<string, unknown>;
  rule?: UiRule;
  elements?: UiNode[];
  [key: string]: unknown;
}

/** A rule after it has been read: one field, one test, one effect. */
export interface LayoutRule {
  effect: string;
  field: string | null;
  equals?: unknown;
  oneOf?: unknown[];
}

interface LayoutCommon {
  /** Set by applyRules. */
  visible?: boolean;
  /** Set by applyRules. */
  enabled?: boolean;
  rule?: LayoutRule | null;
  elements?: LayoutNode[];
}

export interface LayoutControl extends LayoutCommon {
  kind: "control";
  field: FormField;
  control: string;
  label: string;
  options: Record<string, unknown>;
}

export interface LayoutContainer extends LayoutCommon {
  kind: "group" | "row" | "tabs";
  label: string;
  elements: LayoutNode[];
}

export type LayoutNode = LayoutControl | LayoutContainer;

/** The control a field ends up with: what was declared, or what fits. */
export function controlFor(field: FormField, declared?: string | null): string {
  if (declared && CONTROLS.includes(declared)) return declared;
  return BY_KIND[field.kind] || "text";
}

/** `#/properties/gain` names the field `gain`. A bare name is accepted too. */
export function fieldOfScope(scope: string | null | undefined): string | null {
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
 */
export function layoutFor(form: FormDescription, ui: UiNode | null | undefined): LayoutNode {
  // An array is not a UI schema. Walking one as a container spec would produce
  // a layout with no controls in it and no error to say why.
  if (!ui || typeof ui !== "object" || Array.isArray(ui)) return defaultLayout(form);
  const used = new Set<string>();
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
export function defaultLayout(form: FormDescription): LayoutContainer {
  const elements: LayoutNode[] = [];
  for (const group of form.groups) {
    const fields = form.fields.filter((f) => (f.group || "") === group);
    if (!fields.length) continue;
    elements.push({ kind: "group", label: group, elements: fields.map((f) => leaf(f, null, null)) });
  }
  return { kind: "group", label: "", elements };
}

function node(spec: UiNode, form: FormDescription, used: Set<string>): LayoutNode {
  const type = String(spec.type || (spec.control || spec.scope ? "control" : "vertical"));
  if (type === "control" || spec.scope) {
    const name = fieldOfScope(spec.scope);
    const field = form.fields.find((f) => f.name === name);
    if (field) used.add(field.name);
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

/**
 * A scope that names nothing still gets a control, so a typo in a UI schema
 * shows up as an empty text box rather than as a missing setting. The reference
 * kit builds a partial field here; this one fills in the rest of the shape so
 * every consumer can read a layout control without checking for holes.
 */
function stubField(name: string | null): FormField {
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

function leaf(field: FormField, control: string | null | undefined, spec: UiNode | null): LayoutControl {
  return {
    kind: "control",
    field,
    control: controlFor(field, control || (spec && spec.control)),
    label: (spec && spec.label) || field.label || field.name,
    options: (spec && spec.options) || {},
    rule: ruleOf(spec),
  };
}

function ruleOf(spec: UiNode | null | undefined): LayoutRule | null {
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
export function applyRules<T extends LayoutNode>(layout: T, values: Record<string, unknown>): T {
  const visit = (n: LayoutNode): void => {
    const state = holds(n.rule, values);
    n.visible = state.visible && (n.kind !== "control" || n.field.visible !== false);
    n.enabled = state.enabled;
    for (const child of n.elements || []) visit(child);
  };
  visit(layout);
  return layout;
}

function holds(
  rule: LayoutRule | null | undefined,
  values: Record<string, unknown>,
): { visible: boolean; enabled: boolean } {
  // A condition with no scope names no field, so there is nothing to test and
  // nothing to hide. It renders rather than disappearing on a typo.
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
export function controlsOf(layout: LayoutNode): LayoutControl[] {
  const out: LayoutControl[] = [];
  const walk = (n: LayoutNode): void => {
    if (n.kind === "control") out.push(n);
    for (const child of n.elements || []) walk(child);
  };
  walk(layout);
  return out;
}

// --------------------------------------------------------- the tester registry
//
// A ranked tester registry, so a client can render a format better than the kit
// does without the plugin knowing.
//
// Every renderer answers one question: how well do you handle this control? The
// highest number wins, ties go to whoever registered last, and a renderer that
// answers zero or less is not asked again. That is JSON Forms' model and it is
// the reason a host can add a colour picker, a curve editor or an audio meter
// for `template.*` controls that no plugin ships code for.
//
// Nothing here is browser specific. The Python kit registers ttk widgets
// against the same ranks and gets the same choices.

/** What the kit's own widgets rank at. Beat it to take over a control. */
export const BASE = 10;
export const SPECIFIC = 20;
export const HOST = 30;

export interface RendererEntry<W = unknown, C = unknown> {
  name: string;
  test: (node: LayoutControl, field: FormField) => number;
  make: (node: LayoutControl, ctx: C) => W;
}

export interface Picked<W = unknown, C = unknown> {
  entry: RendererEntry<W, C>;
  rank: number;
}

export class Renderers<W = unknown, C = unknown> {
  entries: Array<RendererEntry<W, C>>;

  constructor() {
    this.entries = [];
  }

  /** @returns removal, so a panel can take its renderer away again. */
  register(entry: RendererEntry<W, C>): () => void {
    if (!entry || typeof entry.test !== "function" || typeof entry.make !== "function") {
      throw new Error("a renderer needs a test and a make function");
    }
    this.entries.push(entry);
    return () => {
      const at = this.entries.indexOf(entry);
      if (at >= 0) this.entries.splice(at, 1);
    };
  }

  /** The best renderer for one control node, or null when nothing will have it. */
  pick(node: LayoutControl): Picked<W, C> | null {
    let best: RendererEntry<W, C> | null = null;
    let bestRank = 0;
    // Later registrations win ties, so a client's own renderer overrides the
    // kit's without having to invent a higher number.
    for (const entry of this.entries) {
      const rank = Number(entry.test(node, node.field)) || 0;
      if (rank > 0 && rank >= bestRank) {
        bestRank = rank;
        best = entry;
      }
    }
    return best ? { entry: best, rank: bestRank } : null;
  }

  /** What would render each control, by name. For a test and for a doctor. */
  explain(layout: LayoutNode): Record<string, string | null> {
    const out: Record<string, string | null> = {};
    const walk = (n: LayoutNode): void => {
      if (n.kind === "control") {
        const picked = this.pick(n);
        out[n.field.name] = picked ? picked.entry.name : null;
      }
      for (const child of n.elements || []) walk(child);
    };
    walk(layout);
    return out;
  }
}

/**
 * Register one renderer per control name, all at the same rank.
 * The shape most clients want: a table from control to widget maker.
 */
export function registerTable<W, C>(
  registry: Renderers<W, C>,
  table: Record<string, (node: LayoutControl, ctx: C) => W>,
  rank: number = BASE,
): () => void {
  const offs: Array<() => void> = [];
  for (const [control, make] of Object.entries(table)) {
    offs.push(
      registry.register({
        name: control,
        test: (node) => (node.control === control ? rank : 0),
        make,
      }),
    );
  }
  return () => offs.forEach((off) => off());
}
