// The media library's folders and tags as plain data: no DOM here, so the
// test page can check every rule on its own.
//
// A folder is a path in the library, "Sermons/2026", as media.list gives
// it. A file's own path is its name, "Sermons/2026/easter.mp4".

export const parentOf = (path) => (path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "");
export const nameOf = (path) => path.slice(path.lastIndexOf("/") + 1);
const low = (s) => String(s || "").toLowerCase();

/** Folders as a tree: [{path, name, children}], each level by name. */
export function tree(folders) {
  const nodes = new Map();
  const top = [];
  for (const path of [...folders].sort((a, b) => low(a).localeCompare(low(b)))) {
    const node = { path, name: nameOf(path), children: [] };
    nodes.set(path, node);
    const parent = nodes.get(parentOf(path));
    (parent ? parent.children : top).push(node);
  }
  return top;
}

/** Whether a file at `name` is in `folder`, or under it with `deep`. */
export function inFolder(name, folder, deep) {
  if (folder === null || folder === undefined) return true;
  const here = low(parentOf(name));
  const f = low(folder);
  return here === f || (deep && (f === "" || here.startsWith(f + "/")));
}

/**
 * The items a person asked to see. `folder` null is every file; `tags` all
 * have to match; every word of `query` has to be in the path or a tag.
 */
export function visible(items, { folder = null, deep = false, tags = [], query = "" } = {}) {
  const words = low(query).split(/\s+/).filter(Boolean);
  return items.filter((i) => {
    const have = (i.tags || []).map(low);
    const text = `${low(i.name)} ${have.join(" ")}`;
    return inFolder(i.name, folder, deep) && tags.every((t) => have.includes(low(t))) && words.every((w) => text.includes(w));
  });
}

/** Files right in each folder and anywhere under it: Map path -> {files, total}. */
export function counts(items, folders) {
  const out = new Map(folders.map((f) => [f, { files: 0, total: 0 }]));
  for (const i of items) {
    let at = parentOf(i.name);
    if (out.has(at)) out.get(at).files += 1;
    while (at) {
      if (out.has(at)) out.get(at).total += 1;
      at = parentOf(at);
    }
  }
  return out;
}

/** Every tag in use, the most used first, in the case first typed. */
export function tagsInUse(items) {
  const seen = new Map();
  for (const i of items) {
    for (const t of i.tags || []) {
      const k = low(t);
      if (seen.has(k)) seen.get(k).count += 1;
      else seen.set(k, { name: t, count: 1 });
    }
  }
  return [...seen.values()].sort((a, b) => b.count - a.count || low(a.name).localeCompare(low(b.name)));
}

/** Tags typed into a box: split at commas, trimmed, unique without case. */
export function typedTags(text) {
  const out = [];
  for (const raw of String(text || "").split(/[,;]/)) {
    const t = raw.trim().replace(/\s+/g, " ");
    if (t && !out.some((o) => low(o) === low(t))) out.push(t);
  }
  return out;
}

/**
 * Whether a folder may go into `into`: never into itself or one of its own
 * folders, and never to where it already is.
 */
export function canMoveFolder(folder, into) {
  if (into === folder || into.startsWith(folder + "/")) return false;
  return parentOf(folder) !== into;
}

/** Where a file goes when it is dropped: nowhere new is null. */
export function fileTarget(name, into) {
  return parentOf(name) === into ? null : into;
}

/** Words for a folder, the top included. */
export function folderLabel(path) {
  return path ? path.split("/").join(" / ") : "Top of the library";
}
