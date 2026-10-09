// The channels on the wall: channel.list once when the wall opens, on a
// channel event, and every two seconds for the bit rates while the wall is
// open and the tab is visible. A core without channels answers -32601 once
// and is not asked again. Nothing runs once the wall closes.

import { channelItems } from "./channel-model.js";
import { noMethod } from "./data.js";

const EVERY_MS = 2000;

export class ChannelWatch {
  constructor(client, changed) {
    this.client = client;
    this.changed = changed;
    this.items = [];
    this.missing = false;
    this.busy = false;
    this.offs = [];
    this.timer = null;
  }

  start() {
    const c = this.client;
    this.offs.push(c.listen ? c.listen("channel.*") : () => {});
    this.offs.push(c.on("event", (e) => { if (/^channel\./.test((e && e.name) || "")) this.read(); }));
    this.timer = setInterval(() => this.read(), EVERY_MS);
    this.read();
  }

  stop() {
    for (const off of this.offs) off();
    this.offs = [];
    clearInterval(this.timer);
    this.timer = null;
    this.stopped = true;
  }

  async read() {
    if (this.busy || this.stopped || this.missing || document.hidden) return;
    this.busy = true;
    try {
      this.items = channelItems(await this.client.call("channel.list", {}));
      this.changed();
    } catch (e) {
      if (noMethod(e)) {
        this.missing = true;
        this.items = [];
        this.changed();
      } else console.debug("channel.list", e);
    } finally {
      this.busy = false;
    }
  }
}
