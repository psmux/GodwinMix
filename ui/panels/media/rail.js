// The folder rail: All files, then the library's folders as a tree that
// opens and closes, each with how many files it holds. Every node is a
// place to drop files and folders on.

import { el, clear } from "../../shell/dom.js";
import { tree, counts } from "./model.js";

const OPEN_KEY = "gmx.mediaOpen";

/** The folders a person left open, kept per browser. */
export function openFolders() {
  try {
    return new Set(JSON.parse(localStorage.getItem(OPEN_KEY) || "[]"));
  } catch {
    return new Set();
  }
}

export function keepOpen(set) {
  try {
    localStorage.setItem(OPEN_KEY, JSON.stringify([...set]));
  } catch {
    /* open folders last the session */
  }
}

/**
 * host: { items, folders, folder (null is All files), deep, open: Set,
 *         select(path), toggle(path), menu(path, x, y) }
 */
export function paintRail(host, rail) {
  clear(rail);
  const n = counts(host.items, host.folders);
  rail.appendChild(node(host, { path: null, name: "All files", children: [] }, 0, host.items.length));
  const walk = (nodes, depth) => {
    for (const f of nodes) {
      const c = n.get(f.path) || { files: 0, total: 0 };
      rail.appendChild(node(host, f, depth, host.deep ? c.total : c.files));
      if (f.children.length && host.open.has(f.path)) walk(f.children, depth + 1);
    }
  };
  walk(tree(host.folders), 1);
}

function node(host, f, depth, count) {
  const chosen = host.folder === f.path;
  const caret = f.children.length
    ? el("button.mx-caret", {
        text: host.open.has(f.path) ? "▾" : "▸",
        title: host.open.has(f.path) ? "Close" : "Open",
        "aria-label": `${host.open.has(f.path) ? "Close" : "Open"} ${f.name}`,
        "aria-expanded": String(host.open.has(f.path)),
        onclick: (e) => {
          e.stopPropagation();
          host.toggle(f.path);
        },
      })
    : el("span.mx-caret");
  const row = el(
    "div.mx-node",
    {
      role: "treeitem",
      tabindex: "0",
      "aria-selected": String(chosen),
      "data-folder": f.path === null ? "" : f.path,
      title: f.path || "Every file in the library",
      style: { paddingLeft: `${4 + depth * 12}px` },
      onclick: () => host.select(f.path),
      onkeydown: (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          host.select(f.path);
        }
      },
      oncontextmenu: (e) => {
        if (f.path === null) return;
        e.preventDefault();
        host.menu(f.path, e.clientX, e.clientY);
      },
    },
    [caret, el("span.ellipsis.grow", { text: f.name }), el("span.num.sm.faint", { text: String(count) })]
  );
  if (f.path !== null) row.dataset.dragFolder = f.path;
  return row;
}
