// The Channels panel: every channel as a card, what is publishing to it (by
// RTMP, RTMPS, SRT or WHIP) and where it is being sent on to, and one small
// line saying which ingest ports are open and for which channels.
//
// Loaded the first time its tab is shown (entry.js is the part the page
// loads). While it is on screen it asks the core for `channel.*` events, and
// while something is live it reads `channel.list` every two seconds for the
// bit rates, which no event carries, and every three seconds each live
// stream's picture. The moment it is hidden it stops all three, so a mixer
// with nobody looking at channels measures and decodes nothing for anybody.

import { el, clear } from "../../shell/dom.js";
import { toast } from "../../shell/toast.js";
import { leaves } from "../../shell/dock-model.js";
import { Channels, isLive } from "./model.js";
import { keyed, write } from "./keyed.js";
import { channelCard } from "./card.js";
import { emptyArt, failed } from "./art.js";
import { installCard } from "./install.js";
import { openPorts, portProblems } from "./ways.js";
import { ChannelPlans } from "./plans.js";
import { channelLayout } from "./layout.js";

import { addChannel } from "./create.js";
import { importLivebox } from "./livebox.js";
import { contextMenu } from "../../shell/menu.js";

export { addChannel, importLivebox };

const CSS_ID = "gmx-channels-css";
/** Seconds between two readings of the bit rates while something is live. */
const RATE_TICKS = 2;
/** Seconds between two pictures of each live stream. */
const PICTURE_TICKS = 3;

/** The view on screen now, so a channel made from elsewhere lands in it. */
export let current = null;

/** Called by entry.js once the module is here. */
export function mount(host) {
  if (!host.view) host.view = new ChannelsView(host, host.client);
  if (host.isConnected) host.view.start();
}

export function stylesheet() {
  if (document.getElementById(CSS_ID)) return;
  document.head.appendChild(el("link#" + CSS_ID, { rel: "stylesheet", href: new URL("./channels.css", import.meta.url).href }));
}

export class ChannelsView {
  constructor(host, client) {
    stylesheet();
    this.client = client;
    this.model = new Channels();
    this.plans = new ChannelPlans(client, () => this.render());
    this.cards = new Map();
    this.list = el("div.chn-list");
    this.layout = channelLayout(this.list, () => this.cards);
    this.count = el("span.chn-count");
    const add = (this.addButton = el("button.btn.primary.chn-add", { text: "Add Channel", onclick: () => this.add() }));
    const more = el("button.btn.chn-topmore", { type: "button", text: "⋯", title: "More ways to add channels", "aria-label": "More ways to add channels" });
    more.onclick = () => {
      const r = more.getBoundingClientRect();
      contextMenu(r.left, r.bottom + 4, [{ label: "Bring channels from Livebox", run: () => importLivebox(this.client) }]);
    };
    // What is open, in the text's own colour; a port a channel wants that
    // would not open is the only part said in amber.
    this.portsOpen = el("span");
    this.portsBad = el("span.bad");
    this.ports = el("p.chn-ports", { role: "status" }, [this.portsOpen, " ", this.portsBad]);
    this.head = el("header.chn-top", {}, [
      el("div.grow", {}, [
        el("h2", {}, ["Channels ", this.count]),
        el("p.chn-lede", { text: "Encoders publish to the mixer by RTMP, SRT or WHIP. Each channel can go on air and on to the platforms." }),
        this.ports,
      ]),
      more,
      this.layout.switch,
      add,
    ]);
    this.root = el("div.chn", {}, [this.head, this.list, this.layout.rows]);
    host.appendChild(this.root);
  }

  add() {
    return addChannel(this.client);
  }

  start() {
    if (this.running) return;
    this.running = true;
    current = this;
    this.release = this.client.listen ? this.client.listen("channel.*") : () => {};
    this.offs = [
      this.client.on("event", (e) => this.event(e)),
      // A socket that dropped missed whatever changed while it was down.
      this.client.on("open", () => this.load()),
    ];
    this.load();
  }

