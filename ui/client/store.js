// The typed state store.
//
// The core sends a snapshot, then deltas, then `event/flush`. Everything in the
// UI reads from here and nothing re-reads the wire. Listeners fire at flush and
// never per event, which is Neovim's rule and the reason a batch of twenty
// changes repaints once.

/** The shape the store starts in, so a panel never has to guard for undefined. */
export function emptyState() {
  return {
    // The source on air, when the programme is one source. Null whenever a
    // scene with more than one item is live, which is what `scene` is for.
    program: null,
    // The scene on air, by name. The core sends both in its status document
    // and only one of them is ever set.
    scene: null,
    preview: null,
    sources: [],
    outputs: [],
    multiview: { enabled: false, width: 0, height: 0, cols: 0, rows: 0, cells: [], fps: 0 },
    uptime_secs: 0,
    running_time_ms: 0,
    backend: null,
    ad: null,
    media: { items: [], dir: null, allow_upload: false },
    // Derived streams the core sends beside the status document.
    meters: { program: [], sources: {} },
    tally: {},
    alerts: [],
    seq: 0,
    connected: false,
  };
}

export class Store {
  constructor() {
    this.state = emptyState();
    this._listeners = new Set();
    this._dirty = false;
  }

  /** Register for the flush tick. Returns an unsubscribe function. */
  subscribe(fn) {
    this._listeners.add(fn);
    return () => this._listeners.delete(fn);
  }

  /** Replace the status document wholesale. Used by `event/snapshot`. */
  snapshot(status, seq) {
    const keep = { media: this.state.media, meters: this.state.meters, alerts: this.state.alerts };
    this.state = Object.assign(emptyState(), keep, status, {
      seq: seq ?? this.state.seq,
      connected: true,
    });
    this._dirty = true;
  }

  /** Shallow merge at the top level. Used by everything else. */
  patch(fields) {
    Object.assign(this.state, fields);
    this._dirty = true;
  }

  /** Replace one source in place, matched by id. Returns true if it was found. */
  patchSource(id, fields) {
    const i = this.state.sources.findIndex((s) => s.id === id);
    if (i < 0) return false;
    this.state.sources[i] = Object.assign({}, this.state.sources[i], fields);
    this._dirty = true;
    return true;
  }

  patchOutput(id, fields) {
    const i = this.state.outputs.findIndex((o) => o.id === id);
    if (i < 0) return false;
    this.state.outputs[i] = Object.assign({}, this.state.outputs[i], fields);
    this._dirty = true;
    return true;
  }

  /** Meters arrive ten times a second and are kept out of the flush path. */
  setMeters(program, sources) {
    if (program) this.state.meters.program = program;
    if (sources) Object.assign(this.state.meters.sources, sources);
  }

  addAlert(alert) {
    this.state.alerts.unshift(Object.assign({ at: Date.now() }, alert));
    if (this.state.alerts.length > 50) this.state.alerts.length = 50;
    this._dirty = true;
  }

  /** Tell every listener, once, if anything changed since the last flush. */
  flush(force) {
    if (!this._dirty && !force) return;
    this._dirty = false;
    for (const fn of this._listeners) {
      try {
        fn(this.state);
      } catch (e) {
        console.error("panel threw while rendering", e);
      }
    }
  }

  // ------------------------------------------------------------ lookups

  source(id) {
    return this.state.sources.find((s) => s.id === id) || null;
  }

  output(id) {
    return this.state.outputs.find((o) => o.id === id) || null;
  }

  /** "program", "preview" or "off" for one source id. */
  tallyOf(id) {
    if (this.state.tally && this.state.tally[id]) return this.state.tally[id];
    if (this.state.program === id) return "program";
    if (this.state.preview === id) return "preview";
    return "off";
  }
}
