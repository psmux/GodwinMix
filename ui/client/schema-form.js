// JSON Schema draft 2020-12 to form controls.
//
// A UI never hardcodes a plugin's settings: it asks for the schema and renders
// it. Covered: objects, scalars, enums, arrays of scalars, `if`/`then`
// visibility, `format: "secret"` (a password field, never echoed back, sent
// only when retyped), `x-gmx-unit` (a suffix beside the control) and
// `x-gmx-group` (a collapsible section, so common fields sit above advanced
// ones without a second schema).
//
// Not covered, on purpose: $ref beyond `#/$defs/...`, oneOf discrimination,
// tuple arrays. A plugin needing those ships its own editor.

const SECRET_KEPT = "••••••••";

export class SchemaForm {
  /**
   * @param {object} schema  a JSON Schema object
   * @param {object} value   the current value, or {}
   */
  constructor(schema, value) {
    this.schema = schema || { type: "object", properties: {} };
    this.value = JSON.parse(JSON.stringify(value || {}));
    this.el = document.createElement("div");
    this.el.className = "form";
    this.fields = [];
    this.secretsTouched = new Set();
    this.onChange = null;
    this._build();
  }

  /** The object to send. Untouched secrets are left out rather than blanked. */
  read() {
    const out = {};
    for (const f of this.fields) {
      if (f.hidden) continue;
      const v = f.read();
      if (v === undefined) continue;
      out[f.name] = v;
    }
    return out;
  }

  /** Field names that are required and empty. */
  missing() {
    const required = this.schema.required || [];
    const got = this.read();
    return required.filter((name) => {
      const f = this.fields.find((x) => x.name === name);
      if (f && f.hidden) return false;
      const v = got[name];
      return v === undefined || v === "" || v === null;
    });
  }

  /** Mark the empty required fields and return whether the form is usable. */
  validate() {
    const bad = new Set(this.missing());
    for (const f of this.fields) f.wrap.classList.toggle("bad", bad.has(f.name));
    return bad.size === 0;
  }

  focusFirst() {
    const first = this.fields.find((f) => !f.hidden && f.input);
    if (first) first.input.focus();
  }

  // ---------------------------------------------------------------- build

  _build() {
    const props = this.schema.properties || {};
    const groups = new Map();
    for (const [name, raw] of Object.entries(props)) {
      const sub = this._resolve(raw);
      const field = this._field(name, sub);
      if (!field) continue;
      this.fields.push(field);
      const group = sub["x-gmx-group"] || "";
      if (!groups.has(group)) groups.set(group, []);
      groups.get(group).push(field.wrap);
    }
    // Ungrouped first, then a collapsible section per group, in the order the
    // schema introduced them.
    for (const [group, wraps] of groups) {
      if (group === "") {
        for (const w of wraps) this.el.appendChild(w);
        continue;
      }
      const details = document.createElement("details");
      const summary = document.createElement("summary");
      summary.textContent = group;
      details.appendChild(summary);
      const body = document.createElement("div");
      body.className = "form";
      body.style.marginTop = "var(--gap)";
      for (const w of wraps) body.appendChild(w);
      details.appendChild(body);
      this.el.appendChild(details);
    }
    this._applyConditions();
  }

  _resolve(node) {
    if (node && typeof node.$ref === "string" && node.$ref.startsWith("#/$defs/")) {
      const key = node.$ref.slice("#/$defs/".length);
      const defs = this.schema.$defs || {};
      return Object.assign({}, defs[key] || {}, omit(node, "$ref"));
    }
    return node || {};
  }

  _field(name, sub) {
    const wrap = document.createElement("div");
    wrap.className = "field";
    const label = document.createElement("span");
    label.className = "lbl";
    label.textContent = sub.title || name;
    if (sub["x-gmx-unit"]) {
      const unit = document.createElement("span");
      unit.className = "unit";
      unit.textContent = sub["x-gmx-unit"];
      label.appendChild(unit);
    }

    const current = this.value[name] !== undefined ? this.value[name] : sub.default;
    const made = this._control(name, sub, current);
    if (!made) return null;

    if (made.inline) {
      const row = document.createElement("label");
      row.className = "inline";
      row.appendChild(made.input);
      row.appendChild(label);
      wrap.appendChild(row);
    } else {
      const lab = document.createElement("label");
      lab.appendChild(label);
      lab.appendChild(made.input);
      wrap.appendChild(lab);
    }

    if (sub.description) {
      const hint = document.createElement("span");
      hint.className = "hint";
      hint.textContent = sub.description;
      wrap.appendChild(hint);
    }

    const field = { name, wrap, input: made.input, read: made.read, hidden: false };
    made.input.addEventListener("input", () => {
      if (made.secret) this.secretsTouched.add(name);
      this._applyConditions();
      if (this.onChange) this.onChange(this.read());
    });
    made.input.addEventListener("change", () => {
      this._applyConditions();
      if (this.onChange) this.onChange(this.read());
    });
    return field;
  }

