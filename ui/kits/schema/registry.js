// A ranked tester registry, so a client can render a format better than the
// kit does without the plugin knowing (11 section 5).
//
// Every renderer answers one question: how well do you handle this control?
// The highest number wins, ties go to whoever registered last, and a renderer
// that answers zero or less is not asked again. That is JSON Forms' model and
// it is the reason a host can add a colour picker, a curve editor or an audio
// meter for `template.*` controls that no plugin ships code for.
//
// Nothing here is browser specific. The Python kit registers ttk widgets
// against the same ranks and gets the same choices.

/** What the kit's own widgets rank at. Beat it to take over a control. */
export const BASE = 10;
export const SPECIFIC = 20;
export const HOST = 30;

export class Renderers {
  constructor() {
    this.entries = [];
  }

  /**
   * @param {{name: string, test: (node, field) => number, make: (node, ctx) => any}} entry
   * @returns {() => void} removal, so a panel can take its renderer away again
   */
  register(entry) {
    if (!entry || typeof entry.test !== "function" || typeof entry.make !== "function") {
      throw new Error("a renderer needs a test and a make function");
    }
    this.entries.push(entry);
    return () => {
      const at = this.entries.indexOf(entry);
      if (at >= 0) this.entries.splice(at, 1);
    };
  }

  /** The best renderer for one control node, or null when nothing will have it. */
  pick(node) {
    let best = null;
    let bestRank = 0;
    // Later registrations win ties, so a client's own renderer overrides the
    // kit's without having to invent a higher number.
    for (const entry of this.entries) {
      const rank = Number(entry.test(node, node.field)) || 0;
      if (rank > 0 && rank >= bestRank) {
        bestRank = rank;
        best = entry;
      }
    }
    return best ? { entry: best, rank: bestRank } : null;
  }

  /** What would render each control, by name. For a test and for a doctor. */
  explain(layout) {
    const out = {};
    const walk = (n) => {
      if (n.kind === "control") {
        const picked = this.pick(n);
        out[n.field.name] = picked ? picked.entry.name : null;
      }
      for (const child of n.elements || []) walk(child);
    };
    walk(layout);
    return out;
  }
}

/**
 * Register one renderer per control name, all at the same rank.
 * The shape most clients want: a table from control to widget maker.
 */
export function registerTable(registry, table, rank = BASE) {
  const offs = [];
  for (const [control, make] of Object.entries(table)) {
    offs.push(
      registry.register({
        name: control,
        test: (node) => (node.control === control ? rank : 0),
        make,
      })
    );
  }
  return () => offs.forEach((off) => off());
}
