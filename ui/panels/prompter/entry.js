// Prompter. Only this loads with the page; the panel loads the first time it
// is shown, and asks the mixer for nothing before then.
import { registerAll } from "../../shell/commands.js";
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
const call = (method, params) => window.gmxClient?.call(method, params || {});
registerAll([
  { id: "prompter.open", title: "Open the prompter page", group: "Prompter", keywords: ["teleprompter", "autocue", "script"], run: () => window.open("/prompter/", "gmx-prompter") },
  { id: "prompter.toggle", title: "Play or pause the prompter", group: "Prompter", keywords: ["teleprompter", "autocue"], run: () => call("prompter.get").then((s) => call(s.playing ? "prompter.pause" : "prompter.play")) },
  { id: "prompter.next", title: "Prompter: next marker", group: "Prompter", keywords: ["teleprompter"], run: () => call("prompter.seek", { step: 1 }) },
  { id: "prompter.previous", title: "Prompter: previous marker", group: "Prompter", keywords: ["teleprompter"], run: () => call("prompter.seek", { step: -1 }) },
]);
