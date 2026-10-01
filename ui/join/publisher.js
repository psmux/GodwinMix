// The publisher: a camera and a microphone, published over WHIP to one
// address with one key.
//
// Nothing here knows whether it is the operator's own browser or a guest's.
// The caller says where to publish (`url`), with what (`key`), and what to
// call things (`labels`); the mixer's page and /join/ both mount this.

import { buildForm, fillSelect, paintState, paintMutes } from "./form.js";
import { secureProblem, remembered, remember, listDevices, openTrack, mediaErrorText } from "./devices.js";
import { Session } from "./session.js";
import { LevelMeter } from "./meter.js";
import { StatsLine } from "./stats.js";

/**
 * @param {HTMLElement} host
 * @param {{url: string, key: string, labels?: object, camera?: boolean,
 *          onState?: (s: object) => void}} opts
 * `camera: false` opens with the camera set to none, for a microphone alone.
 */
export function mountPublisher(host, opts) {
  const r = buildForm(opts.labels);
  host.appendChild(r.root);
  const problem = secureProblem();
  if (problem) return blocked(r, problem);
  return new Publisher(r, opts).controller();
}

function blocked(r, text) {
  r.error.textContent = text;
  for (const b of [r.go, r.cameraMute, r.micMute, r.camera, r.mic, r.processing]) b.disabled = true;
  return { state: () => "blocked", active: () => false, stop() {}, setVisible() {}, destroy: () => r.root.remove() };
}

class Publisher {
  constructor(r, opts) {
    this.r = r;
    this.opts = opts;
    this.tracks = { video: null, audio: null };
    this.wanted = remembered();
    if (opts.camera === false) this.wanted.camera = "off";
    this.shown = true;
    this.meter = new LevelMeter(r.level);
    this.stats = new StatsLine(r.stats, () => this.session && this.session.pc);
    this.session = null;
    this.wire();
    this.open().then(() => this.refreshDevices());
  }

  controller() {
    return {
      state: () => (this.session ? this.session.state : "idle"),
      active: () => !!this.session && this.session.active,
      stop: () => this.stopPublishing(),
      setVisible: (v) => this.setVisible(v),
      destroy: () => this.destroy(),
    };
  }

  wire() {
    const r = this.r;
    r.go.onclick = () => (this.session && this.session.active ? this.stopPublishing() : this.startPublishing());
    r.camera.onchange = () => this.switchTo("video", r.camera.value);
    r.mic.onchange = () => this.switchTo("audio", r.mic.value);
    r.processing.onchange = () => this.switchTo("audio", r.mic.value);
    r.cameraMute.onclick = () => this.toggle("video");
    r.micMute.onclick = () => this.toggle("audio");
    this.onDevices = () => this.refreshDevices();
    this.onHidden = () => this.applyVisibility();
    navigator.mediaDevices.addEventListener("devicechange", this.onDevices);
    document.addEventListener("visibilitychange", this.onHidden);
  }

  async open() {
    await this.openKind("video", this.wanted.camera, false);
    await this.openKind("audio", this.wanted.mic, false);
  }

  /** Open one device. A person's own pick is `exact`; a remembered one is a hint. */
  async openKind(kind, deviceId, exact) {
    const what = kind === "video" ? "camera" : "microphone";
    let track = null;
    try {
      track = await openTrack(kind, deviceId, { exact, processing: this.r.processing.checked });
      this.r.error.textContent = "";
    } catch (e) {
      this.r.error.textContent = mediaErrorText(e, what);
    }
    const old = this.tracks[kind];
    if (old && track) track.enabled = old.enabled;
    if (old) old.stop();
    this.tracks[kind] = track;
    if (track) track.onended = () => (this.r.error.textContent = `The ${what} stopped. Pick it again, or another one.`);
    if (this.session) await this.session.setTrack(kind, track).catch(() => {});
    this.paintTracks();
  }

  async switchTo(kind, deviceId) {
    remember(kind === "video" ? "camera" : "mic", deviceId);
    await this.openKind(kind, deviceId, true);
  }

  toggle(kind) {
    const t = this.tracks[kind];
    if (!t) return;
    t.enabled = !t.enabled;
    this.paintTracks();
  }

  async refreshDevices() {
    try {
      const { cameras, mics } = await listDevices();
      const current = (kind) => this.tracks[kind]?.getSettings?.().deviceId || (kind === "video" && !this.tracks.video ? "off" : "");
      fillSelect(this.r.camera, cameras, current("video"), "No camera");
      fillSelect(this.r.mic, mics, current("audio"), "");
    } catch (e) {
      this.r.error.textContent = mediaErrorText(e, "list of devices");
    }
  }

  paintTracks() {
    const v = this.tracks.video;
    paintMutes(this.r, this.tracks);
    this.r.noPicture.hidden = !!v && v.enabled;
    this.meter.setTrack(this.tracks.audio);
    this.applyVisibility();
  }

  startPublishing() {
    this.session = new Session({
      url: this.opts.url,
      key: this.opts.key,
      tracks: { ...this.tracks },
      onChange: (s) => this.changed(s),
    });
    this.session.start();
  }

  stopPublishing() {
    if (this.session) this.session.stop();
  }

  changed(s) {
    paintState(this.r, s);
    this.applyVisibility();
    if (this.opts.onState) this.opts.onState(s);
  }

  setVisible(v) {
    this.shown = v;
    this.applyVisibility();
  }

  /** Preview, meter and stats run only while somebody can see them. */
  applyVisibility() {
    const seen = this.shown && !document.hidden;
    const v = this.tracks.video;
    const want = seen && v ? new MediaStream([v]) : null;
    if (!want) this.r.preview.srcObject = null;
    else if (this.r.preview.srcObject?.getVideoTracks()[0] !== v) this.r.preview.srcObject = want;
    if (seen) this.meter.start();
    else this.meter.stop();
    if (seen && this.session && this.session.state === "live") this.stats.start();
    else this.stats.stop();
  }

  destroy() {
    this.stopPublishing();
    this.stats.stop();
    this.meter.destroy();
    for (const t of Object.values(this.tracks)) if (t) t.stop();
    navigator.mediaDevices.removeEventListener("devicechange", this.onDevices);
    document.removeEventListener("visibilitychange", this.onHidden);
    this.r.root.remove();
  }
}
