// This browser's camera and microphone as a mixer source.
//
// The publisher from /join/, mounted in a card that floats over the
// workspace, so it keeps publishing while the operator switches scenes,
// opens drawers and closes dialogs. It publishes by WHIP into the `browser`
// channel, and the channel makes the source, as it would for OBS.

import { el } from "../../shell/dom.js";
import { confirmModal } from "../../shell/modal.js";
import { errorToast } from "../../shell/toast.js";
import { mountPublisher } from "../../join/publisher.js";
import { ensureBrowserChannel, streamNameFor, sourceIdFor, streamOf } from "./browser-channel.js";

let current = null;

/** Open the card, or bring back the one already open. `camera: false` starts with the microphone alone. */
export async function openBrowserDevice(client, { camera = true } = {}) {
  if (current) return current.show();
  let got;
  try {
    got = await ensureBrowserChannel(client);
  } catch (e) {
    errorToast(e, "This browser's camera");
    return null;
  }
  if (current) return current.show();
  current = new BrowserDock(client, got, camera);
  return current;
}

class BrowserDock {
  constructor(client, { channel, key }, camera) {
    this.client = client;
    this.channel = channel;
    this.stream = streamNameFor();
    this.source = sourceIdFor(channel, this.stream);
    this.label = camera ? "This browser's camera" : "This browser's microphone";
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
      labels: { where },
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

  /** The line above the picture: what the mixer has made of the stream. */
  paint() {
    const s = streamOf(this.channel, this.stream);
    const inMixer = (this.client.state.sources || []).some((x) => x.id === this.source);
    let text = "";
    if (this.pub.active() && inMixer) text = `In the mixer as ${this.source}. Put it in a scene from Sources.`;
    else if (this.pub.active() && s && s.state === "live") text = `The channel has the stream; ${this.source} is on its way.`;
    this.note.textContent = text;
    const state = this.pub.state();
    this.title.textContent = state === "live" ? `${this.label} (live)` : this.label;
  }

  show() {
    this.node.classList.remove("min");
    this.pub.setVisible(true);
    return this;
  }

  fold() {
    const folding = !this.node.classList.contains("min");
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
