// The fallback chain, which is the promise of 11 section 5: no missing editor
// ever blocks a user.
//
//   1. the plugin's own web component, when it ships one and the page trusts it
//   2. else the UI schema, rendered natively
//   3. else the data schema with default widgets
//   4. else a raw JSON box
//
// Home Assistant's YAML fallback and VS Code's "edit in settings.json" are the
// same idea: the last link always works, so a plugin written against a newer
// core is still editable on an older client, and a plugin that ships nothing
// but a schema is still editable at all.

import { el } from "../../shell/dom.js";
import { SchemaInspector, defaultRenderers } from "./render.js";

export { SchemaInspector, defaultRenderers } from "./render.js";
export { Renderers, registerTable, BASE, SPECIFIC, HOST } from "./registry.js";
export { describeForm, readForm, valuesOf, applyConditions, missing } from "./describe.js";
export { layoutFor, defaultLayout, applyRules, controlsOf, controlFor, CONTROLS } from "./ui-schema.js";

/**
 * Build the best editor this client can manage for one item type.
 *
 * @param {{plugin?: string, designer?: object, schema?: object, value?: object,
 *          trusted?: boolean, onChange?: Function, base?: string}} spec
 * @returns {Promise<{mode: string, el: Node, read: () => object, why: string}>}
 */
export async function editorFor(spec) {
  const value = spec.value || {};
  if (spec.designer && spec.designer.editor && spec.trusted !== false && spec.plugin) {
    const made = await customElement(spec, value);
    if (made) return made;
  }
  const ui = await uiSchema(spec);
  if (spec.schema && Object.keys(spec.schema.properties || {}).length) {
    const inspector = new SchemaInspector({
      schema: spec.schema,
      ui,
      value,
      onChange: spec.onChange,
      renderers: spec.renderers || defaultRenderers(),
    });
    return {
      mode: ui ? "ui-schema" : "schema",
      why: ui ? "the plugin's UI schema" : "the plugin's data schema, with default widgets",
      el: inspector.el,
      read: () => inspector.read(),
    };
  }
  return rawJson(value, spec.onChange);
}

/**
 * The plugin's own custom element, from `/plugins/<name>/ui/<entry>`.
 *
 * It is loaded as a module and is expected to have defined a custom element by
 * the time the import resolves. A module that throws, or defines nothing, is
 * not an error the operator has to care about: the chain carries on to the UI
 * schema and the console says what happened.
 */
async function customElement(spec, value) {
  const entry = String(spec.designer.editor);
  const url = entry.includes("://") ? entry : `${spec.base || ""}/plugins/${spec.plugin}/ui/${entry.replace(/^ui\//, "")}`;
  try {
    const module = await import(/* @vite-ignore */ url);
    // The module says what it defined. `export const tag = "gmx-chroma-editor"`
    // is one line, and it beats this file guessing at a naming convention.
    const tag = typeof module.tag === "string" ? module.tag : `gmx-${spec.plugin}-editor`;
    if (!customElements.get(tag)) {
      console.warn(`${spec.plugin} loaded an editor but defined no element called ${tag}`);
      return null;
    }
    const node = document.createElement(tag);
    if (typeof node.setSchema === "function") node.setSchema(spec.schema || {});
    if (typeof node.setValue === "function") node.setValue(value);
    else node.value = value;
    if (spec.onChange) node.addEventListener("change", () => spec.onChange(readNode(node), null));
    return {
      mode: "custom-element",
      why: `the editor ${spec.plugin} ships`,
      el: node,
      read: () => readNode(node),
    };
  } catch (e) {
    console.warn(`the editor from ${spec.plugin} did not load, falling back to its schema`, e);
    return null;
  }
}

function readNode(node) {
  if (typeof node.read === "function") return node.read();
  return node.value || {};
}

/** Fetch the UI schema the designer block names, when there is one. */
async function uiSchema(spec) {
  if (spec.ui) return spec.ui;
  const path = spec.designer && spec.designer.ui;
  if (!path || !spec.plugin) return null;
  const url = `${spec.base || ""}/plugins/${spec.plugin}/ui/${String(path).replace(/^(ui|schemas)\//, "")}`;
  try {
    const res = await fetch(url);
    if (!res.ok) return null;
    return await res.json();
  } catch {
    // A UI schema that will not load costs the layout and nothing else: the
    // data schema below still renders every property.
    return null;
  }
}

/** The last link: the value itself, editable, always available. */
function rawJson(value, onChange) {
  const box = el("textarea", {
    rows: "8",
    spellcheck: "false",
    value: JSON.stringify(value, null, 2),
    style: { width: "100%", fontFamily: "var(--num)" },
  });
  const err = el("span.err", { hidden: true });
  let parsed = value;
  box.oninput = () => {
    try {
      parsed = JSON.parse(box.value || "{}");
      err.hidden = true;
      if (onChange) onChange(parsed, null);
    } catch (e) {
      err.hidden = false;
      err.textContent = e.message;
    }
  };
  return {
    mode: "json",
    why: "this item type ships no schema, so its settings are the values themselves",
    el: el("div.col", {}, [el("span.sm.dim", { text: "No schema for this item type. Edit the values directly." }), box, err]),
    read: () => parsed,
  };
}
