// After an install: the plugin is ready with no other step. The core has
// already started what runs on its own; this side finds the panels it brings
// (read again from /plugins/index.json, which the core writes from disk) and
// opens the first. Open, on a card, does the same thing later: it brings the
// panel forward, or opens the add source picker on a source plugin's form.

import * as registry from "../../shell/registry.js";

/** The page's workspace, or null on a page with none, such as a test. */
const workspace = () => document.querySelector("gmx-shell")?.workspace || null;

/** Bring one panel up, in the dock or on a phone. False when there is nowhere. */
export function showPanel(id) {
  const ws = workspace();
  if (!ws || typeof ws.show !== "function") return false;
  ws.show(id);
  return true;
}

/**
 * Read the plugin panels again and open the first one this plugin brought.
 * Answers the ids of its panels, so the card can say what appeared.
 */
export async function arrive(p, discover = registry.discover) {
  await discover(location.origin);
  const ids = registry.panelsOf(p.name).map((s) => s.id);
  if (ids.length) showPanel(ids[0]);
  return ids;
}

/** Whether a plugin has something Open can show. */
export function canOpen(p) {
  if (!p.installed) return false;
  return registry.panelsOf(p.name).length > 0 || (p.kinds || []).includes("source");
}

/**
 * What Open does for an installed plugin.
 *
 * @param {() => void} leave  closes the gallery, so what opens can be seen
 */
export async function openPlugin(client, p, leave) {
  const panel = registry.panelsOf(p.name)[0];
  if (panel) {
    leave();
    return showPanel(panel.id);
  }
  if (!(p.kinds || []).includes("source")) return false;
  leave();
  const kind = await sourceKind(client, p.name);
  const { openPicker } = await import("../../shell/picker-loader.js");
  openPicker(client, "source", kind ? { kind } : {});
  return true;
}

/** The first source a plugin provides, as the picker names its kind. */
export async function sourceKind(client, name) {
  const list = await client.call("plugin.list", {}).catch(() => null);
  const mine = ((list && list.plugins) || []).find((q) => q.name === name);
  if (!mine) return "";
  const kinds = mine.provide_kinds || {};
  return (mine.provides || []).find((id) => kinds[id] === "source" || (!kinds[id] && id.endsWith("/source"))) || "";
}
