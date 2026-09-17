// Plugins: what is installed, and where to get more.
//
// This file is a stub on purpose. Nobody opening the page to run a show needs
// a plugin manager on first paint, so the body arrives with an `import()` the
// first time somebody opens the section, the way the composer does. What ships
// eagerly is this element, which is a name in the registry and a title in the
// shell's section list.

import { el } from "../../shell/dom.js";

class PluginsPanel extends HTMLElement {
  static get panel() {
    return { id: "core/plugins", title: "Plugins", slots: ["main"], tag: "gmx-plugins" };
  }

  setClient(client) {
    this.client = client;
  }

  connectedCallback() {
    if (this.built) return;
    this.built = true;
    this.body = el("div.plugins");
    this.append(this.body);
    // The shell wraps every panel in a <details>, and this one starts closed.
    // A panel mounted somewhere without one loads straight away rather than
    // waiting for a toggle that will never come.
    const section = this.closest("details.panel-section");
    if (!section || section.open) {
      this.load();
      return;
    }
    const watch = () => {
      if (!section.open) return;
      section.removeEventListener("toggle", watch);
      this.load();
    };
    section.addEventListener("toggle", watch);
    this.offSection = () => section.removeEventListener("toggle", watch);
  }

  disconnectedCallback() {
    if (this.offSection) this.offSection();
    this.offSection = null;
    if (this.view) this.view.destroy();
    this.view = null;
  }

  /** The body, once and only once, and a sentence if it will not arrive. */
  load() {
    if (this.loading) return this.loading;
    this.body.appendChild(el("p.faint.sm", { text: "Reading the plugin list." }));
    this.loading = import("./tabs.js")
      .then((m) => {
        if (!this.isConnected) return;
        this.body.textContent = "";
        this.view = m.mountTabs(this.client, this.body);
      })
      .catch((e) => {
        this.body.textContent = "";
        this.body.appendChild(
          el("p.sm", {
            text:
              `The Plugins panel did not load: ${e && e.message ? e.message : e}. ` +
              "Reload the page to try again; the mixer itself is unaffected.",
          })
        );
      });
    return this.loading;
  }
}

customElements.define("gmx-plugins", PluginsPanel);
if (window.godwinmixPanels) window.godwinmixPanels.push(PluginsPanel);
export default PluginsPanel;
