// The ad break under the Media list: a cue in seconds, Roll and End early,
// and a custom path for anything not in the library.

import { el } from "../../shell/dom.js";
import { errorToast, toast } from "../../shell/toast.js";

export const AD_KEY = "gmx.adUri";

export class AdBreak {
  constructor(client) {
    this.client = client;
    try {
      this.selected = localStorage.getItem(AD_KEY) || "";
    } catch {
      this.selected = "";
    }
    this.cue = el("input", { type: "number", min: "0", max: "600", value: "0", style: { width: "5em" }, title: "Seconds from now" });
    this.custom = el("input", { type: "text", placeholder: "Or a path the mixer can read" });
    this.hint = el("div.sm.faint.pad");
  }

  nodes() {
    return [
      el("div.row.pad", {}, [
        el("span.sm.dim", { text: "Ad break in" }),
        this.cue,
        el("span.sm.dim", { text: "s" }),
        el("button.btn.primary", { text: "Roll ad", onclick: () => this.roll() }),
        el("button.btn", { text: "End early", onclick: () => this.end() }),
        el("span.grow"),
      ]),
      el("details.pad", {}, [el("summary.sm.dim", { text: "Custom path" }), this.custom]),
      this.hint,
    ];
  }

  choose(path) {
    this.selected = path;
    try {
      localStorage.setItem(AD_KEY, path);
    } catch {
      /* the choice lasts the session */
    }
    this.custom.value = path;
  }

  async roll() {
    const uri = (this.custom.value || this.selected || "").trim();
    if (!uri) {
      toast({ kind: "warning", text: "Pick a clip first, or type a path." });
      return;
    }
    const delay = Number(this.cue.value) || 0;
    const params = { uri };
    if (delay > 0) params.at_running_time_ms = (this.client.state.running_time_ms || 0) + delay * 1000;
    try {
      await this.client.call("adbreak.start", params);
      toast({ text: delay > 0 ? `Ad break in ${delay}s.` : "Ad break rolling." });
    } catch (e) {
      errorToast(e, "Roll ad");
    }
  }

  end() {
    this.client.call("adbreak.end", {}).catch((e) => errorToast(e, "End ad"));
  }

  paint() {
    const ad = this.client.state.ad;
    this.hint.textContent = ad ? (ad.on_air ? `On air: ${ad.uri}${ad.return_to ? `, then back to ${ad.return_to}` : ""}` : `Cued: ${ad.uri}`) : "";
  }
}
