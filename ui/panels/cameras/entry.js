// Cameras. Only this loads with the page; the window loads when first shown.
const load = () => import("./mount.js");
class Cameras extends HTMLElement {
  static get panel() { return { id: "core/cameras", title: "Cameras", slots: ["main", "sidebar"], tag: "gmx-cameras" }; }
  setClient(c) { this.client = c; }
  connectedCallback() { load().then((m) => m.mount(this)); }
  disconnectedCallback() { this.view?.stop(); }
  setWorkspaceActive(on) { this.view?.[on ? "start" : "stop"](); }
}
customElements.define("gmx-cameras", Cameras);
window.godwinmixPanels?.push(Cameras);
