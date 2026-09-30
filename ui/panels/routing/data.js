// What the routing view knows, and only as much as is on screen.
//
// `channel.list` and `show.list` are asked once when the view opens and again
// on their events: both are one small answer from the station. The expensive
// part is per group: a show's outputs, sources and plan, a channel's plan.
// Those are asked for when the group scrolls into view, followed while it
// stays there, and let go a few seconds after it leaves (AGENTS.md rule 1).

import { showLink } from "./link.js";
import { groups } from "./model.js";

const NO_METHOD = -32601;
const LET_GO_MS = 3000;

export class RoutingData {
  constructor(client, onChange) {
    this.client = client;
    this.onChange = onChange;
    this.channels = [];
    this.shows = [];
    this.current = null;
    this.detail = {};
    this.plans = {};
    this.links = new Map();
    this.timers = new Map();
    this.onscreen = new Set();
    this.offs = [];
  }

  async start() {
    const c = this.client;
    this.offs.push(c.listen ? c.listen("channel.*") : () => {}, c.listen ? c.listen("show.*") : () => {});
    this.offs.push(c.on("event", ({ name }) => {
      if (/^channel\./.test(name)) this.readChannels();
      if (/^show\./.test(name)) this.readShows();
    }));
    await Promise.all([this.readChannels(), this.readShows()]);
  }

  changed() {
    if (!this.stopped) this.onChange();
  }

  groups() {
    return groups(this.channels, this.shows.map((s) => ({ ...s, detail: this.detail[s.id] || null })));
  }

  async readChannels() {
    try {
      this.channels = (await this.client.call("channel.list", {})).channels || [];
    } catch (e) {
      if (e && e.code !== NO_METHOD) console.debug("channel.list", e);
      this.channels = [];
    }
    this.changed();
  }

  /** The station's shows, or this mixer as the one show on a core without them. */
  async readShows() {
    try {
      const a = await this.client.call("show.list", {});
      this.shows = a.shows || [];
      this.current = new URLSearchParams(location.search).get("show") || a.current;
    } catch (e) {
      if (!e || e.code !== NO_METHOD) console.debug("show.list", e);
      const s = this.client.state || {};
      this.shows = [{ id: "", name: "This mixer", state: "running", on_air: s.scene || s.program || null }];
      this.current = "";
    }
    this.changed();
    // A show that started while its group was on screen is read now, and
    // one that stopped lets go of its line.
    for (const g of this.groups()) {
      if (g.kind !== "show" || !this.onscreen.has(g.key)) continue;
      if (g.state === "running") this.want(g);
      else this.release(g.key);
    }
  }

  /** A group came on screen or went off it. */
  visible(group, on) {
    clearTimeout(this.timers.get(group.key));
    if (on) this.onscreen.add(group.key);
    else this.onscreen.delete(group.key);
    if (on) return this.want(group);
    this.timers.set(group.key, setTimeout(() => this.release(group.key), LET_GO_MS));
  }

  want(group) {
    if (this.links.has(group.key) || this.stopped) return;
    if (group.kind === "show" && group.state !== "running") return;
    const id = group.kind === "show" ? group.id : this.current;
    const link = showLink(this.client, id, this.current);
    this.links.set(group.key, link);
    const read = group.kind === "show" ? () => this.readShow(group.id, link) : () => this.readPlan(group.key, link, { scope: `channel:${group.id}` });
    let soon = null;
    const again = () => { clearTimeout(soon); soon = setTimeout(read, 150); };
    const off = link.on((name, params) => {
      if (name === "rendition.plan" && group.kind === "channel" && params && params.scope === `channel:${group.id}`) {
        this.plans[group.key] = params.plan;
        return this.changed();
      }
      if (name === "tally" && link.client !== this.client) link.tally = (params && params.sources) || {};
      if (group.kind === "show" && /^(output|source|rendition)\.|^tally$/.test(name)) again();
    });
    link.read = read;
    const close = link.close;
    link.close = () => { clearTimeout(soon); off(); close(); };
    read();
  }

  /** Read a group again now, after something on it was changed from here. */
  refresh(key) {
    const link = this.links.get(key);
    if (link && link.read) link.read();
  }

  release(key) {
    const link = this.links.get(key);
    if (!link) return;
    this.links.delete(key);
    link.close();
  }

  async readPlan(key, link, params) {
    try {
      this.plans[key] = await link.call("rendition.plan", params);
      this.changed();
    } catch (e) {
      if (e && e.code !== NO_METHOD) console.debug("rendition.plan", e);
    }
  }

  async readShow(id, link) {
    const quiet = (p) => p.catch((e) => (e && e.code !== NO_METHOD && console.debug("routing", e), null));
    const [outputs, sources, plan] = await Promise.all([
      quiet(link.call("output.list", {})),
      quiet(link.call("source.list", {})),
      quiet(link.call("rendition.plan", {})),
    ]);
    const s = link.client && link.client.state;
    const tally = { ...(s && s.tally) };
    if (s && s.program && !tally[s.program]) tally[s.program] = "program";
    this.detail[id] = {
      outputs: list(outputs, "outputs"),
      sources: list(sources, "sources"),
      tally: link.tally || tally,
    };
    if (plan) this.plans[`s:${id}`] = plan;
    this.changed();
  }

  stop() {
    this.stopped = true;
    for (const key of [...this.links.keys()]) this.release(key);
    for (const t of this.timers.values()) clearTimeout(t);
    for (const off of this.offs) off();
  }
}

/** `/rpc` answers with the array; some answers wrap it in an object. */
function list(answer, field) {
  if (Array.isArray(answer)) return answer;
  return (answer && answer[field]) || [];
}
