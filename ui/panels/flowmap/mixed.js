// What a mixed show is doing, for its part of the map: its sources, its
// outputs, what is on air and armed, and the renditions it makes. The show
// this page is on is read from the page's own state, which is already
// kept current; any other is asked over a line of its own (routing's
// link.js), every two seconds while the map is open, and the line closes
// when the map does or the show stops. An output's rate is worked out from
// how many bytes it has sent between two reads.

import { showLink } from "../routing/link.js";

const EVERY_MS = 2000;
const MOST = 12;

const quiet = (p) => p.catch(() => null);

export class MixedWatch {
  constructor(client, changed) {
    this.client = client;
    this.changed = changed;
    this.links = new Map();
    this.detail = new Map();
    this.sent = new Map();
    this.current = "";
  }

  start() {
    this.timer = setInterval(() => this.tick(), EVERY_MS);
  }

  stop() {
    clearInterval(this.timer);
    for (const l of this.links.values()) l.close();
    this.links.clear();
    this.stopped = true;
  }

  /** Follow the mixed shows that run, and let go of the rest. */
  follow(shows, current) {
    if (this.stopped) return;
    this.current = current;
    const want = shows.filter((s) => s.compositing !== false && (s.state === "running" || s.state === "live" || s.id === current)).slice(0, MOST).map((s) => s.id);
    for (const [id, l] of this.links) if (!want.includes(id)) (l.close(), this.links.delete(id), this.detail.delete(id));
    const fresh = want.filter((id) => !this.links.has(id));
    for (const id of fresh) this.links.set(id, showLink(this.client, id, current));
    if (fresh.length) this.tick();
  }

  async tick() {
    if (this.busy || this.stopped || document.hidden || !this.links.size) return;
    this.busy = true;
    this.round = (this.round || 0) + 1;
    try {
      await Promise.all([...this.links].map(([id, l]) => this.read(id, l)));
      this.changed();
    } finally {
      this.busy = false;
    }
  }

  async read(id, link) {
    // The status for every show, this page's too, since what an output has
    // sent is counted there; what is armed and the scene's own name only this
    // page's state knows.
    const page = id === this.current;
    const got = await quiet(link.call("core.status", {}));
    const own = page ? this.client.state || {} : {};
    const s = got ? { ...got, preview: own.preview, sceneName: own.sceneName, tally: own.tally || got.tally } : page ? own : null;
    if (!s) return;
    const was = this.detail.get(id) || {};
    // The plan changes only when an output does: every third read is enough.
    const plan = !was.plan || this.round % 3 === 0 ? await quiet(link.call("rendition.plan", {})) : null;
    const sources = s.sources || [];
    const named = (v) => (sources.find((x) => x.id === v) || {}).name || v;
    this.detail.set(id, {
      sources, outputs: s.outputs || [], program: s.program, scene: s.scene, sceneName: s.sceneName || null,
      preview: s.preview, previewName: s.preview ? named(s.preview) : null, tally: s.tally || {}, multiview: s.multiview, backend: s.backend,
      uptime_secs: s.uptime_secs, plan: plan || was.plan || null, rates: this.rates(id, s.outputs || []),
    });
  }

  /** kbit/s per output, from `bytes_out` between this read and the one before. */
  rates(id, outputs) {
    const now = performance.now();
    const before = this.sent.get(id) || new Map();
    const after = new Map();
    const out = new Map();
    for (const o of outputs) {
      if (o.bytes_out == null) continue;
      after.set(o.id, { b: o.bytes_out, t: now });
      const p = before.get(o.id);
      if (p && now > p.t + 200 && o.bytes_out >= p.b) out.set(o.id, Math.round(((o.bytes_out - p.b) * 8) / (now - p.t)));
      else if (p && p.k) out.set(o.id, p.k);
      if (out.has(o.id)) after.get(o.id).k = out.get(o.id);
    }
    this.sent.set(id, after);
    return out;
  }
}
