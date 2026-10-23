// What the flow map knows: every show on the station with show.stats for
// all of them once a second, and two minutes of each one's rates and
// errors, from the headend view's own data (headend/data.js), which this
// only widens to the shows that mix. Those are followed by mixed.js. The
// GPU and the box are headend/machine.js. Nothing here runs until the map
// opens, and all of it stops when it closes.

import { WallData } from "../wall/data.js";
import { HeadendData } from "../headend/data.js";

export class FlowData extends HeadendData {
  /** Every show, mixed or not: the map draws them all. */
  async read() {
    await WallData.prototype.read.call(this);
    this.ids = this.shows.map((s) => s.id);
    this.tick();
  }

  event(e) {
    WallData.prototype.event.call(this, e);
  }

  /** The show this page is on, as the station names it. */
  get current() {
    return new URLSearchParams(location.search).get("show") || this.currentId || "";
  }

  set current(v) {
    this.currentId = v || "";
  }

  /** For a core with no shows of its own: this mixer as its one show. */
  page(client) {
    const s = client.state || {};
    return { id: "", name: "This mixer", state: s.connected === false ? "starting" : "running", compositing: true, programme_kbps: 0, health: { state: "ok", alarms: [] } };
  }
}