  _control(name, sub, current) {
    const type = Array.isArray(sub.type) ? sub.type.find((t) => t !== "null") : sub.type;

    if (Array.isArray(sub.enum)) {
      const select = document.createElement("select");
      for (const option of sub.enum) {
        const o = document.createElement("option");
        o.value = String(option);
        o.textContent = String(option);
        select.appendChild(o);
      }
      if (current !== undefined) select.value = String(current);
      return { input: select, read: () => coerce(select.value, type) };
    }

    if (type === "boolean") {
      const input = document.createElement("input");
      input.type = "checkbox";
      input.checked = !!current;
      input.style.width = "auto";
      return { input, inline: true, read: () => input.checked };
    }

    if (type === "integer" || type === "number") {
      const input = document.createElement("input");
      input.type = "number";
      if (sub.minimum !== undefined) input.min = String(sub.minimum);
      if (sub.maximum !== undefined) input.max = String(sub.maximum);
      input.step = type === "integer" ? "1" : sub.multipleOf ? String(sub.multipleOf) : "any";
      if (current !== undefined && current !== null) input.value = String(current);
      return {
        input,
        read: () => {
          if (input.value === "") return undefined;
          const n = Number(input.value);
          return Number.isFinite(n) ? n : undefined;
        },
      };
    }

    if (type === "array") {
      // One value per line. Enough for a list of URLs or names, and a plugin
      // that needs more than that ships its own editor.
      const input = document.createElement("textarea");
      input.rows = 3;
      if (Array.isArray(current)) input.value = current.join("\n");
      const items = this._resolve(sub.items || {});
      return {
        input,
        read: () =>
          input.value
            .split("\n")
            .map((s) => s.trim())
            .filter(Boolean)
            .map((s) => coerce(s, items.type)),
      };
    }

    if (type === "object") {
      const input = document.createElement("textarea");
      input.rows = 4;
      input.spellcheck = false;
      input.value = current ? JSON.stringify(current, null, 2) : "";
      return {
        input,
        read: () => {
          if (!input.value.trim()) return undefined;
          try {
            return JSON.parse(input.value);
          } catch {
            return undefined;
          }
        },
      };
    }

    // Strings, and anything the schema did not name a type for.
    const secret = sub.format === "secret" || sub.format === "password";
    const input = document.createElement("input");
    input.type = secret ? "password" : sub.format === "uri" ? "url" : "text";
    if (secret) {
      input.autocomplete = "new-password";
      input.value = current ? SECRET_KEPT : "";
      input.placeholder = current ? "unchanged" : "";
    } else if (current !== undefined && current !== null) {
      input.value = String(current);
    }
    if (sub.examples && sub.examples.length) input.placeholder = String(sub.examples[0]);
    return {
      input,
      secret,
      read: () => {
        if (secret && !this.secretsTouched.has(name)) return undefined;
        return input.value === "" ? undefined : input.value;
      },
    };
  }

  /**
   * `if`/`then` visibility. Only the shape a settings schema uses: an `if` that
   * tests one or more properties with `const` or `enum`, and a `then` that
   * names further properties. A matching `if` shows them; anything under a
   * failing `if` is hidden and left out of `read()`.
   */
  _applyConditions() {
    const conds = [];
    if (this.schema.if) conds.push(this.schema);
    for (const c of this.schema.allOf || []) if (c.if) conds.push(c);
    if (conds.length === 0) return;

    const conditional = new Set();
    for (const c of conds) {
      for (const name of Object.keys((c.then && c.then.properties) || {})) conditional.add(name);
      for (const name of Object.keys((c.else && c.else.properties) || {})) conditional.add(name);
    }
    const shown = new Set();
    const current = this.readRaw();
    for (const c of conds) {
      const branch = matches(c.if, current) ? c.then : c.else;
      for (const name of Object.keys((branch && branch.properties) || {})) shown.add(name);
    }
    for (const f of this.fields) {
      if (!conditional.has(f.name)) continue;
      f.hidden = !shown.has(f.name);
      f.wrap.hidden = f.hidden;
    }
  }

  /** Every field including the hidden ones, which is what `if` tests against. */
  readRaw() {
    const out = {};
    for (const f of this.fields) {
      const v = f.read();
      if (v !== undefined) out[f.name] = v;
    }
    return out;
  }
}

function matches(cond, value) {
  if (!cond) return false;
  const props = cond.properties || {};
  for (const [name, test] of Object.entries(props)) {
    const v = value[name];
    if (test.const !== undefined && v !== test.const) return false;
    if (Array.isArray(test.enum) && !test.enum.includes(v)) return false;
  }
  for (const name of cond.required || []) {
    if (value[name] === undefined || value[name] === "") return false;
  }
  return true;
}

function coerce(text, type) {
  if (type === "integer") return parseInt(text, 10);
  if (type === "number") return Number(text);
  if (type === "boolean") return text === "true" || text === true;
  return text;
}

function omit(obj, key) {
  const copy = Object.assign({}, obj);
  delete copy[key];
  return copy;
}
