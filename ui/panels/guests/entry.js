// Guests. Only this loads with the page; the panel loads when first shown.
const load = () => import("./panel.js");
class Guests extends HTMLElement {
  static get panel() { return { id: "core/guests", title: "Guests", tag: "gmx-guests" }; }
  setClient(c) { this.client = c; }
  connectedCallback() { load().then((m) => m.mount(this)); }
  disconnectedCallback() { this.view?.stop(); }
  setWorkspaceActive(on) { this.view?.[on ? "start" : "stop"](); }
}
customElements.define("gmx-guests", Guests);
window.godwinmixPanels?.push(Guests);
