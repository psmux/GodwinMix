// The card for this browser's camera and microphone: the publisher from
// /join/, folded to a bar over the workspace. See browser-device.js for how
// it is opened and why it has to stay on the page.

import { el } from "../../shell/dom.js";
import { confirmModal } from "../../shell/modal.js";
import { errorToast } from "../../shell/toast.js";
import { movable } from "../../shell/float-drag.js";
import { mountPublisher } from "../../join/publisher.js";
import { deviceName, sourceIdFor, streamOf } from "./browser-channel.js";

export class BrowserDock {
  constructor(client, { channel, key }, camera, devices, onGone) {
    this.onGone = onGone;
    this.client = client;
    this.channel = channel;
    this.stream = deviceName().stream;
    this.source = sourceIdFor(channel, this.stream);
    this.label = camera ? "This browser's camera" : "This browser's microphone";
    this.waiting = [];
    stylesheet();
    this.note = el("div.pub-dock-note.sm.dim");
    this.body = el("div.pub-dock-body", {}, [this.note]);
    this.title = el("strong.grow.ellipsis", { text: this.label });
    const head = el("div.row.pub-dock-head", { title: "Drag to move" }, [
      this.title,
      el("button.btn.icon", { type: "button", text: "_", title: "Fold away", "aria-label": "Fold away", onclick: () => this.fold() }),
      el("button.btn.icon", { type: "button", text: "×", title: "Close", "aria-label": "Close", onclick: () => this.close() }),
    ]);
    // A tap on the folded bar opens it, which is what a thumb tries first.
    head.addEventListener("click", (e) => {
      if (!e.target.closest("button") && this.node.classList.contains("min")) this.show();
    });
    this.node = el("section.pub-dock.min", { role: "dialog", "aria-label": "This browser's camera and microphone" }, [head, this.body]);
    document.body.appendChild(this.node);
    // It floats over whatever is under it, so it goes wherever it is dragged.
    this.unmove = movable(this.node, head, "gmx.pubdock.at");
    const where = `Publishing to ${channel.app || channel.id}/${this.stream}, which becomes the source ${this.source}.`;
    this.pub = mountPublisher(this.body, {
      url: `/whip/${encodeURIComponent(channel.app || channel.id)}/${encodeURIComponent(this.stream)}`,
      key,
      camera,
      ...devices,
      autostart: true,
      labels: { where, go: "Send to the mixer" },
      onState: (s) => this.stateChanged(s),
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
    this.pub.setVisible(false);
    if (this.pub.state() === "blocked") this.show();
    this.paint();
  }

  /** A device another row named: the same stream carries it from now on. */
  use({ cameraId, micId }) {
    if (cameraId) this.pub.use("video", cameraId);
    if (micId) this.pub.use("audio", micId);
    return this;
  }

  /** Open up when there is something to read, but not again for an error the operator folded away. */
  stateChanged(s) {
    this.error = s && s.state !== "live" ? s.error || null : null;
    if (this.error && this.error !== this.dismissed) this.show();
    this.paint();
  }

  /** Call `fn` with the mixer source once it exists: now, or when it arrives. */
  whenSource(fn) {
    if (fn) this.waiting.push(fn);
    this.paint();
    return this;
  }

  /** Hand the source to whoever is waiting for it, once each. */
  arrived(source) {
    const waiting = this.waiting.splice(0);
    for (const fn of waiting) Promise.resolve(fn(source)).catch((e) => errorToast(e, this.label));
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
    const word = { live: "live", connecting: "connecting", reconnecting: "reconnecting", stopped: "stopped" }[state];
    this.title.textContent = word ? `${this.label}: ${word}` : this.label;
  }

  show() {
    this.node.classList.remove("min");
    this.pub.setVisible(true);
    return this;
  }

  fold(folding = !this.node.classList.contains("min")) {
    if (folding) this.dismissed = this.error;
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
    this.unmove();
    removeEventListener("beforeunload", this.onUnload);
    this.pub.destroy();
    this.node.remove();
    this.onGone();
  }
}

/** The publisher's stylesheet, fetched with the first card and not before. */
function stylesheet() {
  if (document.querySelector("link[data-pub-css]")) return;
  const href = new URL("../../join/join.css", import.meta.url).href;
  document.head.appendChild(el("link", { rel: "stylesheet", href, "data-pub-css": "1" }));
}
