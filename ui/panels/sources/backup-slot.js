// Where a source's backup line goes, on a tile and under the programme. The
// line loads when a source first has something to say, so a show with no
// backups never fetches it.

import { el } from "../../shell/dom.js";

const down = (b) => !!(b && b.phase && b.phase !== "ready");

/** Gone down with no backup set. Text and graphics never go down. */
export function wantsBackupHint(s) {
  if (!s || s.on_loss || s.has_video === false || /^(text|ticker|template)\//.test(s.type || "")) return false;
  return s.state === "stalled" || s.state === "failed";
}

function lazy(load, wanted, make) {
  let loading = false;
  let last = [];
  const slot = { node: el("div"), part: null };
  slot.sync = (...args) => {
    last = args;
    if (slot.part) return slot.part.sync(...last);
    if (loading || !wanted(...args)) return;
    loading = true;
    load().then((m) => {
      slot.part = make(m);
      slot.node.append(slot.part.node);
      slot.part.sync(...last);
    }, () => (loading = false));
  };
  return slot;
}

export function lazyLine(deps) {
  const wanted = (s) => down(s.backup) || !!(s.backup && s.backup.warning) || wantsBackupHint(s);
  return lazy(() => import("./backup-line.js"), wanted, (m) => m.backupLine(deps));
}

export function lazyBanner(client) {
  const wanted = (state) => state.sources.some((s) => down(s.backup));
  return lazy(() => import("../multiview/backup-banner.js"), wanted, (m) => new m.BackupBanner(client));
}

/** A source's name by its id. */
export function namesIn(state, nameOf) {
  return (id) => {
    const found = (state.sources || []).find((s) => s.id === id);
    return found ? nameOf(found) : id;
  };
}
