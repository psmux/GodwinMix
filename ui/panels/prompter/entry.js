// Prompter. Only this loads with the page; the panel loads when first shown.
const load = () => import("./panel.js");
class Prompter extends HTMLElement {
  static get panel() { return { id: "core/prompter", title: "Prompter", tag: "gmx-prompter" }; }
  setClient(c) { this.client = c; }
  connectedCallback() { load().then((m) => m.mount(this)); }
  disconnectedCallback() { this.view?.stop(); }
  setWorkspaceActive(on) { this.view?.[on ? "start" : "stop"](); }
}
customElements.define("gmx-prompter", Prompter);
window.godwinmixPanels?.push(Prompter);
