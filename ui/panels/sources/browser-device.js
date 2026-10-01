// This browser's camera and microphone as a mixer source.
//
// Picking it is the whole of adding it, as for a camera on the mixer: the
// card opens the devices, starts sending by itself, and once the channel has
// made the source it is handed to `onSource`, which is how the picker puts it
// in the scene. The card then folds to a bar. It has to stay on the page,
// because the camera belongs to this tab, and the bar is where the operator
// switches device, mutes or stops it.

import { el } from "../../shell/dom.js";
import { confirmModal } from "../../shell/modal.js";
import { errorToast } from "../../shell/toast.js";
import { mountPublisher } from "../../join/publisher.js";
import { ensureBrowserChannel, streamNameFor, sourceIdFor, streamOf } from "./browser-channel.js";

let current = null;

/**
 * Open the card, or bring back the one already open. `camera: false` starts
 * with the microphone alone. `onSource` is called once with the mixer source,
 * as soon as there is one.
 */
export async function openBrowserDevice(client, { camera = true, onSource } = {}) {
  if (current) return current.show().whenSource(onSource);
  let got;
  try {
    got = await ensureBrowserChannel(client);
  } catch (e) {
    errorToast(e, "This browser's camera");
    return null;
  }
  if (current) return current.show().whenSource(onSource);
  current = new BrowserDock(client, got, camera);
  return current.whenSource(onSource);
}

class BrowserDock {
  constructor(client, { channel, key }, camera) {
    this.client = client;
    this.channel = channel;
    this.stream = streamNameFor();
    this.source = sourceIdFor(channel, this.stream);
    this.label = camera ? "This browser's camera" : "This browser's microphone";
    this.waiting = [];
    this.folded = false;
    stylesheet();
    this.note = el("div.pub-dock-note.sm.dim");
    this.body = el("div.pub-dock-body", {}, [this.note]);
    this.title = el("strong.grow.ellipsis", { text: this.label });
    this.node = el("section.pub-dock", { role: "dialog", "aria-label": "This browser's camera and microphone" }, [
      el("div.row.pub-dock-head", {}, [
        this.title,
        el("button.btn.icon", { type: "button", text: "_", title: "Fold away", "aria-label": "Fold away", onclick: () => this.fold() }),
        el("button.btn.icon", { type: "button", text: "×", title: "Close", "aria-label": "Close", onclick: () => this.close() }),
      ]),
      this.body,
    ]);
    document.body.appendChild(this.node);
    const where = `Publishing to ${channel.app || channel.id}/${this.stream}, which becomes the source ${this.source}.`;
    this.pub = mountPublisher(this.body, {
      url: `/whip/${encodeURIComponent(channel.app || channel.id)}/${encodeURIComponent(this.stream)}`,
      key,
      camera,
      autostart: true,
      labels: { where, go: "Send to the mixer" },
      onState: () => this.paint(),
    });
    this.offs = [
      client.on("event", ({ name, params }) => {
        if (name === "channel.changed" && params.channel && params.channel.id === this.channel.id) {
          this.channel = params.channel;
          this.paint();
        }
      }),
      client.onRender(() => this.paint()),
    ];
    this.onUnload = (e) => {
      if (this.pub.active()) e.preventDefault();
    };
    addEventListener("beforeunload", this.onUnload);
    this.paint();
  }

  /** Call `fn` with the mixer source once it exists: now, or when it arrives. */
  whenSource(fn) {
    if (fn) this.waiting.push(fn);
    this.paint();
    return this;
  }

  /** Hand the source to whoever is waiting for it, once each, and fold away the first time. */
  arrived(source) {
    const waiting = this.waiting.splice(0);
    for (const fn of waiting) Promise.resolve(fn(source)).catch((e) => errorToast(e, this.label));
    if (!this.folded) {
      this.folded = true;
      this.fold(true);
    }
  }

  /** The line above the picture: what the mixer has made of the stream. */
  paint() {
    const s = streamOf(this.channel, this.stream);
    const source = (this.client.state.sources || []).find((x) => x.id === this.source);
    const inMixer = !!source;
    if (source && this.pub.active()) this.arrived(source);
    let text = "";
    if (this.pub.active() && inMixer) text = `In the mixer as ${this.source}.`;
    else if (this.pub.active() && s && s.state === "live") text = `The channel has the stream; ${this.source} is on its way.`;
    else if (this.pub.active()) text = "Sending to the mixer.";
    this.note.textContent = text;
    const state = this.pub.state();
    this.title.textContent = state === "live" ? `${this.label} (live)` : this.label;
  }

  show() {
    this.node.classList.remove("min");
    this.pub.setVisible(true);
    return this;
  }

  fold(folding = !this.node.classList.contains("min")) {
    this.node.classList.toggle("min", folding);
    this.pub.setVisible(!folding);
  }

  async close() {
    if (this.pub.active()) {
      const yes = await confirmModal(
        `This browser is publishing to the mixer. Closing stops it, and ${this.source} goes from Sources unless a scene holds it, in which case the scene shows its last picture.`,
        "Stop and close"
      );
      if (!yes) return;
    }
    this.destroy();
  }

  destroy() {
    for (const off of this.offs) off();
    removeEventListener("beforeunload", this.onUnload);
    this.pub.destroy();
    this.node.remove();
    if (current === this) current = null;
  }
}

/** The publisher's stylesheet, fetched with the first card and not before. */
function stylesheet() {
  if (document.querySelector("link[data-pub-css]")) return;
  const href = new URL("../../join/join.css", import.meta.url).href;
  document.head.appendChild(el("link", { rel: "stylesheet", href, "data-pub-css": "1" }));
}
