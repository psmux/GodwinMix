// One per page: what the ptz and atem plugins say, shared by the source
// tiles, camera properties, the Preview overlay, the Programme header's
// switcher chips and the Outputs panel. Each of them asks for what it shows
// with want(topic) and gives it back when it goes, and hub-read.js reads only
// what somebody still wants:
//
//   "cams"          the camera list and each camera's tracking state
//   "people:<cam>"  who one camera sees, about eight times a second
//   "atem"          the switcher list and each switcher's state
//
// With neither plugin installed it asks plugin.list now and then and nothing
// else. The demo (demo/client.js) stands in for both plugins when somebody
// asks for it in camera properties, and this browser remembers that.

import { cameraApi } from "./api.js";
import { reader } from "./hub-read.js";

const hubs = new WeakMap();
const KEY = "gmx.cameras.demo";

export function hubFor(client) {
  let hub = hubs.get(client);
  if (!hub) hubs.set(client, (hub = new Hub(client)));
  return hub;
}

function remembered() {
  try {
    return JSON.parse(localStorage.getItem(KEY) || "null");
  } catch {
    return null;
  }
}

function remember(value) {
  try {
    if (value) localStorage.setItem(KEY, JSON.stringify(value));
    else localStorage.removeItem(KEY);
  } catch {
    /* a private window keeps nothing, which is fine */
  }
}

class Hub {
  constructor(client) {
    this.real = client;
    this.demo = null;
    this.has = { ptz: null, atem: null };
    this.wants = new Map();
    this.listeners = new Set();
    this.reset(client);
    this.read = reader(this);
    const saved = remembered();
    if (saved) this.ready = this.setDemo(true, saved.sources);
  }

  reset(client) {
    this.api = cameraApi(client);
    this.cams = [];
    this.track = new Map();
    this.seen = new Map();
    this.presets = new Map();
    this.atems = [];
    this.atemState = new Map();
    this.loaded = false;
  }

  on(fn) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  changed(what) {
    for (const fn of [...this.listeners]) fn(what);
  }

  /** Ask for a topic; the answer is a function that gives it back. */
  want(topic) {
    this.wants.set(topic, (this.wants.get(topic) || 0) + 1);
    this.read.start();
    let given = false;
    return () => {
      if (given) return;
      given = true;
      const n = (this.wants.get(topic) || 1) - 1;
      if (n) this.wants.set(topic, n);
      else this.wants.delete(topic);
      if (!this.wants.size) this.read.stop();
    };
  }

  wanted(prefix) {
    for (const k of this.wants.keys()) if (k === prefix || k.startsWith(prefix + ":")) return true;
    return false;
  }

  /** Cameras answer: the ptz plugin is installed, or the demo is on. */
  get cameras() {
    return !!(this.demo || this.has.ptz);
  }

  get switchers() {
    return !!(this.demo || this.has.atem);
  }

  cam(id) {
    return this.cams.find((c) => c.id === id) || null;
  }

  /** The camera that points at a GodwinMix source, or null. */
  cameraFor(source) {
    return this.cams.find((c) => c.source === source) || null;
  }

  /** Which plugins are installed, asked again after `maxAge` ms. */
  async plugins(maxAge = 30000) {
    if (this.askedAt && Date.now() - this.askedAt < maxAge) return this.has;
    this.askedAt = Date.now();
    const list = await this.real.call("plugin.list", {}).catch(() => null);
    const names = new Set(((list && list.plugins) || []).map((p) => p.name));
    const was = this.has.ptz + "/" + this.has.atem;
    this.has = { ptz: names.has("ptz"), atem: names.has("atem") };
    if (was !== this.has.ptz + "/" + this.has.atem) this.changed("plugins");
    return this.has;
  }

  /** The demo on or off. `sources` are the GodwinMix sources it stands in for. */
  async setDemo(on, sources) {
    this.offProgramme?.();
    this.offProgramme = null;
    this.demo?.stop();
    this.demo = null;
    if (on) {
      const { demoClient } = await import("./demo/client.js");
      this.demo = demoClient({ sources });
      // A GodwinMix take reaches the pretend switchers, so Follows can be seen.
      let last = null;
      this.offProgramme = this.real.onRender ? this.real.onRender((s) => {
        if (this.demo && s.program !== last) (last = s.program), this.demo.call("program.take", { source: s.program || "" });
      }) : null;
      if (this.wants.size) this.demo.start();
    }
    remember(on ? { sources } : null);
    this.reset(this.demo || this.real);
    this.changed("demo");
    await this.read.now();
  }

  async loadPresets(id) {
    this.presets.set(id, await this.api.presets(id).catch(() => []));
    this.changed("presets");
  }

  /** Read again now, after a command, rather than at the next tick. */
  refresh() {
    return this.read.now();
  }
}
