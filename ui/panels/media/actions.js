// What the Media panel asks the mixer to do with files and folders. Each
// move that the mixer refuses because a file is on programme asks the
// person once, then tries again with force.

import { errorToast, toast } from "../../shell/toast.js";
import { confirmModal } from "../../shell/modal.js";
import { nameOf, folderLabel } from "./model.js";

/** The dialogs, fetched the first time one is wanted. */
export const dialogs = () => import("./edit.js");

async function withForce(client, method, params, what) {
  try {
    return await client.call(method, params);
  } catch (e) {
    const live = e && e.data && e.data.sources;
    if (!live || !live.length) throw e;
    const ok = await confirmModal(`${live.join(", ")} is on air now and plays ${what}. Move it anyway? It starts again on air.`, "Move anyway");
    if (!ok) return null;
    return client.call(method, { ...params, force: true });
  }
}

/** Move files into a folder ("" is the top of the library). */
export async function moveFiles(client, names, folder) {
  const what = names.length > 1 ? "from these files" : nameOf(names[0]);
  try {
    const done = await withForce(client, "media.move", { names, folder }, what);
    if (done) toast({ text: `${names.length > 1 ? `${names.length} files` : nameOf(names[0])} moved to ${folderLabel(folder)}.` });
  } catch (e) {
    errorToast(e, "Move");
  }
}

export async function renameFile(client, name, newName) {
  try {
    await withForce(client, "media.move", { name, new_name: newName }, nameOf(name));
  } catch (e) {
    errorToast(e, "Rename");
  }
}

/** Move a folder into another, rename it, or both. */
export async function moveFolder(client, path, into, to) {
  const params = { name: path };
  if (into !== undefined) params.into = into;
  if (to !== undefined) params.to = to;
  try {
    await withForce(client, "media.folder.move", params, `from ${nameOf(path)}`);
  } catch (e) {
    errorToast(e, "Move folder");
  }
}

export async function makeFolder(client, path) {
  try {
    const made = await client.call("media.folder.add", { name: path });
    return made.path;
  } catch (e) {
    errorToast(e, "New folder");
    return null;
  }
}

export async function removeFolder(client, path, contents) {
  try {
    await withForce(client, "media.folder.remove", { name: path, contents }, `from ${nameOf(path)}`);
    toast({ text: `${nameOf(path)} removed. Nothing in it was deleted.` });
  } catch (e) {
    errorToast(e, "Remove folder");
  }
}

/** Add tags to files, or take them off. */
export async function tagFiles(client, names, change) {
  try {
    await client.call("media.tag", { names, ...change });
  } catch (e) {
    errorToast(e, "Tags");
  }
}

export function addAsSource(client, item) {
  client
    .call("source.add", { uri: item.converted_path || item.path, name: item.name })
    .then(() => toast({ text: `${item.name} added.` }))
    .catch((e) => errorToast(e, "Add as source"));
}
