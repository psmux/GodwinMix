// Alerts: the last few things that went wrong, kept where they can be read
// again after the toast has gone.

import { el, clear } from "../../shell/dom.js";
import { registerAll } from "../../shell/commands.js";

class AlertsPanel extends HTMLElement {
  static get panel() {
    return { id: "core/alerts", title: "Alerts", slots: ["footer"], tag: "gmx-alerts" };
  }

  setClient(client) {
    this.client = client;
  }

  connectedCallback() {
    if (this.built) return;
    this.built = true;
    this.list = el("div.col.pad", { style: { maxHeight: "160px", overflow: "auto" } });
    this.count = el("span.sm.dim");
    this.append(
      el("div.row.pad", {}, [
        el("strong", { text: "Alerts" }),
        this.count,
        el("span.grow"),
        el("button.btn", { text: "Clear", onclick: () => this.clear() }),
      ]),
      this.list
    );
    this.offs = [
      this.client.onRender((s) => this.render(s)),
      registerAll([{ id: "alerts.clear", title: "Clear alerts", group: "Shell", run: () => this.clear() }]),
    ];
    this.render(this.client.state);
  }

  setWorkspaceActive(active) {
    this.workspaceActive = active;
    if (active) this.render(this.client.state);
  }

  disconnectedCallback() {
    for (const off of this.offs || []) off();
    this.offs = [];
  }

  clear() {
    this.client.state.alerts.length = 0;
    this.render(this.client.state);
  }

  render(s) {
    if (this.workspaceActive === false) return;
    const alerts = s.alerts || [];
    this.count.textContent = alerts.length ? String(alerts.length) : "";
    clear(this.list);
    if (!alerts.length) {
      this.list.appendChild(el("p.faint.sm", { text: "Nothing has gone wrong.", style: { margin: 0 } }));
      return;
    }
    for (const a of alerts.slice(0, 20)) {
      this.list.appendChild(
        el("div.row.sm", {}, [
          el("span.dot" + (a.severity === "error" || a.severity === "critical" ? ".failed" : a.severity === "warning" ? ".stalled" : "")),
          el("span.num.faint", { text: new Date(a.at).toLocaleTimeString() }),
          el("span.grow", { text: a.message }),
        ])
      );
    }
  }
}

customElements.define("gmx-alerts", AlertsPanel);
window.godwinmixPanels.push(AlertsPanel);
export default AlertsPanel;
