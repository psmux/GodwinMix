// The Channels panel: every RTMP channel as a card, what is publishing to it
// and where it is being sent on to.
//
// Loaded the first time its tab is shown (entry.js is the part the page
// loads). While it is on screen it asks the core for `channel.*` events and
// nothing else; the moment it is hidden it gives them back, so a mixer with
// nobody looking at channels sends nobody anything about them.

import { el, clear } from "../../shell/dom.js";
import { toast } from "../../shell/toast.js";
import { leaves } from "../../shell/dock-model.js";
import { Channels, isLive } from "./model.js";
import { keyed } from "./keyed.js";
import { channelCard } from "./card.js";
import { emptyArt } from "./art.js";
import { installCard } from "./install.js";

import { addChannel } from "./create.js";

export { addChannel };

const CSS_ID = "gmx-channels-css";

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
    this.cards = new Map();
    this.list = el("div.chn-list");
    this.count = el("span.chn-count");
    const add = el("button.btn.primary.chn-add", { text: "Add RTMP Channel", onclick: () => this.add() });
    this.head = el("header.chn-top", {}, [
      el("div.grow", {}, [el("h2", {}, ["RTMP channels ", this.count]), el("p.chn-lede", { text: "Encoders publish to the mixer. Each channel can go on air and on to the platforms." })]),
      add,
    ]);
    this.root = el("div.chn", {}, [this.head, this.list]);
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
    this.count.textContent = channels.length ? String(channels.length) : "";
    if (this.missing || this.failure || !channels.length) {
      this.cards.clear();
      clear(this.list);
      this.list.appendChild(this.missing ? installCard(this.client, () => this.load()) : this.failure ? failed(this.failure, () => this.load()) : emptyArt(() => this.add()));
      // The empty picture has its own big button; one is enough.
      this.head.hidden = !this.missing && !this.failure;
      this.clock(false);
      return;
    }
    this.head.hidden = false;
    if (!this.cards.size) clear(this.list);
    keyed(this.list, this.cards, channels, (c) => c.id, (c) => channelCard(this, c), (c) => (c.enabled ? "on" : "off"));
    this.clock(channels.some(isLive));
  }

  /** Uptime moves every second while something is live. Local only: no call. */
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
  }
}

function failed(message, retry) {
  return el("div.chn-note", {}, [
    el("p", { text: `The channels could not be read: ${message}` }),
    el("button.btn", { text: "Try again", onclick: retry }),
  ]);
}

/** Make the Channels tab the one on show, wherever it has been docked. */
export function bringForward() {
  const ws = document.querySelector("gmx-shell")?.workspace;
  if (!ws) return;
  const group = leaves(ws.state.tree).find((g) => g.tabs.includes("core/channels"));
  if (group) ws.activate(group, "core/channels");
  else ws.show("core/channels");
}
