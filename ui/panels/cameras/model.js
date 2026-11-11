// What the camera window knows, and the reading that keeps it current. It
// reads only between start() and stop(), which the panel calls while it is
// on screen: the people in each picture about eight times a second, the
// tracking state and the switcher twice a second, the camera list every
// five. Nothing here draws.

import { cameraApi } from "./api.js";

const SLOW = 40; // ticks of 125 ms: the camera and switcher lists
const MID = 4; // tracking state and switcher state

export class Model {
  constructor(client) {
    this.listeners = new Set();
    this.mix = {};
    this.n = 0;
    this.setClient(client);
  }

  setClient(client) {
    this.api = cameraApi(client);
    this.cams = [];
    this.next = null;
    this.seen = new Map();
    this.resting = new Map();
    this.track = new Map();
    this.presets = new Map();
    this.atems = [];
    this.atemId = null;
    this.atem = null;
    this.loaded = false;
    this.error = null;
    this.n = 0;
  }

  on(fn) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  changed(what) {
    for (const fn of this.listeners) fn(what);
  }

  cam(id) {
    return this.cams.find((c) => c.id === id) || null;
  }

  get current() {
    return this.cam(this.next) || this.cams[0] || null;
  }

  start() {
    if (this.timer) return;
    this.timer = setInterval(() => this.tick(), 125);
    this.n = 0;
    return this.tick();
  }

  stop() {
    clearInterval(this.timer);
    this.timer = null;
  }

  async tick() {
    // A slow plugin must not stack reads up behind itself.
    if (this.busy) return;
    this.busy = true;
    const n = this.n++;
    try {
      if (n % SLOW === 0) await this.lists();
      if (n % MID === 0) await this.states();
      await this.people();
    } catch (e) {
      if (!this.error) console.warn("the camera window could not read", e);
      this.error = e;
      this.changed("error");
    } finally {
      this.busy = false;
    }
  }

  async lists() {
    const [cams, atems] = await Promise.all([this.api.cameras(), this.api.atems()]);
    this.cams = cams;
    this.atems = atems;
    if (!this.atems.some((a) => a.id === this.atemId)) this.atemId = (atems[0] || {}).id || null;
    if (!cams.some((c) => c.id === this.next)) this.next = (cams[1] || cams[0] || {}).id || null;
    await Promise.all(cams.map((c) => this.loadPresets(c.id)));
    this.loaded = true;
    this.error = null;
    this.changed("lists");
  }

  async loadPresets(id) {
    this.presets.set(id, await this.api.presets(id).catch(() => []));
    this.changed("presets");
  }

  async states() {
    const tracks = await Promise.all(this.cams.map((c) => this.api.trackState(c.id).catch(() => null)));
    this.cams.forEach((c, i) => this.track.set(c.id, tracks[i]));
    this.atem = this.atemId ? await this.api.atemState(this.atemId).catch(() => null) : null;
    this.changed("state");
  }

  async people() {
    // A camera whose picture cannot be read (no source yet) is asked again
    // every five seconds rather than eight times a second.
    const now = Date.now();
    const ask = (c) => (this.resting.get(c.id) > now ? null : this.api.observe(c.id).catch(() => (this.resting.set(c.id, now + 5000), null)));
    const seen = await Promise.all(this.cams.map(ask));
    this.cams.forEach((c, i) => this.seen.set(c.id, seen[i]));
    this.changed("people");
  }

  /** Read the switcher again now, after a command, rather than at the next tick. */
  async refresh() {
    await this.states().catch(() => {});
  }

  pick(id) {
    if (this.next === id) return;
    this.next = id;
    this.changed("pick");
  }

  pickAtem(id) {
    this.atemId = id;
    this.refresh();
  }
}
