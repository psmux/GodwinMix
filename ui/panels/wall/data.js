// What the wall knows and how it keeps it current. show.list once and on
// show events; show.stats once a second for the rows on screen and no
// others; governor.status every few seconds for the header. Nothing runs
// while the tab is hidden, and nothing at all once the wall closes.

const STATS_MS = 1000;
const GOVERNOR_MS = 3000;
const HISTORY = 40;

export class WallData {
  constructor(client, changed) {
    this.client = client;
    this.changed = changed;
    this.shows = [];
    this.stats = new Map();
    this.history = new Map();
    this.gov = null;
    this.ids = [];
    this.missing = false;
    this.loaded = false;
    this.busy = false;
    this.offs = [];
    this.timers = [];
  }

  start() {
    const c = this.client;
    this.offs.push(c.listen ? c.listen("show.*") : () => {});
    this.offs.push(c.on("event", (e) => this.event(e)));
    this.offs.push(c.on("open", () => this.read()));
    this.timers.push(setInterval(() => this.tick(), STATS_MS));
    this.timers.push(setInterval(() => this.governor(), GOVERNOR_MS));
    this.read();
    this.governor();
  }

  stop() {
    for (const off of this.offs) off();
    for (const t of this.timers) clearInterval(t);
    this.offs = [];
    this.timers = [];
    this.stopped = true;
  }

  async read() {
    try {
      const got = await this.client.call("show.list", {});
      this.shows = (got && got.shows) || [];
      this.current = got && got.current;
    } catch (e) {
      if (e && e.code === -32601) this.missing = true;
      else console.debug("show.list", e);
    }
    this.loaded = true;
    this.changed();
  }

  /** The view says which shows are on screen; they are read now, not in a second. */
  visible(ids) {
    const fresh = ids.some((id) => !this.stats.has(id));
    this.ids = ids;
    if (fresh) this.tick();
  }

  async tick() {
    if (this.busy || this.stopped || !this.ids.length || document.hidden) return;
    this.busy = true;
    const ids = this.ids.slice();
    try {
      const got = await this.client.call("show.stats", { ids });
      for (const s of (got && got.shows) || []) this.take(s);
      this.changed();
    } catch (e) {
      if (e && e.code !== -32601) console.debug("show.stats", e);
    } finally {
      this.busy = false;
    }
  }

  take(s) {
    this.stats.set(s.id, s);
    const h = this.history.get(s.id) || [];
    h.push((s.input && s.input.kbps) || 0);
    if (h.length > HISTORY) h.shift();
    this.history.set(s.id, h);
    if (s.health) this.health(s.id, s.health);
  }

  async governor() {
    if (this.stopped || document.hidden) return;
    try {
      this.gov = await this.client.call("governor.status", {});
      this.changed();
    } catch {
      this.gov = null;
    }
  }

  health(id, health) {
    const show = this.shows.find((s) => s.id === id);
    if (show) show.health = health;
  }

  event({ name, params }) {
    if (!/^show\./.test(name || "")) return;
    const p = params || {};
    if (name === "show.health") this.health(p.id, p.health);
    else if (name === "show.removed") this.shows = this.shows.filter((s) => s.id !== p.id);
    else if (p.show) {
      const at = this.shows.findIndex((s) => s.id === p.show.id);
      if (at < 0) this.shows.push(p.show);
      else this.shows[at] = { ...this.shows[at], ...p.show };
    } else return this.read();
    this.changed();
  }

  find(id) {
    return this.shows.find((s) => s.id === id);
  }

  /** show.set, with the row changed at once and put back if refused. */
  async set(id, patch) {
    const show = this.find(id);
    const before = show && { ...show };
    if (show) Object.assign(show, patch);
    this.changed();
    try {
      const got = await this.client.call("show.set", { id, ...patch });
      if (show && got && got.id === id) Object.assign(show, got);
      return got;
    } catch (e) {
      if (show) Object.assign(show, before);
      this.changed();
      throw e;
    }
  }
}
