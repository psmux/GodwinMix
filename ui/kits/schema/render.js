// The schema kit's widgets, for a browser.
//
// One function per control name, registered into a ranked registry, so a panel
// that wants a better colour picker registers a higher rank and nothing else
// changes. The markup is the shell's own form classes, so an inspector looks
// like the rest of the page without a stylesheet of its own.

import { el } from "../../shell/dom.js";
import { Renderers, registerTable, BASE, SPECIFIC } from "./registry.js";
import { applyRules, controlsOf, layoutFor } from "./ui-schema.js";
import { describeForm, readForm, valuesOf, applyConditions } from "./describe.js";

/** A form built from a data schema, a UI schema, or both. */
export class SchemaInspector {
  /**
   * @param {{schema: object, ui?: object, value?: object,
   *          onChange?: (values: object, name: string) => void,
   *          renderers?: Renderers}} opts
   */
  constructor(opts) {
    this.form = describeForm(opts.schema || {}, opts.value || {});
    this.layout = layoutFor(this.form, opts.ui);
    this.renderers = opts.renderers || defaultRenderers();
    this.values = valuesOf(this.form);
    this.onChange = opts.onChange || null;
    this.touchedSecrets = new Set();
    this.nodes = new Map();
    this.el = el("div.form");
    this.build();
  }

  /** The object to send: hidden fields out, untouched secrets left alone. */
  read() {
    applyConditions(this.form, this.values);
    return readForm(this.form, this.values, this.touchedSecrets);
  }

  build() {
    applyRules(applyConditionsInto(this.layout, this.form, this.values), this.values);
    this.el.appendChild(this.node(this.layout));
    this.refresh();
  }

  node(n) {
    if (n.kind === "control") return this.control(n);
    const body = el(n.kind === "row" ? "div.row" : "div.form");
    for (const child of n.elements || []) body.appendChild(this.node(child));
    if (!n.label) {
      this.nodes.set(n, body);
      return body;
    }
    const box = el("details", { open: true }, [el("summary", { text: n.label }), body]);
    this.nodes.set(n, box);
    return box;
  }

  control(n) {
    const picked = this.renderers.pick(n);
    const ctx = {
      value: () => this.values[n.field.name],
      set: (v) => this.set(n.field.name, v),
      touchSecret: () => this.touchedSecrets.add(n.field.name),
    };
    const input = picked ? picked.entry.make(n, ctx) : jsonBox(n, ctx);
    const wrap = el("div.field", {}, [
      el("span.lbl", {}, [n.label, n.field.unit ? el("span.unit", { text: n.field.unit }) : null]),
      input,
      n.field.description ? el("span.hint", { text: n.field.description }) : null,
    ]);
    this.nodes.set(n, wrap);
    return wrap;
  }

  set(name, value) {
    this.values[name] = value;
    applyConditions(this.form, this.values);
    applyRules(this.layout, this.values);
    this.refresh();
    if (this.onChange) this.onChange(this.values, name);
  }

  /** Show and hide from the rules. Nothing is rebuilt: a rebuild loses focus. */
  refresh() {
    for (const [n, node] of this.nodes) {
      node.hidden = n.visible === false;
      if (n.kind === "control") {
        for (const input of node.querySelectorAll("input, select, textarea, button")) {
          input.disabled = n.enabled === false;
        }
      }
    }
  }
}

/** The data schema's own `if`/`then`, folded into the layout's own rules. */
function applyConditionsInto(layout, form, values) {
  applyConditions(form, values);
  for (const node of controlsOf(layout)) {
    const field = form.fields.find((f) => f.name === node.field.name);
    if (field) node.field = field;
  }
  return layout;
}

// ------------------------------------------------------------------ widgets

export function defaultRenderers() {
  const r = new Renderers();
  registerTable(r, {
    text: (n, ctx) => textInput(n, ctx, "text"),
    textarea: (n, ctx) => area(n, ctx),
    number: (n, ctx) => numberInput(n, ctx, "any"),
    integer: (n, ctx) => numberInput(n, ctx, "1"),
    slider: (n, ctx) => slider(n, ctx),
    boolean: (n, ctx) => toggle(n, ctx),
    select: (n, ctx) => select(n, ctx, false),
    multiselect: (n, ctx) => select(n, ctx, true),
    radio: (n, ctx) => radio(n, ctx),
    colour: (n, ctx) => colour(n, ctx),
    file: (n, ctx) => textInput(n, ctx, "text"),
    font: (n, ctx) => textInput(n, ctx, "text"),
    source: (n, ctx) => select(n, ctx, false),
    unit: (n, ctx) => numberInput(n, ctx, "any"),
    json: (n, ctx) => jsonBox(n, ctx),
  }, BASE);
  // The host tier: a plugin that asks for `template.colour` gets a real colour
  // input without shipping one, which is the escape hatch 11 section 5 names.
  registerTable(r, {
    "template.colour": (n, ctx) => colour(n, ctx),
    "template.chroma": (n, ctx) => colour(n, ctx),
    "template.curve": (n, ctx) => slider(n, ctx),
    "template.meter": (n, ctx) => slider(n, ctx),
  }, SPECIFIC);
  // A secret is a secret whatever the UI schema calls it.
  r.register({
    name: "secret",
    test: (n) => (n.field.kind === "secret" ? SPECIFIC + 1 : 0),
    make: (n, ctx) => textInput(n, ctx, "password"),
  });
  return r;
}

