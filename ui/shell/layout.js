// The layout: which panels sit in which slot.
//
// JSON, `{slot: [panel ids]}`, saved in localStorage for now. When the core
// grows a per surface store the only change here is where `load` and `save`
// read and write; every caller stays as it is. A preset ships a default layout
// by handing one to `setDefault` before the shell mounts.

const KEY = "gmx.layout";

export const SLOTS = ["header", "monitor", "main", "sidebar", "strip", "footer", "modal"];

let fallback = {
  header: ["core/header"],
  monitor: ["core/program"],
  main: ["core/sources", "core/scenes", "core/outputs", "core/media", "core/alerts"],
  sidebar: [],
  strip: [],
  footer: [],
  modal: [],
};

/** A preset calls this before `mountShell` to ship its own arrangement. */
export function setDefault(layout) {
  fallback = normalise(layout);
}

export function defaultLayout() {
  return normalise(fallback);
}

export function load() {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return defaultLayout();
    return normalise(JSON.parse(raw));
  } catch {
    return defaultLayout();
  }
}

export function save(layout) {
  try {
    localStorage.setItem(KEY, JSON.stringify(normalise(layout)));
  } catch {
    /* the arrangement lasts the session */
  }
}

export function reset() {
  try {
    localStorage.removeItem(KEY);
  } catch {
    /* nothing stored, nothing to remove */
  }
  return defaultLayout();
}

/** Every slot present, every entry a string, no duplicates, no unknown slots. */
function normalise(layout) {
  const out = {};
  for (const slot of SLOTS) {
    const list = layout && Array.isArray(layout[slot]) ? layout[slot] : [];
    out[slot] = [...new Set(list.filter((x) => typeof x === "string"))];
  }
  // Upgrade saved layouts too: the monitor never shares a scroll container.
  for (const slot of SLOTS) {
    if (slot !== "monitor" && out[slot].includes("core/program")) {
      out[slot] = out[slot].filter((id) => id !== "core/program");
      if (!out.monitor.includes("core/program")) out.monitor.push("core/program");
    }
  }
  for (const id of ["core/outputs", "core/media", "core/alerts"]) {
    if (out.footer.includes(id)) {
      out.footer = out.footer.filter((panel) => panel !== id);
      if (!out.main.includes(id)) out.main.push(id);
    }
  }
  return out;
}

/** Put a panel in a slot, keeping the rest. Used when a plugin declares one. */
export function place(layout, slot, panelId) {
  const next = normalise(layout);
  if (!SLOTS.includes(slot)) return next;
  if (!next[slot].includes(panelId)) next[slot].push(panelId);
  return next;
}

export function remove(layout, panelId) {
  const next = normalise(layout);
  for (const slot of SLOTS) next[slot] = next[slot].filter((id) => id !== panelId);
  return next;
}
