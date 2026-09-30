// The words for a scene whose sources are not running, and the way into the
// Fix dialog. Shared by the Preview note, the composer and the dialog itself,
// all of which load on first use, so none of this is in the page a volunteer
// opens. Which sources those are is `shell/scene-health.js`, which is.

import { el } from "../../shell/dom.js";
import { lazyAction } from "../../shell/lazy-action.js";

export { notRunning, sceneNotRunning } from "../../shell/scene-health.js";

/** Every source a set of records draws, once each, in order. */
export function sourcesOf(records) {
  const seen = new Set();
  for (const r of records || []) {
    const source = r && r.content && r.content.source;
    if (source) seen.add(source);
  }
  return [...seen];
}

/** A source's name where the mixer has it, else its id. */
export function nameOf(client, id) {
  const source = client && client.store && client.store.source(id);
  return (source && source.name) || id;
}

/** "3 sources here are not running", or "Lyrics is not running". */
export function summaryLine(client, ids) {
  if (ids.length === 1) return `${nameOf(client, ids[0])} is not running`;
  return `${ids.length} sources here are not running`;
}

/**
 * The Fix dialog, fetched the first time it is asked for.
 * @param {{client, scenes, scene: string, draft?: string, records?: () => object[], onChanged?: (answer) => void}} opts
 */
export const openFix = lazyAction(() => import("./fix.js").then((m) => m.openFix), "Fix");

/**
 * The warning line with its Fix button, for a note that has room for a
 * sentence. `ids` empty hides it.
 */
export function fillNote(note, client, ids, opts, after) {
  // Built again only when the list changes: the Preview pane redraws on every
  // status, and a button swapped out between press and release is a click
  // that never lands.
  const key = ids.map((id) => `${id}=${nameOf(client, id)}`).join("|") + after;
  note.fixOpts = opts;
  if (note.fixKey === key) return;
  note.fixKey = key;
  note.hidden = !ids.length;
  note.replaceChildren();
  if (!ids.length) return;
  note.append(
    el("span.grow", { text: `${summaryLine(client, ids)}. ${after || ""}`.trim() }),
    el("button.btn.sm", {
      text: "Fix",
      title: `See why and fix it: ${ids.map((id) => nameOf(client, id)).join(", ")}`,
      onclick: (e) => {
        e.stopPropagation();
        openFix(note.fixOpts());
      },
      ondblclick: (e) => e.stopPropagation(),
    })
  );
}
