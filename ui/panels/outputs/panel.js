// Outputs: where the programme is going, and whether it is arriving.
//
// The buffer depth and the reconnect count are the two numbers that tell an
// operator whether a wobbly link is coping, so both are on the row rather than
// behind a hover.
//
// The state is spelled out in words a volunteer can act on. "Reconnecting" and
// a count of twelve is the truth, but "Needs a stream key" is the truth *and*
// the next step, and it is the one that used to end with somebody being told
// to open a TOML file on the machine. `has_key` on the record is what makes
// the difference readable here without the key ever reaching this page.

import { el, clear } from "../../shell/dom.js";
import { confirmModal } from "../../shell/modal.js";
import { errorToast, toast } from "../../shell/toast.js";
import { registerAll } from "../../shell/commands.js";
import { settings } from "../../shell/settings.js";
import { startRecording, recordingRow, isRecording } from "./recording.js";
import { addDestination, editDestination } from "./destination.js";

/** The state of one destination, in words that say what to do about it. */
export function stateLabel(output) {
  if (output.state === "live") return "Live";
  // Ahead of the connection state on purpose: a destination still carrying a
  // preset's placeholder is not going to connect, and "Reconnecting, attempt
  // 47" tells nobody why.
  if (output.has_key === false) return "Needs a stream key";
  switch (output.state) {
    case "connecting":
      return "Connecting";
    case "reconnecting":
      return `Reconnecting, attempt ${output.reconnects || 1}`;
    case "failed":
      return "Stopped";
    default:
      return output.state || "";
  }
}

/** Which dot a destination gets. A missing key is a fault, not a warning. */
export function dotClass(output) {
  if (output.state === "live") return "live";
  if (output.has_key === false) return "failed";
  if (output.state === "connecting" || output.state === "reconnecting") return "connecting";
  return "failed";
}

class OutputsPanel extends HTMLElement {
  static get panel() {
    return { id: "core/outputs", title: "Outputs", slots: ["footer"], tag: "gmx-outputs" };
  }

  setClient(client) {
    this.client = client;
  }

  connectedCallback() {
    if (this.built) return;
    this.built = true;
    this.count = el("span.sm.dim");
    this.list = el("div.col.pad");
    this.append(
      el("div.row.pad", {}, [
        el("strong", { text: "Outputs" }),
        this.count,
        el("span.grow"),
        el("button.btn", { text: "Record", onclick: () => startRecording(this.client) }),
        el("button.btn", { text: "Add destination", onclick: () => this.add() }),
      ]),
      this.list
    );
    this.offs = [
      this.client.onRender((s) => this.render(s)),
      registerAll([
        { id: "output.add-destination", title: "Add a destination", group: "Outputs", run: () => this.add() },
      ]),
    ];
    this.render(this.client.state);
  }

  disconnectedCallback() {
    for (const off of this.offs || []) off();
    this.offs = [];
  }

  add() {
    return addDestination(this.client);
  }

  render(s) {
    const outputs = s.outputs || [];
    this.count.textContent = outputs.length ? String(outputs.length) : "";
    clear(this.list);
    if (!outputs.length) {
      this.list.appendChild(
        el("p.dim.sm", {
          text: "Nothing is being sent anywhere. Add a destination to go live.",
          style: { margin: 0 },
        })
      );
      return;
    }
    for (const output of outputs) this.list.appendChild(this.row(output));
  }

  row(output) {
    if (isRecording(output)) return recordingRow(this.client, output);
    const needsKey = output.has_key === false;
    return el("div.output-row", {}, [
      el("div.row", {}, [
        el("span.dot." + dotClass(output), { title: output.state }),
        el("strong", { text: output.id }),
        el("span.sm" + (needsKey ? ".needs-key" : ".dim"), { text: stateLabel(output) }),
        el("span.grow"),
        el("button.btn.icon" + (needsKey ? ".primary" : ""), {
          text: needsKey ? "Add key" : "Edit",
          onclick: () => editDestination(this.client, output),
        }),
        el("button.btn.icon", {
          text: "Reconnect",
          onclick: async () => {
            try {
              await this.client.call("output.reconnect", { id: output.id });
              toast({ text: `Reconnecting ${output.id}.` });
            } catch (e) {
              errorToast(e, "Reconnect");
            }
          },
        }),
        el("button.btn.icon.danger", {
          text: "Remove",
          onclick: async () => {
            if (settings().confirmRemove && !(await confirmModal(`Stop sending to "${output.id}"?`, "Stop"))) return;
            try {
              await this.client.call("output.remove", { id: output.id });
            } catch (e) {
              errorToast(e, "Remove");
            }
          },
        }),
      ]),
      el("div.row.sm.faint.output-detail", {}, [
        el("span.ellipsis.grow", { text: output.uri_host }),
        el("span.num", {
          text: `buffer ${Number(output.queue_secs || 0).toFixed(1)}s · ${output.reconnects || 0} reconnects`,
        }),
      ]),
    ]);
  }
}

customElements.define("gmx-outputs", OutputsPanel);
if (window.godwinmixPanels) window.godwinmixPanels.push(OutputsPanel);
export default OutputsPanel;
