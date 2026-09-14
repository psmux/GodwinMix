// The panel registry, and the two tiers a panel can run in.
//
// Trusted: a custom element in the page, handed the real client. Sandboxed: an
// HTML file in an <iframe sandbox="allow-scripts"> talking the same JSON-RPC
// over postMessage, never with allow-same-origin. The contract and a worked
// example are in docs/how-to/write-a-panel.md.
//
// `customElements.define` is optional: a class that only pushes itself onto
// window.godwinmixPanels is defined here under a tag derived from its id. That
// is one less thing for a first panel to get wrong.

import { el } from "./dom.js";
import { SandboxHost } from "./sandbox.js";

/** id -> {id, title, slots, tier, Class?, src?, plugin?} */
const panels = new Map();
const listeners = new Set();

/** Panels announce themselves by pushing onto this array, at any time. */
export function installGlobal() {
  const existing = Array.isArray(window.godwinmixPanels) ? window.godwinmixPanels : [];
  const arr = [];
  arr.push = function (...classes) {
    for (const C of classes) {
      Array.prototype.push.call(this, C);
      try {
        registerElement(C);
      } catch (e) {
        console.error("a panel refused to register", e);
      }
    }
    return this.length;
  };
  window.godwinmixPanels = arr;
  for (const C of existing) arr.push(C);
  return arr;
}

export function onPanelsChanged(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

function changed() {
  for (const fn of listeners) fn(list());
}

export function list() {
  return [...panels.values()];
}

export function get(id) {
  return panels.get(id) || null;
}

/** A trusted panel: a custom element class with `static get panel()`. */
export function registerElement(Class, plugin) {
  const spec = Class.panel;
  if (!spec || !spec.id) throw new Error("a panel class needs `static get panel() { return {id, title, slots} }`");
  const tag = spec.tag || tagFor(spec.id);
  if (!customElements.get(tag)) customElements.define(tag, Class);
  panels.set(spec.id, {
    id: spec.id,
    title: spec.title || spec.id,
    slots: spec.slots && spec.slots.length ? spec.slots : ["sidebar"],
    tier: "trusted",
    tag,
    Class,
    plugin: plugin || "first party",
  });
  changed();
  return spec.id;
}

/** A sandboxed panel: an HTML file loaded into an iframe with no same origin. */
export function registerSandboxed(spec) {
  if (!spec || !spec.id || !spec.src) throw new Error("a sandboxed panel needs an id and a src");
  panels.set(spec.id, {
    id: spec.id,
    title: spec.title || spec.id,
    slots: spec.slots && spec.slots.length ? spec.slots : ["sidebar"],
    tier: "sandboxed",
    src: spec.src,
    height: spec.height || 260,
    plugin: spec.plugin || "plugin",
  });
  changed();
  return spec.id;
}

/** "ndi/senders" becomes "gmx-ndi-senders". Legible, and a valid tag name. */
export function tagFor(id) {
  const body = String(id)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return body.startsWith("gmx-") ? body : "gmx-" + (body || "panel");
}

/**
 * Build a live instance of one panel, in whichever tier it registered for.
 * The shell calls this; a panel author never does.
 */
export function instantiate(id, client, config) {
  const spec = panels.get(id);
  if (!spec) return null;
  if (spec.tier === "trusted") {
    const node = document.createElement(spec.tag);
    if (typeof node.setClient === "function") node.setClient(client);
    if (typeof node.setConfig === "function") node.setConfig(config || {});
    node.dataset.panel = id;
    return { node, destroy: () => node.remove() };
  }
  const frame = el("iframe.panel", {
    src: spec.src,
    // No allow-same-origin, ever. With it the frame could reach this page's
    // storage and its token, and the sandbox would be decoration.
    sandbox: "allow-scripts",
    title: spec.title,
    height: String(spec.height),
    loading: "lazy",
    referrerpolicy: "no-referrer",
  });
  frame.dataset.panel = id;
  const host = new SandboxHost(frame, client, config || {}, spec);
  return { node: frame, destroy: () => host.destroy() };
}

/**
 * Discover panels the mixer is serving.
 *
 * `/plugins/index.json` is written by the core from what is actually on disk,
 * so a file dropped into ~/.godwinmix/plugins/x/ui/ appears with no manifest,
 * no restart of this page's code, and no edit to the shell.
 */
export async function discover(base) {
  let index;
  try {
    const res = await fetch(new URL("/plugins/index.json", base || location.origin));
    if (!res.ok) return [];
    index = await res.json();
  } catch {
    return [];
  }
  const found = [];
  for (const p of index.plugins || []) {
    try {
      if (p.tier === "trusted" && p.module) {
        // The module registers itself by pushing onto window.godwinmixPanels.
        await import(/* @vite-ignore */ p.module);
        found.push(p.name);
      } else if (p.page) {
        registerSandboxed({
          id: p.id || `${p.name}/panel`,
          title: p.title || p.name,
          slots: p.slots,
          src: p.page,
          height: p.height,
          plugin: p.name,
        });
        found.push(p.name);
      }
    } catch (e) {
      console.error(`plugin panel '${p.name}' did not load`, e);
    }
  }
  return found;
}
