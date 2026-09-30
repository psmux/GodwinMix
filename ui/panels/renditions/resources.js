// Resources: what this machine has, what is using it, and what the
// governor had to shed. The Outputs panel shows it as a tab.
//
// While it is on screen it asks for `governor.*` and `rendition.*` events
// and reads `governor.status` every two seconds, because load moves without
// any event saying so. Off screen it asks for nothing (AGENTS.md rule 1).

import { el } from "../../shell/dom.js";
import { toast, errorToast } from "../../shell/toast.js";
import { capacityBar, sessionPips } from "./bars.js";
import { cpuUsers, deviceUsers, onAir, deviceTitle, share, coresShort } from "./usage.js";
import { measuredWhen, upload } from "./words.js";
import { followPlan, missing, stylesheet } from "./shared.js";

const POLL_MS = 2000;
const CALM = "Measuring runs a short encode of each format on each encoder. It cannot run while something is on air, because it would take the machine from it.";

export class ResourcesView {
  constructor(client) {
    stylesheet();
    this.client = client;
    this.status = null;
    this.plan = null;
    this.shed = [];
    this.when = el("span.rnd-dim");
    this.measure = el("button.btn", { type: "button", text: "Measure this machine again", onclick: () => this.calibrate() });
    this.why = el("p.rnd-dim.rnd-why", { hidden: true, text: CALM });
    this.cards = el("div.rnd-rescards");
    this.node = el("div.rnd-resources", {}, [
      el("div.rnd-reshead", {}, [el("div.grow", {}, [el("strong", { text: "This machine" }), " ", this.when]), this.measure]),
      this.why,
      this.cards,
    ]);
  }

  start() {
    if (this.running) return;
    this.running = true;
    this.release = this.client.listen ? this.client.listen("governor.*") : () => {};
    this.unplan = followPlan(this.client, "programme", (plan) => { this.plan = plan; this.render(); });
    this.offs = [
      this.client.on("event", (e) => this.event(e)),
      this.client.onRender ? this.client.onRender(() => this.onAirChanged()) : () => {},
    ];
    this.timer = setInterval(() => this.read(), POLL_MS);
    this.read();
  }

  stop() {
    if (!this.running) return;
    this.running = false;
    this.release();
    this.unplan();
    for (const off of this.offs) off();
    clearInterval(this.timer);
  }

  event({ name, params }) {
    if (name !== "governor.shed" || !params) return;
    this.shed = [params, ...this.shed.filter((s) => s.what !== params.what)].slice(0, 20);
    this.render();
  }

  async read() {
    if (this.reading) return;
    this.reading = true;
    try {
      this.status = await this.client.call("governor.status", {});
      this.failure = "";
    } catch (e) {
      this.failure = missing(e) ? "This mixer does not measure its resources yet. It may be older than this page." : e.message || String(e);
    } finally {
      this.reading = false;
    }
    this.render();
  }

  onAirChanged() {
    const busy = onAir(this.client.state);
    this.measure.disabled = busy || !this.status;
    this.why.hidden = !busy;
  }

  async calibrate() {
    this.measure.disabled = true;
    try {
      await this.client.call("governor.calibrate", {});
      toast({ text: "Measuring. The numbers here change when it is done, in a minute or so." });
    } catch (e) {
      errorToast(e, "Measure this machine");
    } finally {
      this.onAirChanged();
    }
  }

  render() {
    this.onAirChanged();
    const s = this.status;
    if (!s) {
      this.cards.replaceChildren(el("p.rnd-dim", { text: this.failure || "Reading this machine." }));
      return;
    }
    this.when.textContent = measuredWhen(s.calibrated_at);
    const total = (s.cpu.cores || 1) * 1000;
    const devices = s.devices || [];
    const cpu = card("CPU", `${s.cpu.cores} cores`, `${coresShort(s.cpu.used_millicores)} in use`,
      capacityBar(total, cpuUsers(this.plan, s), s.cpu.room_millicores || 0, coresShort));
    const gpus = devices.map((d) => card(deviceTitle(d), "", `${share(d.used_millis)} busy`,
      capacityBar(1000, deviceUsers(this.plan, d, devices.length), d.room_millis || 0, share), sessionPips(d.sessions_used || 0, d.sessions_max || 0)));
    const none = devices.length ? [] : [card("GPU", "none found", "Everything is encoded on the CPU", el("p.rnd-dim", { text: "No hardware encoder was found when this machine was measured. The CPU does it all, which is what GodwinMix is built for." }))];
    const up = card("Upload", "", `${upload(s.egress_kbps || 0)} going out`, el("p.rnd-dim", { text: "Copies cost no CPU, only upload. The governor counts this too." }));
    this.cards.replaceChildren(cpu, ...gpus, ...none, up, this.shedCard());
  }

  shedCard() {
    const listed = [...this.shed, ...((this.status && this.status.shed) || [])];
    const seen = new Set();
    const items = listed.filter((x) => !seen.has(x.what) && seen.add(x.what));
    const body = items.length
      ? el("ul.rnd-shed", {}, items.map((x) => el("li", {}, [el("strong", { text: x.what }), el("span", { text: x.why })])))
      : el("p.rnd-dim", { text: "Nothing has been dropped. If the machine runs short while on air, previews go first, then the smallest HLS size, and never the programme." });
    return card("Shed", items.length ? String(items.length) : "", items.length ? "Dropped to keep the programme going" : "Nothing shed", body, null, items.length ? "warn" : "");
  }
}

function card(title, aside, figure, body, extra, tone) {
  return el("section.rnd-rescard" + (tone ? "." + tone : ""), {}, [
    el("div.rnd-rctop", {}, [el("span.rnd-kicker", { text: title }), el("span.rnd-dim", { text: aside })]),
    el("div.rnd-figure", { text: figure }),
    body,
    extra || null,
  ]);
}
