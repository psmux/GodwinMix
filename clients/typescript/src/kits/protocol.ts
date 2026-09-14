// The protocol kit: the record mirror, the prediction ledger and the undo proxy.
//
// A typed port of ui/kits/protocol/mirror.js, predict.js and undo.js. The
// reference modules in ui/kits are the source of truth for the algorithm and
// ui/kits/fixtures.json is the source of truth for the behaviour, which
// test/kits.test.ts replays against this file.
//
// Nothing here touches a DOM, a socket or a timer. It is state and arithmetic,
// so it runs under node, under Deno and inside a bundle. UndoProxy needs one
// thing from a caller: an object with a `call` method.

// --------------------------------------------------------------- the shapes

/** A record as the core wrote it. Fields this build does not read are kept. */
export interface SceneRecord {
  id: string;
  kind: string;
  /** The scene or group this item hangs under. */
  parent?: string;
  /** A fractional key (`order.rs`), compared as a string. */
  order?: string;
  name?: string;
  [key: string]: unknown;
}

export interface Canvas {
  width: number;
  height: number;
  fps?: number;
  [key: string]: unknown;
}

/** The derived geometry of one item, from the flattened list a view carries. */
export interface GeometryBox {
  item: string;
  [key: string]: unknown;
}

/** What a mutating `scene.*` command answers with: one scene, whole. */
export interface SceneView {
  id: string;
  name?: string;
  canvas?: Canvas;
  records: SceneRecord[];
  geometry?: GeometryBox[];
  [key: string]: unknown;
}

/** One half of an `updated` entry in a patch. */
export interface RecordChange {
  before?: SceneRecord;
  after?: SceneRecord;
}

/** One `event/scene.patch`, which is one transaction in the core. */
export interface ScenePatch {
  seq?: number;
  source_client?: string;
  /** "document" is what this mirror draws. "presence" and the rest are skipped. */
  scope?: string;
  added?: SceneRecord[];
  updated?: RecordChange[];
  /** An id, or an object carrying one. */
  removed?: Array<string | { id?: string }>;
  [key: string]: unknown;
}

export interface PatchResult {
  applied: boolean;
  /** True when this is the echo of a change we made ourselves. */
  echo: boolean;
  /** True when the sequence skipped, so events were dropped somewhere. */
  gap: boolean;
  seq: number;
  added: string[];
  updated: string[];
  removed: string[];
}

export interface ViewResult {
  changed: string[];
}

export interface MirrorOptions {
  clientId?: string | null;
}

/** A record kind this mirror understands. Anything else is kept, not read. */
const KNOWN = new Set(["scene", "item"]);

// -------------------------------------------------------------- the mirror

/**
 * The scene document as this client believes it to be.
 *
 * The core is authoritative. A client never edits its copy and hopes; it sends
 * a command, and the truth comes back two ways: as the view every mutating call
 * answers with, and as `event/scene.patch` for changes somebody else made. Both
 * land here, and three rules cover the whole class:
 *
 *   * apply patches, never refetch.
 *   * suppress the echo of your own edits by `source_client`. Suppressed does
 *     not mean discarded: the record still lands, because the core may have
 *     clamped what we asked for. It means the caller is told the change is its
 *     own, so a drag in flight is not redrawn from underneath the hand.
 *   * ignore record kinds and trailing fields you do not know, so a mirror
 *     written today survives a document written by a newer core.
 */
export class SceneMirror {
  clientId: string | null;
  /** id -> record, every kind, exactly as it arrived. */
  records: Map<string, SceneRecord>;
  /** The highest patch sequence number applied. */
  seq: number;
  /** How many records had a kind this build does not know. */
  unknown: number;
  /** The canvas of the last view that carried one. */
  canvas?: Canvas;

  constructor(opts: MirrorOptions = {}) {
    this.clientId = opts.clientId || null;
    this.records = new Map();
    this.seq = 0;
    this.unknown = 0;
  }

  setClientId(id: string | null | undefined): void {
    this.clientId = id || null;
  }

  // ------------------------------------------------------------- reading