  stop() {
    if (!this.running) return;
    this.running = false;
    if (current === this) current = null;
    this.release();
    for (const off of this.offs) off();
    this.plans.stop();
    this.clock(false);
  }

  async load() {
    try {
      this.model.load(await this.client.call("channel.list", {}));
      this.missing = false;
      this.failure = "";
    } catch (e) {
      this.missing = e && e.code === -32601;
      this.failure = this.missing ? "" : (e && e.message) || String(e);
    }
    this.render();
  }

  event({ name, params }) {
    if (name === "channel.changed") this.accept(params.channel);
    else if (name === "channel.removed") {
      this.model.remove(params.id);
      this.render();
    } else if (name === "channel.refused") {
      const who = params.from ? ` from ${params.from}` : "";
      toast({ kind: "warning", text: `A publisher${who} was turned away from ${params.id}: ${params.why}` });
    }
  }

  /** A channel as a call or an event just described it. */
  accept(channel) {
    this.model.put(channel);
    this.render();
  }

  render() {
    const channels = this.model.list();
    // Only what the panel first opens on counts, not a channel made later.
    const first = !this.opened && !this.missing && !this.failure;
    if (first) this.opened = true;
    this.count.textContent = channels.length ? String(channels.length) : "";
    if (this.missing || this.failure || !channels.length) {
      this.cards.clear();
      clear(this.list);
      this.list.appendChild(this.missing ? installCard(this.client, () => this.load()) : this.failure ? failed(this.failure, () => this.load()) : emptyArt(() => this.add()));
      // The empty picture has its own big button; one is enough.
      this.head.hidden = !this.missing && !this.failure;
      // Without the plugin there is nothing to add a channel to.
      this.addButton.hidden = this.missing;
      this.layout.update([]);
      this.clock(false);
      return;
    }
    this.head.hidden = false;
    this.addButton.hidden = false;
    const problems = portProblems(this.model.listeners);
    write(this.portsOpen, "textContent", openPorts(this.model.listeners));
    write(this.portsBad, "textContent", problems.join(" "));
    if (!this.cards.size) clear(this.list);
    keyed(this.list, this.cards, channels, (c) => c.id, (c) => channelCard(this, c), (c) => (c.enabled ? "on" : "off"));
    this.layout.update(channels);
    if (first) this.openOnDefault(channels);
    if (this.running) this.plans.sync(channels);
    this.clock(channels.some(isLive));
  }

  /**
   * A panel that opens on one channel with nothing publishing to it (a new
   * mixer's default `live`) opens its Connect, so the server and the key are
   * there to copy into OBS.
   */
  openOnDefault(channels) {
    if (channels.length !== 1 || isLive(channels[0]) || !channels[0].enabled) return;
    this.cards.get(channels[0].id)?.connect(true);
  }

  /** Uptime moves every second while something is live, and every second
   * tick the numbers are read again. */
  clock(wanted) {
    if (!wanted) {
      clearInterval(this.timer);
      this.timer = null;
      return;
    }
    if (!this.timer) this.timer = setInterval(() => this.tick(), 1000);
  }

  tick() {
    for (const card of this.cards.values()) card.tick?.();
    this.layout.tick();
    this.ticks = (this.ticks || 0) + 1;
    if (this.ticks % RATE_TICKS === 0) this.poll();
    if (this.ticks % PICTURE_TICKS === 0 && !document.hidden) for (const card of this.cards.values()) card.picture?.();
  }

  /** Read the numbers again, quietly: a failure here waits for the next one. */
  async poll() {
    if (this.polling || !this.running) return;
    this.polling = true;
    try {
      this.model.load(await this.client.call("channel.list", {}));
      this.render();
    } catch {
      /* the next tick asks again */
    } finally {
      this.polling = false;
    }
  }
}

/** Make the Channels tab the one on show, wherever it has been docked. */
export function bringForward() {
  const ws = document.querySelector("gmx-shell")?.workspace;
  if (!ws) return;
  const group = leaves(ws.state.tree).find((g) => g.tabs.includes("core/channels"));
  if (group) ws.activate(group, "core/channels");
  else ws.show("core/channels");
}
