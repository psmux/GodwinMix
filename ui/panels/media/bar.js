// The line over the Media list: the tags filtering it, the switch for a
// folder's subfolders, and what to do with the files chosen in Select.

import { el, clear } from "../../shell/dom.js";
import { inUse, fileMenu } from "./menus.js";
import { fileTarget, canMoveFolder } from "./model.js";
import * as act from "./actions.js";

export function paintBar(host) {
  clear(host.bar);
  for (const t of host.tags) host.bar.appendChild(el("button.pill.mx-tag", { text: `${t} ×`, title: `Stop filtering by ${t}`, onclick: () => ((host.tags = host.tags.filter((x) => x !== t)), host.render()) }));
  if (host.folder !== null) {
    host.bar.appendChild(el("label.row.sm", {}, [el("input", { type: "checkbox", checked: host.deep, onchange: (e) => ((host.deep = e.target.checked), host.render()) }), el("span", { text: "Include subfolders" })]));
  }
  if (host.picking) {
    const names = [...host.chosen];
    const targets = host.items.filter((i) => host.chosen.has(i.name));
    host.bar.append(
      el("span.grow"),
      el("span.sm", { text: `${names.length} chosen` }),
      el("button.btn", { text: "Add tags", disabled: !names.length, onclick: () => act.dialogs().then((d) => d.editTags(host.client, targets, inUse(host))) }),
      el("button.btn", { text: "Move to", disabled: !names.length, onclick: () => act.dialogs().then((d) => d.moveTo(host.client, host.folders, { names })) })
    );
  }
  host.bar.hidden = !host.bar.childNodes.length;
}

/** What a file row needs from the panel. */
export function rowHost(p) {
  return {
    client: p.client, selected: p.ad.selected, picking: p.picking, chosen: p.chosen,
    select: (item) => (p.ad.choose(item.converted_path || item.path), p.render()),
    roll: () => p.ad.roll(),
    pick: (item, yes) => (yes ? p.chosen.add(item.name) : p.chosen.delete(item.name), p.picking || p.setPicking(true), p.render()),
    tagFilter: (t) => (p.tags.some((x) => x.toLowerCase() === t.toLowerCase()) || p.tags.push(t), p.render()),
    menu: (item, x, y) => fileMenu(p, item, x, y),
  };
}

/** What a drag carries, and where it lands. */
export function carried(p, node) {
  if (node.dataset.dragFolder) return { kind: "folder", path: node.dataset.dragFolder };
  const name = node.dataset.dragMedia;
  return { kind: "files", names: p.chosen.has(name) ? [...p.chosen] : [name] };
}

export function dropped(p, what, into) {
  if (what.kind === "folder") return canMoveFolder(what.path, into) && act.moveFolder(p.client, what.path, into);
  const names = what.names.filter((n) => fileTarget(n, into) !== null);
  if (names.length) act.moveFiles(p.client, names, into);
}
