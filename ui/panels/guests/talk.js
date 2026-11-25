// Press to talk: hold a button and the operator's voice goes into one
// guest's return, or every guest's, and never to air.
//
// The voice is this browser's microphone, published to /guest/talkback/whip
// the first time a Talk button is held, as the source guest-talkback, which
// the mixer keeps muted on the programme. Its track is enabled only while a
// button is held, so between presses it carries silence. It is let go when
// the panel goes off screen. A mixer with another input chosen for
// talkback (guest.talkback.set with source) uses that instead, and nothing
// is published from here.

import { el } from "../../shell/dom.js";
import { publish, unpublish } from "../../join/whip.js";
import { storedToken } from "../../client/index.js";
import { withShow } from "../../client/transport-rpc.js";

export class Talkback {
  constructor(client, fail) {
    this.client = client;
    this.fail = fail;
    this.buttons = [];
    this.state = { source: "guest-talkback", present: false, held: [] };
    this.mic = null;
    this.held = null;
  }

  /** A press to talk button: `guest` is an id, or null for everyone. */
  button(guest, label) {
    const b = el("button.btn.guests-talk", { type: "button", text: label, title: "Hold to talk to them off air", "aria-pressed": "false" });
    const down = (e) => {
      e.preventDefault();
      this.hold(guest);
    };
    const up = () => this.letGo();
    b.addEventListener("pointerdown", down);
    for (const ev of ["pointerup", "pointerleave", "pointercancel"]) b.addEventListener(ev, up);
    b.addEventListener("keydown", (e) => (e.key === " " || e.key === "Enter") && !e.repeat && down(e));
    b.addEventListener("keyup", (e) => (e.key === " " || e.key === "Enter") && up());
    this.buttons.push({ b, guest });
    return b;
  }

  paint(state) {
    this.state = state || this.state;
    for (const { b, guest } of this.buttons) {
      const on = guest ? this.state.held.includes(guest) : this.state.held.length > 0;
      b.setAttribute("aria-pressed", on ? "true" : "false");
      b.classList.toggle("talking", on);
    }
  }

  async hold(guest) {
    this.held = { guest };
    try {
      if (this.state.source === "guest-talkback") await this.openMic();
      if (!this.held) return;
      if (this.mic) this.mic.track.enabled = true;
      const params = { on: true };
      if (guest) params.guest = guest;
      await this.client.call("guest.talkback.set", params);
    } catch (e) {
      this.held = null;
      this.fail(e);
    }
  }

  async letGo() {
    if (!this.held) return;
    const { guest } = this.held;
    this.held = null;
    if (this.mic) this.mic.track.enabled = false;
    const params = { on: false };
    if (guest) params.guest = guest;
    await this.client.call("guest.talkback.set", params).catch(() => {});
  }

  /** The microphone, published once and kept while the panel is shown. */
  async openMic() {
    if (this.mic) return;
    const stream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true }, video: false });
    const track = stream.getAudioTracks()[0];
    const url = withShow(new URL("/guest/talkback/whip", location.href)).toString();
    const got = await publish({ url, key: storedToken() || "", media: { video: null, audio: track } });
    this.mic = { track, ...got };
    await this.present();
  }

  /** Wait for the mixer to have the microphone as a source: up to fifteen
   * seconds, since the first press also makes the source and connects. */
  async present() {
    for (let i = 0; i < 150 && !this.state.present; i += 1) {
      await new Promise((r) => setTimeout(r, 100));
      if (i % 5 === 4) this.paint((await this.client.call("guest.invite.list", {}).catch(() => null))?.talkback);
    }
  }

  /** The panel went off screen: stop talking and let the microphone go. */
  release() {
    this.letGo();
    if (!this.mic) return;
    unpublish(this.mic.location);
    this.mic.pc.close();
    this.mic.track.stop();
    this.mic = null;
  }
}
