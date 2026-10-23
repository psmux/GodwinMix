// The box: gpu.status and machine.status once a second while the headend
// view is open and the tab is visible. The station reads the GPU only
// while something asks, so closing the view stops that too, ten seconds
// later. Each gpu.status after the first asks only for what is new.
// headend.status, which Flussonic config the station follows, every ten.

import { noMethod } from "../wall/data.js";

const EVERY_MS = 1000;
const KEEP = 120;

export class MachineWatch {
  constructor(client, changed) {
    this.client = client;
    this.changed = changed;
    this.gpu = null;
    this.box = null;
    this.missing = false;
    this.cpu = [];
    this.busy = false;
    this.follow = null;
    this.n = 0;
  }

  start() {
    this.timer = setInterval(() => this.tick(), EVERY_MS);
    this.tick();
  }

  stop() {
    clearInterval(this.timer);
    this.stopped = true;
  }

  async tick() {
    if (this.busy || this.stopped || this.missing || document.hidden) return;
    this.busy = true;
    try {
      const [gpu, box] = await Promise.all([this.read("gpu.status", this.since()), this.read("machine.status", {}), this.followed()]);
      this.merge(gpu);
      if (box) {
        this.box = box;
        if (box.cpu_pct != null) this.cpu.push(box.cpu_pct), this.cpu.length > KEEP && this.cpu.shift();
      }
      this.changed();
    } finally {
      this.busy = false;
    }
  }

  async read(method, params) {
    try {
      return await this.client.call(method, params);
    } catch (e) {
      // A station from before these methods: say so once, and stop asking.
      if (noMethod(e)) this.missing = true;
      return null;
    }
  }

  /** Which Flussonic config the station follows, if any. A station without the method follows none. */
  async followed() {
    if (this.n++ % 10) return;
    this.follow = await this.client.call("headend.status", {}).catch(() => null);
  }

  since() {
    const last = this.gpu && this.gpu.gpus.map((g) => g.now.at_ms).reduce((a, b) => Math.max(a, b), 0);
    return last ? { since_ms: last } : {};
  }

  /** Fold a new answer into what is held: new history on the end, two minutes kept. */
  merge(got) {
    if (!got) return;
    const held = new Map(((this.gpu && this.gpu.gpus) || []).map((g) => [g.index, g]));
    for (const g of got.gpus || []) {
      const was = held.get(g.index);
      const history = (was ? was.history.concat(g.history || []) : g.history || []).slice(-KEEP);
      held.set(g.index, { ...g, history });
    }
    for (const k of [...held.keys()]) if (!(got.gpus || []).some((g) => g.index === k)) held.delete(k);
    this.gpu = { ...got, gpus: [...held.values()].sort((a, b) => a.index - b.index) };
  }
}
