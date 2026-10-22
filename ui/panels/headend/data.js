// What the headend knows: the wall's show list and events, with show.stats
// for every channel once a second (one call, from numbers the station
// already holds, so two hundred cost no more than one), and for each
// channel two minutes of rates, continuity errors and sound peak, and a
// log of what happened to it since this page opened. The pictures are not
// here: thumbs.js asks for them only for tiles on screen.

import { WallData } from "../wall/data.js";

export const KEEP = 120;
const LOG = 60;

const push = (arr, v) => { arr.push(v); if (arr.length > KEEP) arr.shift(); };

/** One channel's history. */
export class Series {
  constructor() {
    this.inK = [];
    this.outK = [];
    this.cc = [];
    this.peak = [];
    this.lastCc = null;
    this.log = [];
  }

  take(st, now) {
    const i = st.input || {};
    push(this.inK, i.kbps || 0);
    push(this.outK, (st.outputs || []).reduce((n, o) => n + (o.kbps || 0), 0));
    const total = (i.cc_errors || 0) + (i.packets_lost || 0);
    const jump = this.lastCc == null ? 0 : Math.max(0, total - this.lastCc);
    this.lastCc = total;
    push(this.cc, jump);
    push(this.peak, i.audio_peak_db ?? null);
    if (jump) this.note(now, "cc", `${jump} continuity ${jump === 1 ? "error" : "errors"} or lost packets`);
  }

  note(at, kind, text) {
    this.log.unshift({ at, kind, text });
    if (this.log.length > LOG) this.log.pop();
  }

  /** Errors in the last minute. */
  ccMinute() {
    return this.cc.slice(-60).reduce((n, v) => n + v, 0);
  }
}

const ALARM_WORDS = {
  "no-input": "No input", stall: "Input stalled", black: "Black picture", freeze: "Frozen picture", silence: "Silence",
  "cc-errors": "Continuity errors", loss: "Packet loss", "output-failed": "Output failed", scrambled: "Scrambled",
  "governor-refused": "Encode refused", shed: "Encode shed", memory: "Memory",
};

export class HeadendData extends WallData {
  constructor(client, changed) {
    super(client, changed);
    this.series = new Map();
  }

  seriesOf(id) {
    if (!this.series.has(id)) this.series.set(id, new Series());
    return this.series.get(id);
  }

  /** The channels: shows with compositing off. A show that mixes is not one. */
  async read() {
    await super.read();
    this.shows = this.shows.filter((s) => s.compositing === false);
    this.ids = this.shows.map((s) => s.id);
    this.tick();
  }

  event(e) {
    super.event(e);
    this.shows = this.shows.filter((s) => s.compositing === false);
  }

  /** Every channel, not only those on screen: the groups and totals need them all. */
  async tick() {
    this.ids = this.shows.map((s) => s.id);
    return super.tick();
  }

  take(s) {
    super.take(s);
    this.seriesOf(s.id).take(s, Date.now());
  }

  /** A health change, logged: each alarm raised and each one cleared. */
  health(id, health) {
    const show = this.find(id);
    const was = new Set(((show && show.health && show.health.alarms) || []).map((a) => a.kind));
    const now = new Set(((health && health.alarms) || []).map((a) => a.kind));
    const s = this.seriesOf(id);
    for (const a of (health && health.alarms) || []) if (!was.has(a.kind)) s.note(Date.now(), "raised", `${ALARM_WORDS[a.kind] || a.kind}: ${a.detail}`);
    for (const k of was) if (!now.has(k)) s.note(Date.now(), "cleared", `${ALARM_WORDS[k] || k} cleared`);
    super.health(id, health);
  }

  /** show.stop then show.start, or either alone. */
  async power(id, what) {
    if (what !== "start") await this.client.call("show.stop", { id });
    if (what !== "stop") await this.client.call("show.start", { id });
    this.seriesOf(id).note(Date.now(), "action", what === "stop" ? "Disabled from the headend view" : what === "start" ? "Enabled from the headend view" : "Restarted from the headend view");
    await this.read();
  }
}
