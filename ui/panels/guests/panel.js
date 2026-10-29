// The Guests panel: invite a guest, send them the link, and run them while
// they are on: their picture and state, take them to air, mute them, talk to
// them off air, let them go. It listens for `guest.*` only while it is on
// screen, and fetches a still of each live guest every two seconds then.

import { el } from "../../shell/dom.js";
import { toast } from "../../shell/toast.js";
import { guestCard } from "./card.js";
import { openLink } from "./share.js";
import { Talkback } from "./talk.js";

const STILL_MS = 2000;

class View {
  constructor(host) {
    this.client = host.client;
    this.running = false;
    this.offs = [];
    this.cards = new Map();
    this.sources = new Map();
    this.list = { guests: [], talkback: { held: [] }, max_guests: 8 };
    this.name = el("input.grow.guests-name", { type: "text", placeholder: "Guest's name, as it goes on screen", "aria-label": "Guest's name", maxLength: 64, onkeydown: (e) => e.key === "Enter" && this.invite() });
    this.talk = new Talkback(this.client, (e) => this.fail(e));
    this.rows = el("div.col.guests-rows");
    this.empty = el("p.dim.sm", { text: "Nobody is invited yet. Type a name and press Invite; send the guest the link or let them scan the code." });
    this.count = el("span.sm.dim");
    this.root = el("div.col.pad.guests-panel", {}, [
      el("div.row.guests-invite", {}, [this.name, el("button.btn.primary", { text: "Invite", onclick: () => this.invite() })]),
      el("div.row.guests-all", {}, [this.count, el("span.grow"), this.talk.button(null, "Talk to everyone")]),
      this.empty,
      this.rows,
    ]);
    host.append(this.root);
  }

  start() {
    if (this.running) return;
    this.running = true;
    this.offs = [
      this.client.listen ? this.client.listen("guest.*") : () => {},
      this.client.on("event", ({ name, params }) => name === "guest.changed" && this.paint(params)),
      this.client.on("open", () => this.reload()),
    ];
    this.stills = setInterval(() => this.refreshStills(), STILL_MS);
    this.reload();
  }

  stop() {
    if (!this.running) return;
    this.running = false;
    for (const off of this.offs) off();
    this.offs = [];
    clearInterval(this.stills);
    this.talk.release();
  }

  fail(e) {
    toast({ kind: "warning", text: (e && e.message) || String(e) });
  }

  async reload() {
    try {
      const [list, sources] = await Promise.all([this.client.call("guest.invite.list", {}), this.client.call("source.list", {})]);
      this.sources = new Map((sources || []).map((s) => [s.id, s]));
      this.paint(list);
    } catch (e) {
      this.fail(e);
    }
  }

  async invite() {
    const name = this.name.value.trim();
    if (!name) return this.name.focus();
    try {
      const made = await this.client.call("guest.invite.create", { name });
      this.name.value = "";
      openLink(made, made.invite.name);
    } catch (e) {
      this.fail(e);
    }
  }

  /** Draw the list as the mixer has it, keeping each card that is still there. */
  paint(list) {
    this.list = list;
    const live = list.guests.filter((g) => g.state === "live" || g.state === "joining").length;
    this.count.textContent = `${live} of ${list.max_guests} on`;
    this.empty.hidden = list.guests.length > 0;
    const seen = new Set();
    for (const g of list.guests) {
      seen.add(g.id);
      let card = this.cards.get(g.id);
      if (!card) {
        card = guestCard(this.client, g, this.talk, (e) => this.fail(e));
        this.cards.set(g.id, card);
      }
      card.update(g, this.sources.get(g.source));
      this.rows.append(card.root);
    }
    for (const [id, card] of this.cards) {
      if (!seen.has(id)) {
        card.root.remove();
        this.cards.delete(id);
      }
    }
    this.talk.paint(list.talkback);
  }

  /** A fresh still of each guest whose picture is arriving, and their fader state. */
  async refreshStills() {
    if (document.hidden) return;
    const on = this.list.guests.filter((g) => g.state === "live");
    if (!on.length) return;
    const sources = await this.client.call("source.list", {}).catch(() => null);
    if (sources) this.sources = new Map(sources.map((s) => [s.id, s]));
    for (const g of on) this.cards.get(g.id)?.still(this.sources.get(g.source));
  }
}

const CSS_ID = "gmx-guests-css";

export function mount(host) {
  if (!document.getElementById(CSS_ID)) {
    document.head.appendChild(el("link#" + CSS_ID, { rel: "stylesheet", href: new URL("./guests.css", import.meta.url).href }));
  }
  if (!host.view) host.view = new View(host);
  if (host.isConnected) host.view.start();
}
