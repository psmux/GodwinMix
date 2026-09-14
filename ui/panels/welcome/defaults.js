// What the core says a surface should start with, and how it gets applied.
//
// `core.info` carries a `ui` object when a preset has been applied: the theme,
// the gallery mode and the layout that preset chose. A preset's own theme is a
// stylesheet inside the preset, served at /presets/<name>/theme.css, which is
// registered with the theme loader under the name the preset gave it.
//
// The operator always wins. A theme or a gallery mode this browser has already
// been told to use is not overwritten by a preset; the preset chooses the first
// value, and Settings changes it from there.

import { addTheme, applyTheme } from "../../shell/theme.js";
import { setSetting, proposeGalleryMode } from "../../shell/settings.js";
import * as layout from "../../shell/layout.js";

const CHOSEN = "gmx.ui.from";

/**
 * A preset names panels by the short name `presets/README.md` documents; the
 * reference UI registers them under `core/<name>`. Anything with a slash in it
 * is a plugin's panel and passes through untouched.
 */
const PANEL_IDS = {
  header: "core/header",
  multiview: "core/program",
  sources: "core/sources",
  scenes: "core/scenes",
  outputs: "core/outputs",
  media: "core/media",
  alerts: "core/alerts",
  welcome: "core/welcome",
};

function resolvePanels(layout) {
  const out = {};
  for (const [slot, panels] of Object.entries(layout || {})) {
    out[slot] = (panels || []).map((p) => (p.includes("/") ? p : PANEL_IDS[p] || p));
  }
  return out;
}

/** What the core last told us, so the welcome panel and Settings agree. */
let current = null;

export function coreUi() {
  return current;
}

/**
 * Read `core.info` and apply what it says. Called once at boot, and again
 * whenever `event/ui.changed` arrives.
 *
 * Returns the info object, or null on a core too old to have one.
 */
export async function applyCoreDefaults(client, opts = {}) {
  let info = null;
  try {
    info = await client.call("core.info", {});
  } catch {
    return null;
  }
  current = info && info.ui ? info.ui : null;
  apply(current, opts.force === true);
  return info;
}

/** Follow the core when a preset is applied from this page or another one. */
export function watchCoreDefaults(client) {
  return client.on("event", (e) => {
    if (!e || e.name !== "ui.changed") return;
    current = e.params && e.params.ui ? e.params.ui : null;
    // A preset the operator just chose is a deliberate choice, so it wins over
    // whatever this browser had remembered.
    apply(current, true);
  });
}

function apply(ui, force) {
  if (!ui) {
    // No preset. The machine proposes the gallery mode, as it always has.
    const proposed = proposeGalleryMode();
    if (proposed) setSetting("gallery", proposed);
    return;
  }
  const already = remembered();
  if (ui.theme && (force || already !== ui.preset)) {
    register(ui);
    applyTheme(ui.theme);
  }
  if (ui.gallery && (force || already !== ui.preset)) {
    setSetting("gallery", ui.gallery);
  }
  if (ui.layout && Object.keys(ui.layout).length && (force || already !== ui.preset)) {
    layout.save(resolvePanels(ui.layout));
  }
  remember(ui.preset);
}

/**
 * A preset's own stylesheet, if the theme is not one the page carries.
 *
 * `/presets/<name>/theme.css` is served by the core out of the preset it was
 * applied from, so a theme travels with the preset and needs no rebuild.
 */
function register(ui) {
  if (!ui.preset || !ui.theme) return;
  const title = ui.theme.charAt(0).toUpperCase() + ui.theme.slice(1).replace(/-/g, " ");
  addTheme({ id: ui.theme, title, href: `presets/${ui.preset}/theme.css` });
}

function remembered() {
  try {
    return localStorage.getItem(CHOSEN);
  } catch {
    return null;
  }
}

function remember(preset) {
  try {
    if (preset) localStorage.setItem(CHOSEN, preset);
  } catch {
    /* the choice lasts the session */
  }
}

/** Settings' "Show this again" forgets which preset this browser followed. */
export function forgetPreset() {
  try {
    localStorage.removeItem(CHOSEN);
  } catch {
    /* nothing was remembered */
  }
}
