// A folder on the mixer, chosen by walking it rather than typing it.
//
// The page may be in another room from the mixer, so a browser's own folder
// dialog would show the wrong machine. This walks `path.list`, which shows
// only folders under the mixer's home and its own folders, and makes a new
// one with `path.create`. Loaded with `import()` by whoever needs a folder.

import { el, clear, on } from "./dom.js";
import { modal } from "./modal.js";
import { CODES } from "../client/errors.js";

/**
 * @param {object} client
 * @param {{start?: string, root?: string, title?: string}} opts
 *   `root` names the root a relative `start` is inside, as `path.list` labels it
 * @returns {Promise<string|null>} the chosen folder, or null when cancelled
 */
export function pickFolder(client, opts = {}) {
  return new Promise((resolve) => {
    let current = null;
    let chosen = null;
    const where = el("strong.num", { style: { overflowWrap: "anywhere" } });
    const note = el("p.sm.dim", { role: "status" });
    const roots = el("div.row.wrap");
    const list = el("div.col.folder-list", { style: { maxHeight: "40vh", overflow: "auto" } });
    const name = el("input", { type: "text", placeholder: "New folder name", autocomplete: "off" });
    const make = el("button.btn", { text: "New folder" });
    const up = el("button.btn", { text: "Up one level" });
    const use = el("button.btn.primary", { text: "Use this folder" });

    async function load(path, why) {
      note.textContent = why || "";
      try {
        show(await client.call("path.list", path ? { path } : {}));
      } catch (e) {
        const nearest = e && e.data && e.data.nearest;
        if (e && e.code === CODES.NOT_FOUND && nearest && nearest !== path) {
          return load(nearest, `${path} does not exist yet. This is the nearest folder that does; make it here with New folder.`);
        }
        if (path && !why) return load(null, e.message || String(e));
        note.textContent = e.message || String(e);
      }
    }

    function show(listing) {
      current = listing;
      where.textContent = listing.path;
      up.disabled = !listing.parent;
      use.disabled = !listing.writable;
      if (!listing.writable) note.textContent = `${note.textContent} The mixer cannot write into this folder. Pick another.`.trim();
      clear(roots);
      for (const r of listing.roots || []) roots.appendChild(el("button.btn.sm", { text: r.label, title: r.path, onclick: () => load(r.path) }));
      clear(list);
      if (!listing.dirs.length) list.appendChild(el("p.sm.dim", { text: "No folders in here." }));
      for (const d of listing.dirs) {
        list.appendChild(el("button.btn.folder", { text: d.name + (d.writable ? "" : " (read only)"), title: d.path, onclick: () => load(d.path) }));
      }
      if (listing.truncated) list.appendChild(el("p.sm.dim", { text: "Only the first 500 folders are shown." }));
    }

    make.onclick = async () => {
      if (!current || !name.value.trim()) return;
      make.disabled = true;
      try {
        show(await client.call("path.create", { parent: current.path, name: name.value.trim() }));
        note.textContent = "Folder made.";
        name.value = "";
      } catch (e) {
        note.textContent = e.message || String(e);
      }
      make.disabled = false;
    };
    on(name, "keydown", (e) => { if (e.key === "Enter") make.click(); });
    up.onclick = () => current && current.parent && load(current.parent);
    use.onclick = () => { chosen = current.path; m.close(); };

    const m = modal({
      title: opts.title || "Choose a folder on the mixer",
      body: el("div.col", {}, [
        el("p.dim", { text: "These are folders on the machine the mixer runs on, not on this device.", style: { marginTop: "0" } }),
        roots,
        el("div.row", {}, [up, where]),
        list,
        el("div.row", {}, [name, make]),
        note,
      ]),
      footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), use],
      onClose: () => resolve(chosen),
    });
    // A relative start (the media folder is "media" in a fresh config) means
    // relative to the mixer's own folder, not home, so it opens at that root.
    const absolute = (p) => /^([a-zA-Z]:)?[\\/]/.test(p || "");
    if (opts.start && !absolute(opts.start) && opts.root) {
      load(null).then(() => {
        const root = current && (current.roots || []).find((r) => r.label === opts.root);
        if (root) load(root.path);
      });
    } else {
      load(opts.start || null);
    }
  });
}