  /** Every scene record, in document order. */
  scenes(): SceneRecord[] {
    return [...this.records.values()].filter((r) => r.kind === "scene").sort(byOrder);
  }

  /** One record by id, or null. */
  record(id: string): SceneRecord | null {
    return this.records.get(id) || null;
  }

  /** The items of one scene, parents before children, bottom of the stack first. */
  items(sceneId: string): SceneRecord[] {
    return [...this.records.values()]
      .filter((r) => r.kind === "item" && r.parent === sceneId)
      .sort(byOrder);
  }

  /** Everything under a scene, groups and their children included. */
  descendants(sceneId: string): SceneRecord[] {
    const out: SceneRecord[] = [];
    const walk = (parent: string): void => {
      for (const r of this.items(parent)) {
        out.push(r);
        walk(r.id);
      }
    };
    walk(sceneId);
    return out;
  }

  /** The scene a record belongs to, following parents up. */
  sceneOf(id: string): string | null {
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
   * One scene as a command answered with it. The scene's own subtree is
   * replaced and nothing else is touched, so an answer about one scene never
   * disturbs another.
   */
  applyView(view: SceneView | null | undefined): ViewResult {
    if (!view || !Array.isArray(view.records)) return { changed: [] };
    const keep = new Set(view.records.map((r) => r.id));
    for (const stale of [view.id, ...this.descendants(view.id).map((r) => r.id)]) {
      if (!keep.has(stale)) this.records.delete(stale);
    }
    const changed: string[] = [];
    for (const record of view.records) {
      this.put(record);
      changed.push(record.id);
    }
    this.canvas = view.canvas || this.canvas;
    return { changed };
  }

  /**
   * Replace the whole mirror. `scene.list` answers with summaries rather than
   * records, so this takes whatever views a client gathered, in one go.
   */
  reset(views: SceneView[] | null | undefined): void {
    this.records.clear();
    this.unknown = 0;
    for (const view of views || []) this.applyView(view);
  }

  /** One `event/scene.patch`. */
  applyPatch(patch: ScenePatch | null | undefined): PatchResult {
    const empty = (seq: number): PatchResult => ({
      applied: false,
      echo: false,
      gap: false,
      seq,
      added: [],
      updated: [],
      removed: [],
    });
    if (!patch || typeof patch !== "object") return empty(this.seq);
    // `presence` (who is looking at what) is not drawn here. Nothing is
    // applied, so the sequence number reported is the one the mirror is
    // actually at: a caller that trusted the patch's own number would think it
    // had caught up with something it never read.
    if (patch.scope && patch.scope !== "document") return empty(this.seq);

    const seq = Number(patch.seq || 0);
    // A patch older than what we have already applied is a duplicate from a
    // reconnect. A patch that skips numbers means events were dropped and the
    // caller wants a fresh snapshot rather than a document with a hole in it.
    const gap = this.seq > 0 && seq > this.seq + 1;
    if (seq > 0 && seq <= this.seq) return empty(this.seq);

    const added: string[] = [];
    const updated: string[] = [];
    const removed: string[] = [];
    for (const record of patch.added || []) {
      this.put(record);
      added.push(record.id);
    }
    for (const change of patch.updated || []) {
      const after = change && change.after;
      if (!after || !after.id) continue;
      this.put(after);
      updated.push(after.id);
    }
    for (const id of patch.removed || []) {
      // A removal names the id; `removed_records` is the core's own business.
      const key = typeof id === "string" ? id : id && id.id;
      if (!key) continue;
      const record = this.records.get(key);
      if (record && !KNOWN.has(record.kind)) this.unknown -= 1;
      this.records.delete(key);
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

  private put(record: SceneRecord | null | undefined): void {
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
function byOrder(a: SceneRecord, b: SceneRecord): number {
  const x = a.order === undefined ? "" : String(a.order);
  const y = b.order === undefined ? "" : String(b.order);
  return x < y ? -1 : x > y ? 1 : 0;
}

/**
 * Geometry by item id, from the flattened list a command answers with.
 * The composer draws handles off this and never recomputes layout itself.
 */
export function geometryIndex(view: SceneView | null | undefined): Map<string, GeometryBox> {
  const out = new Map<string, GeometryBox>();
  for (const box of (view && view.geometry) || []) out.set(box.item, box);
  return out;
}

// ---------------------------------------------------------- the prediction

/** A bag of item properties. Nothing here reads inside it. */
export type Props = Record<string, unknown>;

interface Held {
  seq: number;
  props: Props;
}

/**
 * Drag at input rate, truth in the core.
 *
 * A drag cannot wait for a round trip. Not because the socket is slow, a
 * loopback call is tens of microseconds, but because a blocked redraw costs a
 * frame: a click tolerates 100 ms and a drag tolerates about 25. So the client
 * draws its own move immediately, sends it with a sequence number, and the core
 * echoes the last number it applied. Echoes older than our latest input are
 * discarded, and the drawing snaps to the core's answer only when the numbers
 * meet. That is what removes the rubber banding where a slow echo drags the
 * handle backwards under the cursor.
 *
 * Nothing here knows what a transform is. It holds `props` objects, whatever
 * they contain, which is why the same class serves a corner drag, an opacity
 * dial and a plugin's own dial on `params.key_tolerance`.
 */
export class Prediction {
  /** The last number handed out. Monotonic for the life of the client. */
  seq: number;
  /** The highest number the core has told us it applied. */
  acked: number;
  /** item id -> the move still in flight. */
  pending: Map<string, Held>;

  constructor() {
    this.seq = 0;
    this.acked = 0;
    this.pending = new Map();
  }

  /** True while any item has a move the core has not confirmed. */
  get busy(): boolean {
    return this.pending.size > 0;
  }

  /**
   * Draw this now, send it with the number this returns.
   * A second prediction for the same item replaces the first: the newer one is
   * where the hand is, and the older one is a frame nobody will ever see again.
   */
  predict(item: string, props: Props): number {
    this.seq += 1;
    const had = this.pending.get(item);
    this.pending.set(item, { seq: this.seq, props: mergeProps(had ? had.props : {}, props) });
    return this.seq;
  }

  /**
   * The core has applied everything up to and including `seq`.
   *
   * Two things call this: the answer to `scene.item.set`, which is the echo on
   * the RPC transport, and `event/scene.patch` carrying the client's own number
   * back. Either is enough; both together are simply earlier.
   *
   * @returns the items that are now the core's again.
   */
  settle(seq: number): string[] {
    const n = Number(seq || 0);
    if (n <= this.acked) return [];
    this.acked = n;
    const done: string[] = [];
    for (const [item, held] of this.pending) {
      if (held.seq <= n) {
        this.pending.delete(item);
        done.push(item);
      }
    }
    return done;
  }

  /** Throw away every prediction, for a cancelled drag or a resync. */
  reset(): void {
    this.pending.clear();
  }

  /**
   * What to draw for one item: our own move while it is in flight, the core's
   * record once it has caught up.
   */
  resolve(item: string, serverProps: Props | undefined): Props | undefined {
    const held = this.pending.get(item);
    return held ? mergeProps(serverProps || {}, held.props) : serverProps;
  }

  /**
   * Should an incoming change be drawn over this item?
   *
   * No while we are still holding a newer move for it: that is the echo of
   * something the operator has already dragged past, and drawing it is exactly
   * the rubber band this class exists to remove.
   */
  accepts(item: string, echoSeq: number): boolean {
    const held = this.pending.get(item);
    if (!held) return true;
    return Number(echoSeq || 0) >= held.seq;
  }
}

/**
 * The sequence number a patch is echoing back.
 *
 * `client_seq` is the name this kit asks for. A core that spells it differently
 * is read anyway rather than ignored: getting the number late costs a snap,
 * getting it never costs a stuck prediction.
 */
export function echoSeqOf(patch: Record<string, unknown> | null | undefined): number {
  if (!patch) return 0;
  const raw = patch["client_seq"] ?? patch["echo_seq"] ?? patch["seq_echo"] ?? 0;
  return Number(raw) || 0;
}

/**
 * Merge `next` onto `base` the way `scene.item.set` merges props into an item:
 * a nested object is merged, everything else is replaced, and neither argument
 * is modified.
 *
 * The client has to do the same arithmetic as the core, or the predicted frame
 * and the confirmed frame differ by whatever the client forgot to carry over.
 */
export function mergeProps(base: Props | null | undefined, next: Props | null | undefined): Props {
  const out: Props = Object.assign({}, base || {});
  for (const [key, value] of Object.entries(next || {})) {
    const had = out[key];
    if (isPlain(had) && isPlain(value)) out[key] = mergeProps(had, value);
    else out[key] = value;
  }
  return out;
}

function isPlain(v: unknown): v is Props {
  return !!v && typeof v === "object" && !Array.isArray(v);
}

// ---------------------------------------------------------- the undo proxy

/** The one thing the undo proxy needs of a client. */
export interface UndoClient {
  call(method: string, params?: Record<string, unknown>): Promise<unknown>;
}

/** One line in the shell's undo menu. */
export interface UndoEntry {
  label: string | null | undefined;
  /** True when the shell should offer an Undo button in its toast. */
  offer: boolean;
  undo: () => Promise<unknown>;
  redo: () => Promise<unknown>;
}

export interface UndoStack {
  push(entry: UndoEntry): void;
}

export interface RecordOptions {
  offer?: boolean;
}

/**
 * Ctrl+Z on this page is `scene.undo` in the core.
 *
 * The undo stack for the document lives in the core, as an inverse diff stack.
 * A client that kept its own would be wrong the moment a second client, an
 * agent or the CLI changed anything, so this class does not keep one. What it
 * keeps is the shell's stack of one line labels, each step of which calls the
 * core and lets the core decide what the inverse actually is.
 *
 * A drag is one step, not forty. `scene.history.mark` folds the moves between
 * two marks into a single entry, which is why `group()` exists.
 */
export class UndoProxy {
  client: UndoClient;
  stack: UndoStack;
  /** False once the core has told us it has no history, so we stop asking. */
  available: boolean;
  /** Set by the shell to hear what each step answered with. */
  onStep?: (answer: unknown) => void;

  constructor(client: UndoClient, stack: UndoStack) {
    this.client = client;
    this.stack = stack;
    this.available = true;
  }

  /** Name what follows, so a drag becomes one Ctrl+Z. */
  async mark(label?: string | null): Promise<void> {
    if (!this.available) return;
    try {
      await this.client.call("scene.history.mark", label ? { label } : {});
    } catch (e) {
      if (e && (e as { code?: number }).code === -32601) this.available = false;
    }
  }

  /**
   * Run a batch of edits as one undo step.
   *
   * The first mark carries the label; the second closes the group, which is the
   * shape `scene.history.mark` documents. The entry is pushed only if the work
   * succeeded: a failed edit has nothing to take back.
   */
  async group<T>(label: string, fn: () => T | Promise<T>, opts: RecordOptions = {}): Promise<T> {
    await this.mark(label);
    const result = await fn();
    await this.mark(null);
    this.record(label, opts);
    return result;
  }

  /**
   * One finished change, as a line in the shell's undo menu.
   * `offer` puts an Undo button in a toast, which the destructive ones want.
   */
  record(label: string | null | undefined, opts: RecordOptions = {}): void {
    if (!this.available) return;
    this.stack.push({
      label,
      offer: !!opts.offer,
      undo: () => this.undo(),
      redo: () => this.redo(),
    });
  }

  undo(): Promise<unknown> {
    return this.step("scene.undo");
  }

  redo(): Promise<unknown> {
    return this.step("scene.redo");
  }

  /**
   * The core answers with the patch it applied and how many steps are left, so
   * a menu can grey itself out without asking a second question.
   */
  async step(method: string): Promise<unknown> {
    const answer = await this.client.call(method, {});
    if (this.onStep) this.onStep(answer);
    return answer;
  }
}
