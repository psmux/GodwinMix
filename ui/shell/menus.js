// The menus themselves: read from menus.json, drawn under the bar, driven by
// the keyboard the way a desktop menu is. Loaded the first time a menu opens.

import { el, on } from "./dom.js";
import { get as command, run } from "./commands.js";
import { shell } from "./shell.js";
import { focusTitle } from "./menubar.js";
import { list as panels } from "./registry.js";
import { checked, act as doAct } from "./menu-actions.js";

let definition = null;
let opened = null;

export const act = doAct;

async function menusOf() {
  if (!definition) {
    const res = await fetch(new URL("./menus.json", import.meta.url));
    definition = (await res.json()).menus;
  }
  return definition;
}

/** Open a menu, or close it when it is the one already open. A menu the
 * pointer opened on its way to the title a moment ago stays open: the click
 * that follows the hover is the same gesture. */
export async function toggle(bar, id) {
  if (opened && opened.id === id) return performance.now() - opened.since > 400 ? close(true) : undefined;
  return open(bar, id, false);
}

let asked = 0;

export async function open(bar, id, keyboard) {
  // Only the last of two quick asks draws: two menus on screen at once is
  // what a hover and a key pressed together would otherwise leave.
  const mine = ++asked;
  const menu = (await menusOf()).find((m) => m.id === id);
  if (mine !== asked) return;
  const title = bar.querySelector(`[data-menu="${id}"]`);
  if (!menu || !title) return;
  if (opened && opened.id === id) return;
  close(false);
  const node = drop(menu.items, bar, title);
  const r = title.getBoundingClientRect();
  node.style.left = Math.min(r.left, window.innerWidth - node.offsetWidth - 6) + "px";
  node.style.top = r.bottom + 2 + "px";
  title.setAttribute("aria-expanded", "true");
  opened = { id, node, bar, title, since: performance.now(), offs: watch(node, bar) };
  if (keyboard) first(node);
}

/** At phone width: every menu in one list, each under its heading. */
export async function openAll(bar, button) {
  if (opened && opened.id === "*") return close(true);
  const mine = ++asked;
  const menus = await menusOf();
  if (mine !== asked) return;
  close(false);
  const node = el("div.menu.menubar-drop.menubar-every", { role: "menu", "aria-label": "Menu" });
  for (const menu of menus) {
    node.append(el("div.menubar-heading", { text: menu.title, role: "presentation" }));
    for (const b of items(menu.items, bar)) node.append(b);
  }
  document.body.append(node);
  const r = button.getBoundingClientRect();
  node.style.left = "8px";
  node.style.top = r.bottom + 2 + "px";
  // A phone's header wraps, so the room under it is measured, not assumed,
  // and the home bar at the foot of a notched phone is left clear.
  node.style.maxHeight = `calc(100dvh - ${Math.round(r.bottom) + 10}px - env(safe-area-inset-bottom))`;
  opened = { id: "*", node, bar, title: button, since: performance.now(), offs: watch(node, bar) };
  first(node);
}

function drop(list, bar, title) {
  const node = el("div.menu.menubar-drop", { role: "menu", "aria-label": title.textContent });
  node.append(...items(list, bar));
  document.body.append(node);
  return node;
}

/** The buttons for one menu's items, plugin panels folded in where it says. */
function items(list, bar) {
  const out = [];
  const expanded = list.flatMap((it) => (it.plugins ? pluginPanels() : [it]));
  for (const it of expanded) {
    if (it.separator) {
      out.push(el("hr", { role: "separator" }));
      continue;
    }
    const cmd = command(it.command);
    const usable = cmd ? !(cmd.enabled && !cmd.enabled()) : !!it.panel;
    const tick = it.check ? checked(it) : null;
    const key = it.key || shell.keymap.keyFor(it.command) || (cmd && cmd.key) || "";
    out.push(el("button", {
      role: tick === null ? "menuitem" : "menuitemcheckbox",
      "aria-checked": tick === null ? undefined : String(tick),
      disabled: !usable,
      tabindex: "-1",
      onclick: () => choose(it, bar),
    }, [el("span.menubar-tick", { text: tick ? "✓" : "" }), el("span.grow", { text: it.title }), key ? el("span.key", { text: key }) : null]));
  }
  return out;
}

function pluginPanels() {
  return panels()
    .filter((p) => !p.id.startsWith("core/") && !p.slots.includes("header") && !p.slots.includes("modal"))
    .map((p) => ({ command: "view.panel", arg: p.id, title: p.title, check: "panel" }));
}

async function choose(it, bar) {
  close(false);
  const title = bar.querySelector("[data-menu][tabindex='0']");
  if (title) title.blur();
  if (!command(it.command) && it.panel) await doAct(shell.client, "view.show", it.panel);
  await run(it.command, it.arg);
}

function first(node) {
  const b = node.querySelector("button:not([disabled])");
  if (b) b.focus();
}

/** Up and down inside, left and right to the next menu, Escape back to the bar. */
function watch(node, bar) {
  const key = (e) => {
    // A key pressed in a menu is the menu's. Enter and Space on the page's
    // own map take and open things, and must not do that from in here.
    e.stopPropagation();
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (node.contains(document.activeElement)) document.activeElement.click();
      return;
    }
    const buttons = [...node.querySelectorAll("button:not([disabled])")];
    const at = buttons.indexOf(document.activeElement);
    const move = { ArrowDown: 1, ArrowUp: -1 }[e.key];
    if (move) {
      e.preventDefault();
      buttons[(at + move + buttons.length) % buttons.length]?.focus();
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      buttons[e.key === "Home" ? 0 : buttons.length - 1]?.focus();
    } else if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && opened && opened.id !== "*") {
      e.preventDefault();
      const titles = [...bar.querySelectorAll("[data-menu]")];
      const next = titles[(titles.indexOf(opened.title) + (e.key === "ArrowLeft" ? -1 : 1) + titles.length) % titles.length];
      focusTitle(bar, next);
      open(bar, next.dataset.menu, true);
    } else if (e.key === "Escape") {
      e.preventDefault();
      close(true);
    } else if (e.key === "Tab") {
      close(false);
    }
  };
  return [
    on(node, "keydown", key),
    on(window, "pointerdown", (e) => {
      if (!node.contains(e.target) && !bar.parentNode.contains(e.target)) close(false);
    }, true),
    on(window, "resize", () => close(false)),
    on(window, "blur", () => close(false)),
  ];
}

export function close(refocus) {
  if (!opened) return;
  const { node, title, offs } = opened;
  opened = null;
  offs.forEach((off) => off());
  node.remove();
  title.setAttribute("aria-expanded", "false");
  if (refocus) title.focus();
}
