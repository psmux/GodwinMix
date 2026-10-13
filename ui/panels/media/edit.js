// The Media panel's dialogs: tags, Move to, a name, and removing a folder.
// Fetched the first time one opens; the page does not carry them.

import { el, clear } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { tree, typedTags, canMoveFolder, folderLabel, parentOf, nameOf } from "./model.js";
import * as act from "./actions.js";

/** One line of text, or null when the person cancels. */
export function askName(title, value, okLabel) {
  return new Promise((resolve) => {
    const input = el("input", { type: "text", value: value || "", "aria-label": title, style: { width: "100%" } });
    let done = false;
    const finish = (v) => {
      done = true;
      m.close();
      resolve(v);
    };
    input.addEventListener("keydown", (e) => e.key === "Enter" && input.value.trim() && finish(input.value.trim()));
    const m = modal({
      title,
      body: input,
      footer: [el("button.btn", { text: "Cancel", onclick: () => finish(null) }), el("button.btn.primary", { text: okLabel || "OK", onclick: () => input.value.trim() && finish(input.value.trim()) })],
      onClose: () => done || resolve(null),
    });
    input.select();
  });
}

/** Tags for one file, or tags to add to several. */
export function editTags(client, items, inUse) {
  const one = items.length === 1;
  let tags = one ? [...(items[0].tags || [])] : [];
  const chips = el("div.row.wrap.mx-tags");
  const list = el("datalist#mx-tag-ideas");
  const input = el("input", { type: "text", list: "mx-tag-ideas", placeholder: "Type a tag, then Enter", "aria-label": "New tag", style: { flex: "1" } });
  const paint = () => {
    clear(chips);
    for (const t of tags) {
      chips.appendChild(el("span.pill.mx-tag", {}, [t, el("button.mx-x", { text: "×", title: `Take off ${t}`, "aria-label": `Take off ${t}`, onclick: () => ((tags = tags.filter((x) => x !== t)), paint()) })]));
    }
    if (!tags.length) chips.appendChild(el("span.sm.faint", { text: one ? "No tags yet." : "Tags you add go on every chosen file." }));
    clear(list);
    for (const t of inUse) if (!tags.some((x) => x.toLowerCase() === t.name.toLowerCase())) list.appendChild(el("option", { value: t.name }));
  };
  const take = () => {
    for (const t of typedTags(input.value)) if (!tags.some((x) => x.toLowerCase() === t.toLowerCase())) tags.push(t);
    input.value = "";
    paint();
  };
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      take();
    }
  });
  paint();
  const m = modal({
    title: one ? `Tags for ${nameOf(items[0].name)}` : `Add tags to ${items.length} files`,
    body: el("div.col", { style: { gap: "8px" } }, [chips, el("div.row", {}, [input, el("button.btn", { text: "Add", onclick: take })]), list]),
    footer: [
      el("button.btn", { text: "Cancel", onclick: () => m.close() }),
      el("button.btn.primary", { text: "Save", onclick: async () => {
        take();
        m.close();
        const names = items.map((i) => i.name);
        await act.tagFiles(client, names, one ? { tags } : { add_tags: tags });
      } }),
    ],
  });
  // The box is what a person came to type in, ahead of the close button.
  input.focus();
}

/** Pick a folder: for files (`names`) or for a folder (`folder`). */
export function moveTo(client, folders, what) {
  let chosen = null;
  const box = el("div.col.mx-pick", { role: "listbox", "aria-label": "Folders" });
  const allowed = (path) => (what.folder ? canMoveFolder(what.folder, path) : !what.names.every((n) => parentOf(n) === path));
  const paint = () => {
    clear(box);
    const add = (path, label, depth) => {
      box.appendChild(el("button.btn.mx-pick-row", {
        text: label,
        role: "option",
        disabled: !allowed(path),
        "aria-selected": String(chosen === path),
        style: { paddingLeft: `${8 + depth * 14}px` },
        onclick: () => ((chosen = path), paint()),
      }));
    };
    add("", "Top of the library", 0);
    const walk = (nodes, depth) => nodes.forEach((n) => (add(n.path, n.name, depth), walk(n.children, depth + 1)));
    walk(tree(folders), 1);
  };
  paint();
  const m = modal({
    title: what.folder ? `Move the folder ${nameOf(what.folder)}` : `Move ${what.names.length > 1 ? `${what.names.length} files` : nameOf(what.names[0])}`,
    body: box,
    footer: [
      el("button.btn", { text: "New folder here", onclick: async () => {
        const name = await askName(`New folder in ${folderLabel(chosen || "")}`, "", "Make folder");
        if (!name) return;
        const made = await act.makeFolder(client, chosen ? `${chosen}/${name}` : name);
        if (made) ((folders = [...folders, made]), (chosen = made), paint());
      } }),
      el("span.grow"),
      el("button.btn", { text: "Cancel", onclick: () => m.close() }),
      el("button.btn.primary", { text: "Move here", onclick: () => {
        if (chosen === null || !allowed(chosen)) return;
        m.close();
        if (what.folder) act.moveFolder(client, what.folder, chosen);
        else act.moveFiles(client, what.names, chosen);
      } }),
    ],
  });
}

/** Remove a folder, asking where what is inside goes. */
export function removeFolder(client, path, inside) {
  let contents = "up";
  const up = parentOf(path);
  const choice = (value, text) =>
    el("label.row", {}, [el("input", { type: "radio", name: "mx-contents", value, checked: value === contents, onchange: () => (contents = value) }), el("span", { text })]);
  const m = modal({
    title: `Remove the folder ${nameOf(path)}`,
    body: el("div.col", { style: { gap: "8px" } }, [
      el("p", { style: { margin: 0 }, text: `It holds ${inside}. No file is deleted: they move.` }),
      choice("up", up ? `Move them up into ${folderLabel(up)}` : "Move them up to the top of the library"),
      choice("top", "Move them to the top of the library"),
    ]),
    footer: [
      el("button.btn", { text: "Cancel", onclick: () => m.close() }),
      el("button.btn.primary", { text: "Remove folder", onclick: () => (m.close(), act.removeFolder(client, path, contents)) }),
    ],
  });
}
