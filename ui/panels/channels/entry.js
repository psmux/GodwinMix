// Channels. Only this loads with the page; the panel loads with its tab.
import { lazyAction } from "../../shell/lazy-action.js";
import { register } from "../../shell/commands.js";
const load = () => import("./panel.js");
export const addChannel = lazyAction(() => load().then((m) => m.addChannel), "Add Channel");
class Channels extends HTMLElement {
  static get panel() { return { id: "core/channels", title: "Channels", tag: "gmx-channels" }; }
  setClient(c) { this.client = c; }
  connectedCallback() { load().then((m) => m.mount(this)); }
  disconnectedCallback() { this.view?.stop(); }
  setWorkspaceActive(on) { this.view?.[on ? "start" : "stop"](); }
}
customElements.define("gmx-channels", Channels);
window.godwinmixPanels?.push(Channels);
register({ id: "channel.add", title: "Add Channel", group: "Channels", run: () => addChannel(window.gmxClient) });
const livebox = lazyAction(() => load().then((m) => m.importLivebox), "Bring channels from Livebox");
register({ id: "channel.import.livebox", title: "Bring channels from Livebox", group: "Channels", run: () => livebox(window.gmxClient) });
