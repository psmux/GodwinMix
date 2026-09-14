// The record mirror: the scene document as this client believes it to be.
//
// The core is authoritative (11 section 4). A client never edits its copy and
// hopes; it sends a command, and the truth comes back two ways: as the view
// every mutating call answers with, and as `event/scene.patch` for changes
// somebody else made. Both land here.
//
// Three rules from 11 section 4, and they are the whole file:
//
//   * apply patches, never refetch. A patch is `{seq, source_client, scope,
//     added, updated: [{before, after}], removed: [id]}`, one per transaction.
//   * suppress the echo of your own edits by `source_client`. Suppressed does
//     not mean discarded: the record still lands, because the core may have
//     clamped what we asked for. It means the caller is told this change is
//     its own, so a drag in flight is not redrawn from underneath the hand.
//   * ignore record kinds and trailing fields you do not know, so a mirror
//     written today survives a document written by a newer core.

/** A record kind this mirror understands. Anything else is kept, not read. */
const KNOWN = new Set(["scene", "item"]);

export class SceneMirror {
  /** @param {{clientId?: string|null}} opts our own `source_client`. */
  constructor(opts = {}) {
    this.clientId = opts.clientId || null;
    /** id -> record, every kind, exactly as it arrived. */
    this.records = new Map();
    /** The highest patch sequence number applied. */
    this.seq = 0;
    /** How many records had a kind this build does not know. */
    this.unknown = 0;
  }

  setClientId(id) {
    this.clientId = id || null;
  }

  // ------------------------------------------------------------- reading

  /** Every scene record, in document order. */
  scenes() {
    return [...this.records.values()].filter((r) => r.kind === "scene").sort(byOrder);
  }

  /** One record by id, or null. */
  record(id) {
    return this.records.get(id) || null;
  }

  /** The items of one scene, parents before children, bottom of the stack first. */
  items(sceneId) {
    return [...this.records.values()].filter((r) => r.kind === "item" && r.parent === sceneId).sort(byOrder);
  }

  /** Everything under a scene, groups and their children included. */
  descendants(sceneId) {
    const out = [];
    const walk = (parent) => {
      for (const r of this.items(parent)) {
        out.push(r);
        walk(r.id);
      }
    };
    walk(sceneId);
    return out;
  }

  /** The scene a record belongs to, following parents up. */
  sceneOf(id) {
    let at = this.records.get(id);
    while (at && at.kind === "item" && at.parent) {
      const up = this.records.get(at.parent);
      if (!up) return at.parent;
      at = up;
    }
    return at && at.kind === "scene" ? at.id : null;
  }

  // ------------------------------------------------------------- writing

  /**
   * One scene as a command answered with it: `{id, name, canvas, records,
   * geometry}`. The scene's own subtree is replaced, and nothing else is
   * touched, so an answer about one scene never disturbs another.
   */
  applyView(view) {
    if (!view || !Array.isArray(view.records)) return { changed: [] };
    const keep = new Set(view.records.map((r) => r.id));
    for (const stale of [view.id, ...this.descendants(view.id).map((r) => r.id)]) {
      if (!keep.has(stale)) this._drop(stale);
    }
    const changed = [];
    for (const record of view.records) {
      this._put(record);
      changed.push(record.id);
    }
    this.canvas = view.canvas || this.canvas;
    return { changed };
  }

  /**
   * Replace the whole mirror. `scene.list` answers with summaries rather than
   * records, so this takes whatever views a client gathered, in one go.
   */
  reset(views) {
    this.records.clear();
    this.unknown = 0;
    for (const view of views || []) this.applyView(view);
  }

  /**
   * One `event/scene.patch`.
   *
   * @returns {{applied: boolean, echo: boolean, gap: boolean, seq: number,
   *            added: string[], updated: string[], removed: string[]}}
   */
  applyPatch(patch) {
    const empty = { applied: false, echo: false, gap: false, seq: this.seq, added: [], updated: [], removed: [] };
    if (!patch || typeof patch !== "object") return empty;
    // `presence` (who is looking at what) is not drawn here. Nothing is
    // applied, so the sequence number reported is the one the mirror is
    // actually at: a caller that trusted the patch's own number would think it
    // had caught up with something it never read.
    if (patch.scope && patch.scope !== "document") return empty;

    const seq = Number(patch.seq || 0);
    // A patch older than what we have already applied is a duplicate from a
    // reconnect. A patch that skips numbers means events were dropped and the
    // caller wants a fresh snapshot rather than a document with a hole in it.
    const gap = this.seq > 0 && seq > this.seq + 1;
    if (seq > 0 && seq <= this.seq) return Object.assign(empty, { seq: this.seq });

    const added = [];
    const updated = [];
    const removed = [];
    for (const record of patch.added || []) {
      this._put(record);
      added.push(record.id);
    }
    for (const change of patch.updated || []) {
      const after = change && change.after;
      if (!after || !after.id) continue;
      this._put(after);
      updated.push(after.id);
    }
    for (const id of patch.removed || []) {
      // A removal names the id; `removed_records` is the core's own business.
      const key = typeof id === "string" ? id : id && id.id;
      if (!key) continue;
      this._drop(key);
      removed.push(key);
    }
    if (seq > 0) this.seq = seq;
    return {
      applied: true,
      echo: !!(this.clientId && patch.source_client === this.clientId),
      gap,
      seq: this.seq,
      added,
      updated,
      removed,
    };
  }

  /** Forget one record, keeping the count of unknown kinds honest. */
  _drop(id) {
    const record = this.records.get(id);
    if (record && !KNOWN.has(record.kind)) this.unknown -= 1;
    this.records.delete(id);
  }

  _put(record) {
    if (!record || !record.id) return;
    const had = this.records.get(record.id);
    if (had && !KNOWN.has(had.kind)) this.unknown -= 1;
    if (!KNOWN.has(record.kind)) this.unknown += 1;
    this.records.set(record.id, record);
  }
}

/**
 * Siblings sort by a fractional key (`order.rs`), which is a string compared
 * lexically so an item can be moved between two others without renumbering.
 */
function byOrder(a, b) {
  // A record with no order sorts first rather than under the string "null",
  // which is where `String(null)` would put it.
  const x = a.order === undefined || a.order === null ? "" : String(a.order);
  const y = b.order === undefined || b.order === null ? "" : String(b.order);
  return x < y ? -1 : x > y ? 1 : 0;
}

/**
 * Geometry by item id, from the flattened list a command answers with.
 * The composer draws handles off this and never recomputes layout itself.
 */
export function geometryIndex(view) {
  const out = new Map();
  for (const box of (view && view.geometry) || []) out.set(box.item, box);
  return out;
}
