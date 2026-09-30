// The Fix dialog: a scene's sources that are not running, why, and what to
// do about each.
//
// Opened from the Preview note, the mark on a scene's tab or tile, and the
// composer. Each row says why in the source's own words (from
// `source.missing`) and carries the one thing that would make it run where
// the page can do it: Retry, Put back, Install, or another camera. Under the
// list, the checked ones come out of the scene in one step, which one Ctrl+Z
// puts back. Fetched on the first press of Fix and never before.

import { el, clear } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { notRunning, sourcesOf } from "./fix-note.js";
import { row } from "./fix-row.js";

/**
 * @param {{client, scenes, scene: string, draft?: string,
 *          records?: () => object[], onChanged?: (answer: object) => void}} opts
 */
export async function openFix(opts) {
  if (!document.querySelector('link[href$="/scenes/fix.css"]')) {
    document.head.appendChild(el("link", { rel: "stylesheet", href: new URL("./fix.css", import.meta.url).href }));
  }
  const { client, scenes } = opts;
  const summary = scenes.summary(opts.scene) || { id: opts.scene, name: opts.scene };
  const records = () => (opts.records ? opts.records() : scenes.mirror.descendants(summary.id));
  const checked = new Set();
  const known = new Set();
  let facts = new Map();
  let shown = "";

  const list = el("div.fix-list", { role: "list", "aria-label": "Sources not running" });
  const removeBtn = el("button.btn.primary", { text: "Remove from this scene", onclick: () => remove() });
  const lead = el("p.sm.dim", { style: { margin: "0 0 8px" } });

  async function learn(ids) {
    try {
      const found = await client.call("source.missing", { ids });
      facts = new Map((found || []).map((m) => [m.id, m]));
    } catch {
      // A core from before `source.missing` still gets the list and the
      // removal; it only cannot say why.
      facts = new Map();
    }
  }

  async function render(force) {
    const ids = notRunning(client, sourcesOf(records()));
    const key = ids.map((id) => `${id}:${(client.store.source(id) || {}).state || "-"}`).join("|");
    if (!force && key === shown) return;
    shown = key;
    await learn(ids);
    // All checked when first seen; a box somebody cleared stays clear.
    for (const id of ids.filter((id) => !known.has(id))) {
      known.add(id);
      checked.add(id);
    }
    for (const id of [...checked]) if (!ids.includes(id)) checked.delete(id);
    clear(list);
    if (!ids.length) {
      list.append(el("p", { text: "Every source in this scene is running now." }));
    }
    for (const id of ids) {
      list.append(row(client, facts.get(id) || { id, why: "unknown" }, checked, () => render(true), paint));
    }
    lead.textContent = ids.length
      ? `${summary.name} goes to air without ${ids.length === 1 ? "this source" : `these ${ids.length} sources`}. Fix what you can, or take them out of the scene.`
      : "";
    paint();
  }

  function paint() {
    removeBtn.disabled = checked.size === 0;
    removeBtn.textContent = checked.size > 1 ? `Remove ${checked.size} from this scene` : "Remove from this scene";
  }

  /** The checked sources' items, out of the scene (or the draft) as one undo step. */
  async function remove() {
    const gone = new Set(checked);
    const items = records().filter((r) => r.content && gone.has(r.content.source));
    if (!items.length) return;
    const label = `Removed ${gone.size === 1 ? "a source" : `${gone.size} sources`} from ${summary.name}`;
    try {
      await scenes.undo.group(label, async () => {
        for (const item of items) {
          const params = { scene: summary.id, item: item.id };
          if (opts.draft) params.draft = opts.draft;
          const answer = await scenes.call("scene.item.remove", params);
          if (opts.onChanged) opts.onChanged(answer);
        }
      }, { offer: true });
      if (!opts.draft) await scenes.reread([summary.id]);
      dialog.close();
    } catch (e) {
      errorToast(e, "Remove from scene");
    }
  }

  const dialog = modal({
    title: `Not running in ${summary.name}`,
    body: el("div.fix", {}, [lead, list]),
    footer: [el("span.grow"), el("button.btn", { text: "Close", onclick: () => dialog.close() }), removeBtn],
    onClose: () => off(),
  });
  dialog.el.classList.add("fix-dialog");
  const off = client.onRender(() => render(false).catch((e) => console.error(e)));
  await render(true);
  if (!list.querySelector(".fix-row")) toast({ text: "Every source in this scene is running." });
  return dialog;
}
