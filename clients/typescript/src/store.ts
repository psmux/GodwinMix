// The typed state store.
//
// The core sends a snapshot, then deltas, then `event/flush`. Everything in a
// surface reads from here and nothing re-reads the wire. Listeners fire at
// flush and never per event, which is the rule that makes a batch of twenty
// changes one repaint.
//
// This is the same store as ui/client/store.js, typed. A test in this package
// runs both over the same scripted session and fails if they disagree.

import type {
  AdStatus,
  AlertEvent,
  Meters,
  MixerStatus,
  MultiviewLayout,
  MultiviewStatus,
  OutputStatus,
  SourceStatus,
} from "./generated/protocol.ts";

export interface State {
  program: string | null;
  /** The preview scene, once a core sends `event/preview.changed`. */
  preview: string | null;
  sources: SourceStatus[];
  outputs: OutputStatus[];
  multiview: MultiviewStatus;
  uptime_secs: number;
  running_time_ms: number;
  backend: MixerStatus["backend"] | null;
  ad: AdStatus | null;
  /** Source id to "program", "preview" or "off". */
  tally: Record<string, string>;
  /** Peak dBFS. Ten a second, and outside the flush path on purpose. */
  meters: Meters;
  /** Newest first, capped at fifty. */
  alerts: Array<AlertEvent & { at: number }>;
  /** The grid the binary frames are cut to. */
  layout: MultiviewLayout | null;
  seq: number;
  connected: boolean;
}

/** The shape the store starts in, so a surface never guards for undefined. */
export function emptyState(): State {
  return {
    program: null,
    preview: null,
    sources: [],
    outputs: [],
    multiview: { enabled: false, width: 0, height: 0, cols: 0, rows: 0, cells: [], fps: 0 },
    uptime_secs: 0,
    running_time_ms: 0,
    backend: null,
    ad: null,
    tally: {},
    meters: { program: [], sources: {} },
    alerts: [],
    layout: null,
    seq: 0,
    connected: false,
  };
}

export type Listener = (state: State) => void;

export class Store {
  state: State = emptyState();
  private listeners = new Set<Listener>();
  private dirty = false;

  /** Register for the flush tick. Returns an unsubscribe function. */
  subscribe(fn: Listener): () => void {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  }

  /** Replace the status document wholesale. Used by `event/snapshot`. */
  snapshot(status: Partial<MixerStatus>, seq?: number): void {
    const keep = { meters: this.state.meters, alerts: this.state.alerts, tally: this.state.tally };
    this.state = Object.assign(emptyState(), keep, status, {
      seq: seq ?? this.state.seq,
      connected: true,
    }) as State;
    this.dirty = true;
  }

  /** Shallow merge at the top level. Used by everything else. */
  patch(fields: Partial<State>): void {
    Object.assign(this.state, fields);
    this.dirty = true;
  }

  /** Replace one source in place, matched by id. True if it was found. */
  patchSource(id: string, fields: Partial<SourceStatus>): boolean {
    const i = this.state.sources.findIndex((s) => s.id === id);
    if (i < 0) return false;
    this.state.sources[i] = Object.assign({}, this.state.sources[i], fields) as SourceStatus;
    this.dirty = true;
    return true;
  }

  patchOutput(id: string, fields: Partial<OutputStatus>): boolean {
    const i = this.state.outputs.findIndex((o) => o.id === id);
    if (i < 0) return false;
    this.state.outputs[i] = Object.assign({}, this.state.outputs[i], fields) as OutputStatus;
    this.dirty = true;
    return true;
  }

  /** Meters arrive ten times a second and are kept out of the flush path. */
  setMeters(program?: number[], sources?: Record<string, unknown>): void {
    if (program) this.state.meters.program = program;
    if (sources) Object.assign(this.state.meters.sources, sources);
  }

  addAlert(alert: AlertEvent): void {
    this.state.alerts.unshift(Object.assign({ at: Date.now() }, alert));
    if (this.state.alerts.length > 50) this.state.alerts.length = 50;
    this.dirty = true;
  }

  /** Tell every listener, once, if anything changed since the last flush. */
  flush(force?: boolean): void {
    if (!this.dirty && !force) return;
    this.dirty = false;
    for (const fn of this.listeners) {
      try {
        fn(this.state);
      } catch (e) {
        console.error("a surface threw while rendering", e);
      }
    }
  }

  source(id: string): SourceStatus | null {
    return this.state.sources.find((s) => s.id === id) || null;
  }

  output(id: string): OutputStatus | null {
    return this.state.outputs.find((o) => o.id === id) || null;
  }

  /**
   * "program", "preview" or "off" for one source.
   *
   * Answered from `event/tally` when the surface asked for it, and worked out
   * from the programme otherwise, so a surface that declined the tally stream
   * still colours its buttons.
   */
  tallyOf(id: string): string {
    if (this.state.tally && this.state.tally[id]) return this.state.tally[id]!;
    if (this.state.program === id) return "program";
    if (this.state.preview === id) return "preview";
    return "off";
  }
}
