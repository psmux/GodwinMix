// Outputs: where the programme is going, and whether it is arriving.
//
// The buffer depth and the reconnect count are the two numbers that tell an
// operator whether a wobbly link is coping, so both are on the row rather than
// behind a hover.

import { el, clear } from "../../shell/dom.js";
import { openPicker } from "../../shell/picker.js";
import { confirmModal } from "../../shell/modal.js";
import { errorToast, toast } from "../../shell/toast.js";
import { registerAll } from "../../shell/commands.js";
import { settings } from "../../shell/settings.js";

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
        el("button.btn", { text: "Add", onclick: () => openPicker(this.client, "output") }),
      ]),
      this.list
    );
    this.offs = [
      this.client.onRender((s) => this.render(s)),
      registerAll([
        { id: "output.add-picker", title: "Add an output", group: "Outputs", run: () => openPicker(this.client, "output") },
      ]),
    ];
    this.render(this.client.state);
  }

  disconnectedCallback() {
    for (const off of this.offs || []) off();
    this.offs = [];
  }

  render(s) {
    const outputs = s.outputs || [];
    this.count.textContent = outputs.length ? String(outputs.length) : "";
    clear(this.list);
    if (!outputs.length) {
      this.list.appendChild(el("p.dim.sm", { text: "Nothing is being sent anywhere. Add an output to go live.", style: { margin: 0 } }));
      return;
    }
    for (const output of outputs) this.list.appendChild(this.row(output));
  }

  row(output) {
    return el("div.row", {}, [
      el("span.dot." + (output.state || ""), { title: output.state }),
      el("strong", { text: output.id }),
      el("span.sm.dim", { text: output.state }),
      el("span.sm.faint.ellipsis.grow", { text: output.uri_host }),
      el("span.num.sm.faint", { text: `buffer ${Number(output.queue_secs || 0).toFixed(1)}s · ${output.reconnects || 0} reconnects` }),
      el("button.btn.icon", {
        text: "Reconnect",
        onclick: async () => {
          try {
            await this.client.call("output.reconnect", { output: output.id });
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
            await this.client.call("output.remove", { output: output.id });
          } catch (e) {
            errorToast(e, "Remove");
          }
        },
      }),
    ]);
  }
}

customElements.define("gmx-outputs", OutputsPanel);
window.godwinmixPanels.push(OutputsPanel);
export default OutputsPanel;
