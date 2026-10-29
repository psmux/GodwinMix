// A guest's return: the programme picture with everyone's sound but their
// own, played from the mixer over WHEP while the guest is live.
//
// One POST with the offer and every candidate in it, as for the publish;
// a DELETE with the key when it is over. The mixer makes the return when the
// guest's camera first arrives, so the first offers may be answered 503 for
// a second or two: those are tried again, as is a connection that drops.

import { el } from "../shell/dom.js";

const WAITS = [1000, 2000, 4000, 8000];

export class GuestReturn {
  constructor(host, { url, key }) {
    this.url = url;
    this.key = key;
    this.video = el("video.pub-return-video", { autoplay: true, playsInline: true, "aria-label": "The show, as you hear it" });
    this.note = el("div.sm.dim", { text: "The show appears here once you are live. You hear everyone but yourself; headphones keep your microphone from picking it up." });
    this.mute = el("button.btn", { type: "button", text: "Mute the show", onclick: () => this.toggleMute() });
    this.root = el("section.col.pub-return", { "aria-label": "The show" }, [
      el("div.row", {}, [el("strong", { text: "The show" }), el("span.grow"), this.mute]),
      this.video,
      this.note,
    ]);
    host.appendChild(this.root);
    this.pc = null;
    this.location = "";
    this.want = false;
    this.tries = 0;
  }

  /** Follow the publisher: play while it is live, stop when it stops. */
  follow(state) {
    if (state === "live" && !this.want) this.start();
    if ((state === "stopped" || state === "idle") && this.want) this.stop();
  }

  start() {
    this.want = true;
    this.tries = 0;
    this.connect();
  }

  stop() {
    this.want = false;
    clearTimeout(this.timer);
    this.hangUp();
    this.video.srcObject = null;
  }

  toggleMute() {
    this.video.muted = !this.video.muted;
    this.mute.textContent = this.video.muted ? "Hear the show" : "Mute the show";
  }

  async connect() {
    this.hangUp();
    const pc = new RTCPeerConnection({ bundlePolicy: "max-bundle" });
    this.pc = pc;
    pc.addTransceiver("video", { direction: "recvonly" });
    pc.addTransceiver("audio", { direction: "recvonly" });
    pc.ontrack = (e) => {
      if (!this.video.srcObject) this.video.srcObject = new MediaStream();
      this.video.srcObject.addTrack(e.track);
    };
    pc.onconnectionstatechange = () => {
      if (pc !== this.pc) return;
      if (pc.connectionState === "connected") this.tries = 0;
      if (pc.connectionState === "failed" || pc.connectionState === "closed") this.again("The connection to the show dropped.");
    };
    try {
      await pc.setLocalDescription(await pc.createOffer());
      await gathered(pc, 3000);
      const res = await fetch(this.url, {
        method: "POST",
        headers: { "Content-Type": "application/sdp", Authorization: `Bearer ${this.key}` },
        body: pc.localDescription.sdp,
      });
      const text = await res.text();
      if (res.status !== 201) return this.again(text.trim() || `The mixer answered ${res.status}.`, res.status);
      this.location = new URL(res.headers.get("Location") || this.url, new URL(this.url, document.baseURI)).href;
      await pc.setRemoteDescription({ type: "answer", sdp: text });
      this.note.textContent = "You hear everyone but yourself. Headphones keep your microphone from picking the show up.";
    } catch (e) {
      this.again((e && e.message) || String(e));
    }
  }

  /** Try again after a wait, unless the mixer said waiting will not help. */
  again(why, status) {
    if (!this.want) return;
    this.note.textContent = why;
    if (status === 401 || status === 403 || status === 404) return this.stop();
    clearTimeout(this.timer);
    const wait = WAITS[Math.min(this.tries, WAITS.length - 1)];
    this.tries += 1;
    this.timer = setTimeout(() => this.want && this.connect(), wait);
  }

  hangUp() {
    if (this.location) {
      fetch(this.location, { method: "DELETE", keepalive: true, headers: { Authorization: `Bearer ${this.key}` } }).catch(() => {});
      this.location = "";
    }
    if (this.pc) {
      const pc = this.pc;
      this.pc = null;
      pc.close();
    }
    if (this.video.srcObject) this.video.srcObject = null;
  }
}

function gathered(pc, ms) {
  if (pc.iceGatheringState === "complete") return Promise.resolve();
  return new Promise((resolve) => {
    const done = () => {
      clearTimeout(timer);
      pc.removeEventListener("icegatheringstatechange", check);
      resolve();
    };
    const check = () => pc.iceGatheringState === "complete" && done();
    const timer = setTimeout(done, ms);
    pc.addEventListener("icegatheringstatechange", check);
  });
}
