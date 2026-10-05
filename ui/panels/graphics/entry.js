// Graphics. Only this loads with the page; the gallery loads with its tab,
// the first time it is shown, and asks the mixer for nothing before then.
class Graphics extends HTMLElement {
  static get panel() {
    return { id: "core/graphics", title: "Graphics", slots: ["footer"], tag: "gmx-graphics" };
  }
  setClient(client) {
    this.client = client;
    if (this.view) this.view.setClient(client);
  }
  connectedCallback() {
    if (this.view) return this.append(this.view);
    import("./panel.js").then(() => {
      if (this.view || !this.isConnected) return;
      this.view = document.createElement("gmx-graphics-view");
      this.view.setClient(this.client);
      if (this.active !== undefined) this.view.setWorkspaceActive?.(this.active);
      this.append(this.view);
    });
  }
  setWorkspaceActive(active) {
    this.active = active;
    this.view?.setWorkspaceActive?.(active);
  }
}
customElements.define("gmx-graphics", Graphics);
window.godwinmixPanels?.push(Graphics);
