// What a card's buttons do, each one public method: the page does nothing
// here an agent could not do with the same calls.

import { el } from "../../shell/dom.js";
import { modal, confirmModal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { uploadUrl } from "./model.js";

/** Add it to the scene on air, hidden. Answers what `gallery.place` said. */
export async function place(client, item) {
  const placed = await client.call("gallery.place", { id: item.id });
  toast({ text: placed.new_scene ? `${item.name} is a new scene, ${placed.scene}.` : `${item.name} is on ${placed.scene}, hidden. Take live shows it.` });
  return placed;
}

/** On air, or off. */
export async function show(client, item, visible) {
  const shown = await client.call("gallery.show", { id: item.id, visible });
  toast({ text: visible ? `${item.name} is on air${shown.took ? `, on ${shown.scene}` : ""}.` : `${item.name} is off air.` });
  return shown;
}

export async function duplicate(client, item) {
  const saved = await client.call("gallery.duplicate", { id: item.id });
  toast({ text: `Copied as ${saved.item.name}.` });
  return saved.item;
}

export async function remove(client, item) {
  if (!(await confirmModal(`Delete ${item.name} (${item.id}) and its files?`, "Delete"))) return false;
  await client.call("gallery.remove", { id: item.id });
  toast({ text: `${item.name} deleted.` });
  return true;
}

/** Write a zip on the mixer and download it. */
export async function exportItems(client, ids) {
  const made = await client.call("gallery.export", ids && ids.length ? { ids } : {});
  if (!made.url) {
    toast({ text: `Written to ${made.path} on the mixer.` });
    return made;
  }
  const tr = client.transport || {};
  const u = new URL(made.url, tr.base || location.origin);
  if (tr.token) u.searchParams.set("token", tr.token);
  const a = el("a", { href: u.toString(), download: made.path.split(/[\\/]/).pop() });
  document.body.appendChild(a);
  a.click();
  a.remove();
  toast({ text: `Exported ${made.ids.length} graphic${made.ids.length === 1 ? "" : "s"}.` });
  return made;
}

/** One file to the gallery's upload route. Answers `{added, refused}`. */
export function upload(client, file) {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open("POST", uploadUrl(client, file.name));
    xhr.setRequestHeader("Content-Type", "application/octet-stream");
    const token = client.transport && client.transport.token;
    if (token) xhr.setRequestHeader("Authorization", "Bearer " + token);
    xhr.onload = () => {
      let body = {};
      try {
        body = JSON.parse(xhr.responseText);
      } catch {
        /* an empty or odd answer is reported below */
      }
      if (xhr.status >= 200 && xhr.status < 300) resolve(body);
      else reject(new Error((body.error && body.error.message) || body.message || `the mixer answered ${xhr.status}`));
    };
    xhr.onerror = () => reject(new Error("the mixer could not be reached"));
    xhr.send(file);
  });
}

/** Every file in turn, then one report of what went in and what did not. */
export async function importFiles(client, files) {
  const added = [];
  const refused = [];
  for (const file of files) {
    try {
      const r = await upload(client, file);
      added.push(...(r.added || []));
      refused.push(...(r.refused || []).map((x) => ({ ...x, file: x.file === file.name ? x.file : `${file.name}: ${x.file}` })));
    } catch (e) {
      refused.push({ file: file.name, reason: e.message, fix: "Try again, or check the mixer's log." });
    }
  }
  report(added, refused);
  return { added, refused };
}

function report(added, refused) {
  if (!refused.length) {
    toast({ text: added.length ? `Added ${added.map((i) => i.name).join(", ")}.` : "Nothing was added." });
    return;
  }
  const rows = refused.map((r) => el("li", {}, [el("strong", { text: r.file }), el("div.sm", { text: r.reason }), el("div.sm.dim", { text: r.fix })]));
  const m = modal({
    title: refused.length === 1 ? "One file was not added" : `${refused.length} files were not added`,
    body: el("div.col", {}, [
      added.length ? el("p.sm", { text: `Added: ${added.map((i) => i.name).join(", ")}.` }) : null,
      el("ul.gx-refused", {}, rows),
    ]),
    footer: [el("button.btn.primary", { text: "OK", onclick: () => m.close() })],
  });
}

export { errorToast };
