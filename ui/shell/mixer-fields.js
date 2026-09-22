// What Mixer Settings adds under each field the schema kit drew: a line for a
// refusal, a line for when the change takes effect, a Default button, and for
// two keys a helper (a folder chooser for the media folder, a token generator
// for the control token).

import { el } from "./dom.js";
import { newToken } from "./mixer-form.js";

/** The layout nodes the inspector drew, by key. */
export function fieldWraps(inspector) {
  const out = new Map();
  for (const [n, node] of inspector.nodes) if (n.kind === "control" && n.field) out.set(n.field.name, node);
  return out;
}

/** Put a value into a drawn field and tell the inspector, as typing would. */
export function putValue(wrap, value) {
  const input = wrap.querySelector("input, select, textarea");
  if (!input) return;
  if (input.type === "checkbox") {
    input.checked = !!value;
    input.dispatchEvent(new Event("change"));
  } else {
    input.value = value === undefined || value === null ? "" : String(value);
    input.dispatchEvent(new Event(input.tagName === "SELECT" ? "change" : "input"));
  }
}

/**
 * Decorate one field. `entry` is its row from `config.get`; `onReset` is
 * called with the key when Default is pressed.
 */
export function decorate(client, key, wrap, entry, onReset) {
  const error = el("span.err.field-error", { role: "alert", hidden: true });
  const status = el("span.hint.field-status", { role: "status" });
  const reset = el("button.btn.sm", {
    text: "Default",
    title: `Put ${key} back to its default`,
    hidden: !entry || entry.source !== "file",
    onclick: () => onReset(key),
  });
  const tools = el("div.row", {}, [reset]);
  if (entry && entry.secret) tools.prepend(...tokenTools(wrap, entry));
  if (key === "media.dir") tools.prepend(folderTool(client, wrap));
  wrap.append(tools, error, status);
  return {
    error(text) {
      error.textContent = text || "";
      error.hidden = !text;
    },
    status(text) {
      status.textContent = text || "";
    },
    written(yes) {
      reset.hidden = !yes;
    },
  };
}

function tokenTools(wrap, entry) {
  const said = entry.set
    ? "A token is set. The mixer never shows it again: to change it, type a new one or generate one, then Save."
    : "No token is set, so anyone who can reach the control port can use this mixer.";
  const generate = el("button.btn.sm", {
    text: "Generate a new token",
    onclick: () => {
      const input = wrap.querySelector("input");
      if (input) input.type = "text";
      putValue(wrap, newToken());
    },
  });
  return [el("span.hint", { text: said }), generate];
}

function folderTool(client, wrap) {
  return el("button.btn.sm", {
    text: "Choose a folder",
    onclick: async () => {
      const { pickFolder } = await import("./folder-picker.js");
      const input = wrap.querySelector("input");
      const chosen = await pickFolder(client, { start: input && input.value, title: "The media folder" });
      if (chosen) putValue(wrap, chosen);
    },
  });
}
