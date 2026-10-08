// The header: what is on air, how loud it is, how long it has been up, and
// which encoder is doing the work.
//
// The only always red thing on the page lives here. Red means going out.

import { el, clear, fmtDuration } from "../../shell/dom.js";
import { addView, dropView, meterElement, takeMeters } from "../../shell/meter.js";
import { openSettings } from "../../shell/settings.js";
import { errorToast } from "../../shell/toast.js";
import { programLabel } from "../../client/store.js";
import { menubar } from "../../shell/menubar.js";
import { showTabs } from "../../shell/show-tabs.js";
import { presenceButton } from "../../shell/presence.js";
import { destinationsPill } from "./destinations.js";

class HeaderPanel extends HTMLElement {
  static get panel() {
    return { id: "core/header", title: "Header", slots: ["header"], tag: "gmx-header" };
  }

  setClient(client) {
    this.client = client;
  }

  connectedCallback() {
    if (this.built) return;
    this.built = true;
    this.className = "header";

    this.tally = el("div.tally", {}, [el("span.dot"), el("span", { text: "black" })]);
    this.meter = meterElement("h");
    this.meter.style.width = "110px";
    this.meter.style.height = "14px";
    this.peak = el("span.num.sm.dim.hdr-peak", { style: { minWidth: "3.2em" } });
    this.uptime = el("span.num.sm.dim.hdr-uptime");
    this.backend = el("span.sm.faint.ellipsis.hdr-backend");
    this.destinations = el("span.pill.hdr-dest", { text: "No destinations", role: "status" });
    this.recording = el("span.pill.live", { text: "REC", hidden: true, role: "status" });
    this.ad = el("span.pill.live", { text: "AD BREAK", hidden: true });

    this.append(
      el("strong.wordmark", { "aria-label": "GodwinMix" }, [el("span.wm-godwin", { text: "Godwin" }), el("span.wm-mix", { text: "Mix" })]),
      // File, Edit, View and the rest, before the live controls so they keep
      // their size; one button in their place on a phone.
      menubar(this.client),
      showTabs(this.client),
      this.tally,
      this.ad,
      this.destinations,
      this.recording,
      el("div.row.hdr-meter", { style: { width: "110px" } }, [this.meter]),
      this.peak,
      el("span.grow.hdr-gap"),
      // How many other people are operating this mixer, from a phone or a
      // desk. Hidden while there is nobody else.
      presenceButton(this.client),
      this.uptime,
      this.backend,
      el("button.btn.hdr-black", {
        text: "Cut to black",
        title: "0",
        onclick: () => this.client.call("program.take", { source: "" }).catch((e) => errorToast(e, "Cut to black")),
      }),
      // The palette arrives when it is asked for, here and on Ctrl+K.
      el("button.btn.icon.hdr-palette", {
        text: "⌘K",
        title: "Command palette",
        onclick: () => import("../../shell/palette.js").then((m) => m.openPalette()),
      }),
      el("button.btn.icon.hdr-settings", { text: "⚙", title: "Settings", "aria-label": "Settings", onclick: () => openSettings(this.client) })
    );

    addView("header", "program", this.meter, "h", this.peak);
    this.offs = [
      this.client.onRender((s) => this.render(s)),
      this.client.on("meters", (p) => takeMeters(p)),
    ];
    // The core's uptime arrives with a snapshot and with nothing else, so the
    // clock counts on from the last one by itself. A number that stands still
    // on a live mixer reads as a page that has hung.
    const clock = setInterval(() => this.tick(), 1000);
    this.offs.push(() => clearInterval(clock));
    this.render(this.client.state);
  }

  disconnectedCallback() {
    dropView("header");
    for (const off of this.offs || []) off();
    this.offs = [];
  }

  /** The uptime the core last said, plus what has passed here since. */
  tick() {
    if (this.uptimeAt === undefined) return;
    const since = this.connected ? (performance.now() - this.uptimeAt) / 1000 : 0;
    this.uptime.textContent = fmtDuration(Math.floor((this.uptimeSaid || 0) + since));
  }

  render(s) {
    // A multi item scene puts nothing in `program`, so reading that alone
    // said "black" over a live programme.
    const on = !!(s.program || s.scene);
    this.tally.classList.toggle("on", on);
    this.tally.lastChild.textContent = programLabel(s) || "black";
    document.body.classList.toggle("onair", on);
    if (s.uptime_secs !== this.uptimeSaid) {
      this.uptimeSaid = s.uptime_secs;
      this.uptimeAt = performance.now();
    }
    this.connected = s.connected;
    this.tick();
    const b = s.backend;
    // "vtenc_h264_hw · hardware" on its own reads like an error code. The word
    // in front says what the name is the name of, and the title says it in
    // full for anybody who hovers.
    const how = b && b.hardware_accelerated ? "hardware" : "software";
    this.backend.textContent = b ? `Encoder ${b.video_encoder} · ${how}` : "";
    this.backend.title = b
      ? `The video encoder in use: ${b.video_encoder}, running in ${how}.`
      : "";
    const outputs = s.outputs || [];
    const pill = destinationsPill(outputs);
    this.destinations.textContent = pill.text;
    this.destinations.title = pill.title;
    this.destinations.classList.toggle("live", pill.kind === "live");
    this.destinations.classList.toggle("failed", pill.kind === "failed");
    this.recording.hidden = !outputs.some(o => o.type === "record/output" && o.state === "live");
    this.ad.hidden = !(s.ad && s.ad.on_air);
  }
}

customElements.define("gmx-header", HeaderPanel);
window.godwinmixPanels.push(HeaderPanel);
export default HeaderPanel;
