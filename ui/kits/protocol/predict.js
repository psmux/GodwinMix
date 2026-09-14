// Drag at input rate, truth in the core (11 section 4).
//
// A drag cannot wait for a round trip. Not because the socket is slow, a
// loopback call is tens of microseconds, but because a blocked redraw costs a
// frame: a click tolerates 100 ms and a drag tolerates about 25. So the client
// draws its own move immediately, sends it with a sequence number, and the core
// echoes the last number it applied. Echoes older than our latest input are
// discarded, and the drawing snaps to the core's answer only when the numbers
// meet. This is Quake's prediction and reconciliation, and it is what removes
// the rubber banding where a slow echo drags the handle backwards under the
// cursor.
//
// Nothing here knows what a transform is. It holds `props` objects, whatever
// they contain, which is why the same file serves a corner drag, an opacity
// dial and a plugin's own dial on `params.key_tolerance`.

export class Prediction {
  constructor() {
    /** The last number handed out. Monotonic for the life of the client. */
    this.seq = 0;
    /** The highest number the core has told us it applied. */
    this.acked = 0;
    /** item id -> {seq, props} still in flight. */
    this.pending = new Map();
  }

  /** True while any item has a move the core has not confirmed. */
  get busy() {
    return this.pending.size > 0;
  }

  /**
   * Draw this now, send it with the number this returns.
   * A second prediction for the same item replaces the first: the newer one is
   * where the hand is, and the older one is a frame nobody will ever see again.
   */
  predict(item, props) {
    this.seq += 1;
    const had = this.pending.get(item);
    this.pending.set(item, { seq: this.seq, props: mergeProps(had ? had.props : {}, props) });
    return this.seq;
  }

  /**
   * The core has applied everything up to and including `seq`.
   *
   * Two things call this: the answer to `scene.item.set`, which is the echo on
   * the RPC transport, and `event/scene.patch` carrying the client's own
   * number back. Either is enough; both together are simply earlier.
   *
   * @returns {string[]} the items that are now the core's again.
   */
  settle(seq) {
    const n = Number(seq || 0);
    if (n <= this.acked) return [];
    this.acked = n;
    const done = [];
    for (const [item, held] of this.pending) {
      if (held.seq <= n) {
        this.pending.delete(item);
        done.push(item);
      }
    }
    return done;
  }

  /** Throw away every prediction, for a cancelled drag or a resync. */
  reset() {
    this.pending.clear();
  }

  /**
   * What to draw for one item: our own move while it is in flight, the core's
   * record once it has caught up.
   */
  resolve(item, serverProps) {
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
  accepts(item, echoSeq) {
    const held = this.pending.get(item);
    if (!held) return true;
    return Number(echoSeq || 0) >= held.seq;
  }
}

/**
 * The sequence number a patch is echoing back.
 *
 * `client_seq` is the name this kit asks for. A core that spells it
 * differently is read anyway rather than ignored: getting the number late
 * costs a snap, getting it never costs a stuck prediction.
 */
export function echoSeqOf(patch) {
  if (!patch) return 0;
  return Number(patch.client_seq ?? patch.echo_seq ?? patch.seq_echo ?? 0) || 0;
}

/**
 * Merge `next` onto `base` the way `scene.item.set` merges props into an item:
 * a nested object is merged, everything else is replaced, and neither argument
 * is modified.
 *
 * The client has to do the same arithmetic as the core or the predicted frame
 * and the confirmed frame differ by whatever the client forgot to carry over.
 */
export function mergeProps(base, next) {
  const out = Object.assign({}, base || {});
  for (const [key, value] of Object.entries(next || {})) {
    const had = out[key];
    if (isPlain(had) && isPlain(value)) out[key] = mergeProps(had, value);
    else out[key] = value;
  }
  return out;
}

function isPlain(v) {
  return !!v && typeof v === "object" && !Array.isArray(v);
}