function textInput(n, ctx, type) {
  const input = el("input", { type, value: str(ctx.value()), placeholder: n.field.placeholder || "" });
  input.oninput = () => {
    if (n.field.kind === "secret") ctx.touchSecret();
    ctx.set(input.value);
  };
  return input;
}

function area(n, ctx) {
  const value = ctx.value();
  const input = el("textarea", { rows: "3", value: Array.isArray(value) ? value.join("\n") : str(value) });
  input.oninput = () => ctx.set(input.value);
  return input;
}

function numberInput(n, ctx, step) {
  const input = el("input", {
    type: "number",
    value: str(ctx.value()),
    step: n.field.step ? String(n.field.step) : step,
    min: n.field.min === undefined ? null : String(n.field.min),
    max: n.field.max === undefined ? null : String(n.field.max),
  });
  input.oninput = () => ctx.set(input.value === "" ? undefined : Number(input.value));
  return input;
}

function slider(n, ctx) {
  const min = n.field.min === undefined ? (n.options && n.options.min) || 0 : n.field.min;
  const max = n.field.max === undefined ? (n.options && n.options.max) || 1 : n.field.max;
  const step = n.field.step || (n.options && n.options.step) || (max - min) / 100;
  const input = el("input", { type: "range", min: String(min), max: String(max), step: String(step), value: str(ctx.value() ?? min) });
  const out = el("span.num.sm", { text: str(ctx.value() ?? min) });
  input.oninput = () => {
    out.textContent = input.value;
    ctx.set(Number(input.value));
  };
  return el("div.row", {}, [input, out]);
}

function toggle(n, ctx) {
  const input = el("input", { type: "checkbox", checked: !!ctx.value() });
  input.onchange = () => ctx.set(input.checked);
  return el("label.inline", {}, [input, el("span.sm.dim", { text: (n.options && n.options.label) || "" })]);
}

function select(n, ctx, many) {
  const choices = n.field.choices || (n.options && n.options.choices) || [];
  const input = el("select", many ? { multiple: true, size: String(Math.min(5, choices.length || 3)) } : {});
  for (const choice of choices) {
    const value = choice && typeof choice === "object" ? choice.value : choice;
    const label = choice && typeof choice === "object" ? choice.label : String(choice);
    input.appendChild(el("option", { value: String(value), text: label }));
  }
  const current = ctx.value();
  if (many && Array.isArray(current)) {
    for (const option of input.options) option.selected = current.map(String).includes(option.value);
  } else if (current !== undefined) {
    input.value = String(current);
  }
  input.onchange = () => {
    if (many) ctx.set([...input.selectedOptions].map((o) => o.value));
    else ctx.set(typed(input.value, choices));
  };
  return input;
}

function radio(n, ctx) {
  const row = el("div.row");
  const name = "r-" + n.field.name;
  for (const choice of n.field.choices || []) {
    const value = choice && typeof choice === "object" ? choice.value : choice;
    const input = el("input", { type: "radio", name, checked: ctx.value() === value });
    input.onchange = () => ctx.set(value);
    row.appendChild(el("label.inline", {}, [input, el("span", { text: String(choice.label ?? value) })]));
  }
  return row;
}

function colour(n, ctx) {
  const value = str(ctx.value()) || "#000000";
  const input = el("input", { type: "color", value: /^#[0-9a-f]{6}$/i.test(value) ? value : "#000000" });
  const text = el("input", { type: "text", value, style: { maxWidth: "10ch" } });
  input.oninput = () => {
    text.value = input.value;
    ctx.set(input.value);
  };
  text.oninput = () => ctx.set(text.value);
  return el("div.row", {}, [input, text]);
}

function jsonBox(n, ctx) {
  const value = ctx.value();
  const input = el("textarea", {
    rows: "4",
    spellcheck: "false",
    value: value === undefined ? "" : JSON.stringify(value, null, 2),
  });
  const err = el("span.err", { hidden: true });
  input.oninput = () => {
    if (!input.value.trim()) {
      err.hidden = true;
      ctx.set(undefined);
      return;
    }
    try {
      ctx.set(JSON.parse(input.value));
      err.hidden = true;
    } catch (e) {
      err.hidden = false;
      err.textContent = e.message;
    }
  };
  return el("div.col", {}, [input, err]);
}

function typed(text, choices) {
  for (const choice of choices) {
    const value = choice && typeof choice === "object" ? choice.value : choice;
    if (String(value) === text) return value;
  }
  return text;
}

function str(v) {
  return v === undefined || v === null ? "" : String(v);
}
