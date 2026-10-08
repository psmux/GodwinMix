// The menu bar. Only the titles are here; what is in each menu is
// menus.json, read by menus.js when a menu first opens, and the desktop app
// builds its native menu from the same file. Every item is a command from
// the palette's registry, so menu, palette and keys cannot disagree.

import { el } from "./dom.js";
import { registerAll, run } from "./commands.js";

const TITLES = [["file", "File"], ["edit", "Edit"], ["view", "View"], ["sources", "Sources"], ["scenes", "Scenes"], ["outputs", "Outputs"], ["help", "Help"]];

let loading = null;
const menus = () => (loading ||= import("./menus.js"));

/** The bar, and one button that stands for it on a phone. */
export function menubar(client) {
  commands(client);
  const bar = el("nav.menubar", { role: "menubar", "aria-label": "Menu bar" });
  for (const [id, title] of TITLES) {
    bar.append(el("button.menubar-title", { role: "menuitem", "aria-haspopup": "menu", "aria-expanded": "false", tabindex: id === "file" ? "0" : "-1", "data-menu": id, text: title }));
  }
  bar.addEventListener("click", (e) => {
    const title = e.target.closest("[data-menu]");
    if (title) menus().then((m) => m.toggle(bar, title.dataset.menu));
  });
  bar.addEventListener("pointerover", (e) => {
    const title = e.target.closest("[data-menu]");
    if (title && bar.querySelector('[aria-expanded="true"]')) menus().then((m) => m.open(bar, title.dataset.menu));
  });
  bar.addEventListener("keydown", (e) => barKey(bar, e));
  const all = el("button.btn.icon.menubar-all", { text: "☰", title: "Menu (F10)", "aria-label": "Menu", "aria-haspopup": "menu", onclick: () => menus().then((m) => m.openAll(bar, all)) });
  altKeys(bar, all);
  opened();
  return el("div.menubar-wrap", {}, [bar, all]);
}

/** Left and right along the bar, down or Enter into a menu. */
function barKey(bar, e) {
  const titles = [...bar.querySelectorAll("[data-menu]")];
  const at = titles.indexOf(e.target);
  if (at < 0) return;
  const step = { ArrowLeft: -1, ArrowRight: 1 }[e.key];
  // Keys the bar uses are not the page's: Space here must not take a shot.
  if (step || ["ArrowDown", "Enter", " ", "Escape"].includes(e.key)) e.stopPropagation();
  if (step) {
    e.preventDefault();
    const next = titles[(at + step + titles.length) % titles.length];
    focusTitle(bar, next);
    if (bar.querySelector('[aria-expanded="true"]')) menus().then((m) => m.open(bar, next.dataset.menu, true));
  } else if (["ArrowDown", "Enter", " "].includes(e.key)) {
    e.preventDefault();
    menus().then((m) => m.open(bar, e.target.dataset.menu, true));
  } else if (e.key === "Escape") {
    e.target.blur();
  }
}

export function focusTitle(bar, title) {
  for (const t of bar.querySelectorAll("[data-menu]")) t.tabIndex = t === title ? 0 : -1;
  title.focus();
}

/** F10, or Alt pressed and let go on its own, puts the keyboard on the bar. */
function altKeys(bar, all) {
  let alone = false;
  window.addEventListener("keydown", (e) => {
    alone = e.key === "Alt" && !e.repeat;
    if (e.key !== "F10" || e.ctrlKey || e.metaKey) return;
    e.preventDefault();
    enter(bar, all);
  }, true);
  window.addEventListener("keyup", (e) => {
    if (e.key !== "Alt" || !alone) return;
    alone = false;
    e.preventDefault();
    enter(bar, all);
  }, true);
  window.addEventListener("pointerdown", () => { alone = false; }, true);
}

function enter(bar, all) {
  // Not while a dialog is up: it holds the keyboard until it is closed.
  const dialog = [...document.querySelectorAll(".scrim")].some((s) => s.getClientRects().length);
  if (dialog || document.documentElement.hasAttribute("data-native-menu")) return;
  if (bar.offsetParent === null) return all.click();
  const first = bar.querySelector("[data-menu]");
  if (bar.contains(document.activeElement)) document.activeElement.blur();
  else focusTitle(bar, first);
}

/** A project was just opened and the page started again: say what it did. */
function opened() {
  let said = null;
  try {
    said = sessionStorage.getItem("gmx.project.opened");
    sessionStorage.removeItem("gmx.project.opened");
  } catch { /* nothing to say */ }
  if (said) import("./project-open.js").then((m) => m.finished(JSON.parse(said)));
}

let registered = false;

/** The commands the menu adds; their work is in menu-actions.js. */
function commands(client) {
  if (registered) return;
  registered = true;
  const act = (id) => (arg) => menus().then((m) => m.act(client, id, arg));
  const cmd = (id, title, group, key, hidden) => ({ id, title, group, key, hidden, run: act(id) });
  registerAll([
    cmd("project.new", "New project", "Project"),
    cmd("project.open", "Open project", "Project", "Ctrl+O"),
    cmd("project.save", "Save project as", "Project", "Ctrl+S"),
    cmd("show.new", "New show", "Show"),
    cmd("show.add-many", "New shows from a list", "Show"),
    cmd("show.switch", "Switch show", "Show"),
    cmd("view.routing", "Routing", "View"),
    cmd("view.wall", "Monitoring wall", "View"),
    cmd("edit.delete", "Delete the selection", "Edit"),
    cmd("view.panel", "Show or hide a panel", "View", "", true),
    cmd("view.studio", "Studio mode on or off", "View"),
    cmd("view.reset-layout", "Reset the layout", "View"),
    cmd("view.theme", "Choose a theme", "View", "", true),
    cmd("source.add-kind", "Add a source of one kind", "Sources", "", true),
    cmd("output.record", "Start recording", "Outputs"),
    cmd("output.stop-recording", "Stop recording", "Outputs"),
    cmd("output.resources", "Show resources", "Outputs"),
    cmd("help.devices", "Open on another device", "Help"),
    cmd("help.agents", "Connect an AI agent", "Help"),
    cmd("help.docs", "Documentation", "Help"),
    cmd("help.about", "About GodwinMix", "Help"),
  ]);
  // The desktop app's native menu runs these in the window: its one way in.
  window.gmxMenu = {
    run: (id, arg) => run(id, arg),
    native: (on) => document.documentElement.toggleAttribute("data-native-menu", !!on),
  };
}
