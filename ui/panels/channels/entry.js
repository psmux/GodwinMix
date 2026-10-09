// Channels. Only this loads with the page; the panel loads with its tab.
import { lazyAction } from "../../shell/lazy-action.js";
import { registerAll } from "../../shell/commands.js";
const load = () => import("./panel.js");
const go = () => import("./go.js");
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
const livebox = lazyAction(() => load().then((m) => m.importLivebox), "Bring channels from Livebox");
// The keywords are what Livebox and most encoders call these, so a person
// moving over finds them by the words they already have.
export const CHANNEL_COMMANDS = [
  { id: "channel.add", title: "Add Channel", group: "Channels", keywords: ["new channel", "rtmp app"], run: () => addChannel(window.gmxClient) },
  { id: "channel.import.livebox", title: "Bring channels from Livebox", group: "Channels", keywords: ["livebox", "import channels", "migrate"], run: () => livebox(window.gmxClient) },
  { id: "channel.open", title: "Open Channels", group: "Channels", keywords: ["push destinations", "push", "restream", "stream key", "stream url", "channel dashboard"], run: () => go().then((m) => m.openChannels()) },
  { id: "channel.rows", title: "Channels as rows", group: "Channels", keywords: ["bulk channel settings", "channel table"], run: () => go().then((m) => m.openChannels("rows")) },
  { id: "channel.paste", title: "Paste several push addresses", group: "Channels", keywords: ["bulk rtmp url", "bulk actions", "restream", "push destinations"], run: () => go().then((m) => m.pasteSeveral()) },
];
registerAll(CHANNEL_COMMANDS);
