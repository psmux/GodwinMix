// The Prompter panel: write and pick scripts, and drive every prompter
// screen while live. It listens for `prompter.*` only while it is on screen;
// the screens themselves are the page at /prompter/.

import { el } from "../../shell/dom.js";
import { registerAll } from "../../shell/commands.js";
import { toast } from "../../shell/toast.js";
import { editor } from "./editor.js";
import { drive } from "./drive.js";
import { openShare } from "./share.js";

class View {
  constructor(host) {
    this.host = host;
    this.client = host.client;
    this.running = false;
    this.offs = [];
    this.list = { scripts: [], on: null };
    this.picker = el("select.grow", { "aria-label": "Script", onchange: () => this.pick(this.picker.value) });
    this.edit = editor(this.client, {
      saved: (script) => this.reload(script.id),
      removed: () => this.reload(""),
      failed: (e) => this.fail(e),
    });
    this.drive = drive(this.client, (e) => this.fail(e));
    this.root = el("div.col.pad.prompter-panel", {}, [
      el("div.row", {}, [
        this.picker,
        el("button.btn", { text: "New", onclick: () => this.pick("") }),
        el("button.btn", { text: "Open on a screen", title: "The prompter page, here or on a phone or tablet", onclick: () => openShare(this.client) }),
      ]),
      this.drive.root,
      this.edit.root,
    ]);
    host.append(this.root);
  }

  start() {
    if (this.running) return;
    this.running = true;
    this.offs = [
      this.client.listen ? this.client.listen("prompter.*") : () => {},
      this.client.on("event", (ev) => this.event(ev)),
      this.client.on("open", () => this.reload()),
    ];
    this.reload();
  }

  stop() {
    if (!this.running) return;
    this.running = false;
    for (const off of this.offs) off();
    this.offs = [];
    this.drive.stop();
  }

  event({ name, params }) {
    if (name === "prompter.changed") this.drive.update(params);
    else if (name === "prompter.script.changed") this.reload();
  }

  fail(e) {
    toast({ kind: "warning", text: (e && e.message) || String(e) });
  }

  /** Read the list and the state again, and show `chosen` in the editor. */
  async reload(chosen) {
    try {
      const [list, state] = await Promise.all([this.client.call("prompter.script.list", {}), this.client.call("prompter.get", {})]);
      this.list = list;
      this.drive.update(state);
      const keep = chosen !== undefined ? chosen : this.picker.value || list.on || (list.scripts[0] && list.scripts[0].id) || "";
      this.paintPicker(keep);
      if (chosen !== undefined || keep !== this.edit.id()) await this.pick(keep);
    } catch (e) {
      this.fail(e);
    }
  }

  paintPicker(chosen) {
    const options = this.list.scripts.map((s) =>
      el("option", { value: s.id, text: (s.id === this.list.on ? "On the prompter: " : "") + s.title, selected: s.id === chosen })
    );
    options.push(el("option", { value: "", text: "A new script", selected: !chosen }));
    this.picker.replaceChildren(...options);
  }

  async pick(id) {
    this.picker.value = id;
    if (!id) return this.edit.show(null);
    try {
      this.edit.show(await this.client.call("prompter.script.get", { id }));
    } catch (e) {
      this.fail(e);
    }
  }
}

const CSS_ID = "gmx-prompter-css";

/** In the palette from the panel's first showing on, for a desk keyboard. */
function commands() {
  const call = (method, params) => window.gmxClient?.call(method, params || {});
  registerAll([
    { id: "prompter.open", title: "Open the prompter page", group: "Prompter", keywords: ["teleprompter", "autocue", "script"], run: () => window.open("/prompter/", "gmx-prompter") },
    { id: "prompter.toggle", title: "Play or pause the prompter", group: "Prompter", keywords: ["teleprompter", "autocue"], run: () => call("prompter.get").then((s) => call(s.playing ? "prompter.pause" : "prompter.play")) },
    { id: "prompter.next", title: "Prompter: next marker", group: "Prompter", keywords: ["teleprompter"], run: () => call("prompter.seek", { step: 1 }) },
    { id: "prompter.previous", title: "Prompter: previous marker", group: "Prompter", keywords: ["teleprompter"], run: () => call("prompter.seek", { step: -1 }) },
  ]);
}

export function mount(host) {
  if (!document.getElementById(CSS_ID)) commands();
  if (!document.getElementById(CSS_ID)) {
    document.head.appendChild(el("link#" + CSS_ID, { rel: "stylesheet", href: new URL("./prompter.css", import.meta.url).href }));
  }
  if (!host.view) host.view = new View(host);
  if (host.isConnected) host.view.start();
}
