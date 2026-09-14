// The header: what is on air, how loud it is, how long it has been up, and
// which encoder is doing the work.
//
// The only always red thing on the page lives here. Red means going out.

import { el, clear, fmtDuration } from "../../shell/dom.js";
import { addView, dropView, meterElement, takeMeters } from "../../shell/meter.js";
import { openSettings } from "../../shell/settings.js";
import { openPalette } from "../../shell/palette.js";
import { errorToast } from "../../shell/toast.js";

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
    this.peak = el("span.num.sm.dim", { style: { minWidth: "3.2em" } });
    this.uptime = el("span.num.sm.dim");
    this.backend = el("span.sm.faint.ellipsis");
    this.ad = el("span.pill.live", { text: "AD BREAK", hidden: true });

    this.append(
      el("strong", { text: "GodwinMix" }),
      this.tally,
      this.ad,
      el("div.row", { style: { width: "110px" } }, [this.meter]),
      this.peak,
      el("span.grow"),
      this.uptime,
      this.backend,
      el("button.btn", {
        text: "Cut to black",
        title: "0",
        onclick: () => this.client.call("program.take", { source: null }).catch((e) => errorToast(e, "Cut to black")),
      }),
      el("button.btn.icon", { text: "⌘K", title: "Command palette", onclick: () => openPalette() }),
      el("button.btn.icon", { text: "⚙", title: "Settings", "aria-label": "Settings", onclick: () => openSettings(this.client) })
    );

    addView("header", "program", this.meter, "h", this.peak);
    this.offs = [
      this.client.onRender((s) => this.render(s)),
      this.client.on("meters", (p) => takeMeters(p)),
    ];
    this.render(this.client.state);
  }

  disconnectedCallback() {
    dropView("header");
    for (const off of this.offs || []) off();
    this.offs = [];
  }

  render(s) {
    const on = !!s.program;
    const source = on ? s.sources.find((x) => x.id === s.program) : null;
    this.tally.classList.toggle("on", on);
    this.tally.lastChild.textContent = source ? source.name : on ? s.program : "black";
    document.body.classList.toggle("onair", on);
    this.uptime.textContent = fmtDuration(s.uptime_secs);
    const b = s.backend;
    this.backend.textContent = b ? `${b.video_encoder} · ${b.hardware_accelerated ? "hardware" : "software"}` : "";
    this.ad.hidden = !(s.ad && s.ad.on_air);
  }
}

customElements.define("gmx-header", HeaderPanel);
window.godwinmixPanels.push(HeaderPanel);
export default HeaderPanel;
