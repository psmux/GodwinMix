// Media. Only this loads with the page; the panel and its library load with
// its tab, the first time it is shown.
class Media extends HTMLElement {
  static get panel() {
    return { id: "core/media", title: "Media", slots: ["footer"], tag: "gmx-media" };
  }
  setClient(client) {
    this.client = client;
    if (this.view) this.view.setClient(client);
  }
  connectedCallback() {
    if (this.view) return this.append(this.view);
    import("./panel.js").then(() => {
      if (this.view || !this.isConnected) return;
      this.view = document.createElement("gmx-media-view");
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
customElements.define("gmx-media", Media);
window.godwinmixPanels?.push(Media);
