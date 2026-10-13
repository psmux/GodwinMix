// The right click and long press menus of the Media panel, for a file and
// for a folder. Everything here is also somewhere a finger can reach.

import { contextMenu } from "../../shell/menu.js";
import { nameOf, parentOf, counts, tagsInUse } from "./model.js";
import * as act from "./actions.js";
import { remove } from "./row.js";

/** host: { client, items, folders, chosen: Set } */
export function fileMenu(host, item, x, y) {
  const many = host.chosen.has(item.name) && host.chosen.size > 1;
  const names = many ? [...host.chosen] : [item.name];
  const targets = host.items.filter((i) => names.includes(i.name));
  const tags = () => act.dialogs().then((d) => d.editTags(host.client, targets, inUse(host)));
  const move = () => act.dialogs().then((d) => d.moveTo(host.client, host.folders, { names }));
  contextMenu(x, y, [
    { label: many ? `Tags for ${names.length} files` : "Tags", run: tags },
    { label: many ? `Move ${names.length} files to` : "Move to", run: move },
    many ? null : { label: "Rename", run: () => rename(host, item) },
    { kind: "separator" },
    many ? null : { label: "Add as source", run: () => act.addAsSource(host.client, item) },
    many ? null : { label: "Delete", run: () => remove(host.client, item) },
  ]);
}

async function rename(host, item) {
  const d = await act.dialogs();
  const name = await d.askName(`Rename ${nameOf(item.name)}`, nameOf(item.name), "Rename");
  if (name && name !== nameOf(item.name)) act.renameFile(host.client, item.name, name);
}

export function folderMenu(host, path, x, y) {
  const n = counts(host.items, host.folders).get(path) || { total: 0 };
  const subs = host.folders.filter((f) => f.startsWith(path + "/")).length;
  const inside = `${n.total} ${n.total === 1 ? "file" : "files"} and ${subs} ${subs === 1 ? "folder" : "folders"}`;
  contextMenu(x, y, [
    { label: "New folder inside", run: () => newFolder(host, path) },
    { label: "Rename", run: () => renameFolder(host, path) },
    { label: "Move to", run: () => act.dialogs().then((d) => d.moveTo(host.client, host.folders, { folder: path })) },
    { kind: "separator" },
    { label: "Remove folder", run: () => act.dialogs().then((d) => d.removeFolder(host.client, path, inside)) },
  ]);
}

export async function newFolder(host, parent) {
  const d = await act.dialogs();
  const name = await d.askName(parent ? `New folder in ${nameOf(parent)}` : "New folder", "", "Make folder");
  if (!name) return null;
  return act.makeFolder(host.client, parent ? `${parent}/${name}` : name);
}

async function renameFolder(host, path) {
  const d = await act.dialogs();
  const name = await d.askName(`Rename ${nameOf(path)}`, nameOf(path), "Rename");
  if (name && name !== nameOf(path)) act.moveFolder(host.client, path, parentOf(path), name);
}

/** Every tag on a file in the library, the most used first. */
export const inUse = (host) => tagsInUse(host.items);
