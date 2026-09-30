// What the planner did for each channel's destinations: "Copied", or which
// encoder made the rendition a destination asked for. One plan per channel
// that has destinations, followed only while the Channels panel is running.

import { followPlan, planLine, planShort } from "../renditions/plan-feed.js";

export class ChannelPlans {
  constructor(client, onChange) {
    this.client = client;
    this.onChange = onChange;
    this.follows = new Map();
    this.plans = new Map();
  }

  /** Follow every channel that sends somewhere, and stop following the rest. */
  sync(channels) {
    const wanted = new Set(channels.filter((c) => (c.destinations || []).length).map((c) => c.id));
    for (const [id, stop] of this.follows) {
      if (wanted.has(id)) continue;
      stop();
      this.follows.delete(id);
      this.plans.delete(id);
    }
    for (const id of wanted) {
      if (this.follows.has(id)) continue;
      this.follows.set(id, followPlan(this.client, `channel:${id}`, (plan) => {
        this.plans.set(id, plan);
        this.onChange();
      }));
    }
  }

  stop() {
    for (const stop of this.follows.values()) stop();
    this.follows.clear();
  }

  /** The line for one destination tile, short or whole. */
  line(channelId, destId, short) {
    const plan = this.plans.get(channelId);
    if (!plan) return "";
    return short ? planShort(plan, destId) : planLine(plan, destId);
  }
}
