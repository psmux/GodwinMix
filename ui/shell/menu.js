// The right click menu, and the colour swatches in it.
//
// Same commands as the palette, because 3b says so: right click is for the
// person who does not know the shortcut, Ctrl+K is for the person who does, and
// neither should offer something the other does not.

import { el, on } from "./dom.js";

export const SWATCHES = [
  ["var(--kind-camera)", "Blue"],
  ["var(--kind-page)", "Green"],
  ["var(--kind-file)", "Orange"],
  ["var(--kind-stream)", "Purple"],
  ["var(--kind-graphic)", "Pink"],
  ["var(--kind-other)", "Grey"],
];

let openMenu = null;

/**
 * @param {number} x
 * @param {number} y
 * @param {Array<{label, run, key?, disabled?, kind?: "separator"|"colours", onColour?}>} items
 */
export function contextMenu(x, y, items) {
  closeMenu();
  const menu = el("div.menu", { role: "menu" });
  for (const item of items) {
    if (!item) continue;
    if (item.kind === "separator") {
      menu.appendChild(el("hr"));
      continue;
    }
    if (item.kind === "colours") {
      const row = el("div.swatches");
      for (const [colour, name] of SWATCHES) {
        row.appendChild(
          el("button", {
            title: name,
            "aria-label": name,
            style: { background: colour },
            onclick: () => {
              closeMenu();
              item.onColour(colour);
            },
          })
        );
      }
      menu.appendChild(row);
      continue;
    }
    menu.appendChild(
      el("button", { role: "menuitem", disabled: !!item.disabled, onclick: () => {
        closeMenu();
        item.run();
      } }, [el("span", { text: item.label }), item.key ? el("span.key", { text: item.key }) : null])
    );
  }

  document.body.appendChild(menu);
  // Placed after measuring, so a menu near the right edge folds back onto the
  // screen rather than off it.
  const r = menu.getBoundingClientRect();
  menu.style.left = Math.min(x, window.innerWidth - r.width - 6) + "px";
  menu.style.top = Math.min(y, window.innerHeight - r.height - 6) + "px";

  const offs = [
    on(window, "pointerdown", (e) => {
      if (!menu.contains(e.target)) closeMenu();
    }, true),
    on(window, "keydown", (e) => {
      if (e.key === "Escape") closeMenu();
    }, true),
    on(window, "blur", closeMenu),
    on(window, "resize", closeMenu),
  ];
  openMenu = { menu, offs };
  const first = menu.querySelector("button:not([disabled])");
  if (first) first.focus();
  return menu;
}

export function closeMenu() {
  if (!openMenu) return;
  for (const off of openMenu.offs) off();
  openMenu.menu.remove();
  openMenu = null;
}
