// Replay: the element, and the I, O and P keys, which fetch the panel first.
import { registerAll } from "../../shell/commands.js";
const load = () => import("./panel.js");
registerAll([["mark-in", "mark in", "markIn"], ["mark-out", "mark out", "markOut"], ["play-last", "play the last event", "playLast"]]
  .map(([id, what, f]) => ({ id: "replay." + id, title: "Replay: " + what, group: "Replay", run: () => load().then((m) => m[f]()) })));
class Replay extends HTMLElement {
  static get panel() { return { id: "core/replay", title: "Replay", tag: "gmx-replay" }; }
  setClient(c) { this.client = c; }
  connectedCallback() { load().then((m) => m.mount(this)); }
  disconnectedCallback() { this.view?.stop(); }
  setWorkspaceActive(on) { this.view?.[on ? "start" : "stop"](); }
}
customElements.define("gmx-replay", Replay);
window.godwinmixPanels?.push(Replay);
