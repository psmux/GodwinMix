// The publisher: a camera and a microphone, published over WHIP to one
// address with one key.
//
// Nothing here knows whether it is the operator's own browser or a guest's.
// The caller says where to publish (`url`), with what (`key`), and what to
// call things (`labels`); the mixer's page and /join/ both mount this.

import { buildForm, paintState, paintMutes, blocked } from "./form.js";
import { secureProblem, remembered, remember } from "./devices.js";
import { openKind, refreshDevices } from "./tracks.js";
import { Session } from "./session.js";
import { LevelMeter } from "./meter.js";
import { StatsLine } from "./stats.js";
import { ScreenAwake } from "./wake.js";
import { facingOf } from "./flip.js";
import { wire } from "./wiring.js";
import { Shaper } from "./shaper.js";
import { SHAPES, startShape } from "./shape.js";
import { shapeControls, rememberedShape } from "./shape-controls.js";

/**
 * @param {HTMLElement} host
 * @param {{url: string | (() => string), key: string, labels?: object,
 *          camera?: boolean, autostart?: boolean, keepAwake?: boolean,
 *          cameraId?: string, micId?: string,
 *          onState?: (s: object) => void}} opts
 * `url` may be a function, read each time publishing starts: /join/ on a
 * phone lets the person rename the device until then.
 * `camera: false` opens with the camera set to none, for a microphone alone.
 * `autostart` publishes as soon as the devices are open, with no button to
 * press: the mixer's own page uses it, because there the browser is a device
 * like any other and picking it is the whole of adding it.
 * `keepAwake` holds a screen wake lock while publishing, for a phone.
 * `shape: {link}` sends the camera through a canvas of a shape the person
 * picks, held upright whatever the phone does (`shaper.js`). `link` is the
 * shape the mixer's link asked for. /join/ uses it; the mixer's own page,
 * where the camera is a laptop's, does not.
 */
export function mountPublisher(host, opts) {
  const r = buildForm(opts.labels);
  host.appendChild(r.root);
  const problem = secureProblem();
  if (problem) return blocked(r, problem);
  return new Publisher(r, opts).controller();
}

class Publisher {
  constructor(r, opts) {
    this.r = r;
    this.opts = opts;
    this.tracks = { video: null, audio: null };
    this.wanted = remembered();
    if (opts.camera === false) this.wanted.camera = "off";
    else if (opts.cameraId) this.wanted.camera = opts.cameraId;
    if (opts.micId) this.wanted.mic = opts.micId;
    this.shown = true;
    this.meter = new LevelMeter(r.level);
    this.stats = new StatsLine(r.stats, () => this.session && this.session.pc);
    this.session = null;
    this.awake = opts.keepAwake ? new ScreenAwake() : null;
    if (opts.shape) this.shapeUp(opts.shape.link || "");
    this.unwire = wire(this);
    this.open().then(() => {
      refreshDevices(this);
      if (opts.autostart && (this.tracks.video || this.tracks.audio)) this.startPublishing();
    });
  }

  /** The canvas stage and its controls, under the picture. */
  shapeUp(link) {
    const mem = rememberedShape();
    this.shaper = new Shaper({ shape: startShape(mem, link), fit: mem.fit, auto: mem.auto });
    this.r.preview.hidden = true;
    this.r.picture.append(this.shaper.canvas, this.shaper.video);
    this.shapeUi = shapeControls(this.shaper, link);
    this.r.picture.after(this.shapeUi.node);
  }

  /** What goes on the wire for a track: the canvas's in place of the camera's. */
  sendable(kind, track) {
    return kind === "video" && this.shaper && track ? this.shaper.track : track;
  }

  controller() {
    return {
      state: () => (this.session ? this.session.state : "idle"),
      active: () => !!this.session && this.session.active,
      stop: () => this.stopPublishing(),
      setVisible: (v) => this.setVisible(v),
      use: (kind, deviceId) => this.switchTo(kind, deviceId),
      destroy: () => this.destroy(),
    };
  }

  async open() {
    await openKind(this, "video", this.wanted.camera, false);
    await openKind(this, "audio", this.wanted.mic, false);
  }

  switchTo(kind, deviceId) {
    remember(kind === "video" ? "camera" : "mic", deviceId);
    return openKind(this, kind, deviceId, true);
  }

  toggle(kind) {
    const t = this.tracks[kind];
    if (!t) return;
    t.enabled = !t.enabled;
    this.paintTracks();
  }

  paintTracks() {
    const v = this.tracks.video;
    paintMutes(this.r, this.tracks);
    this.r.noPicture.hidden = !!v && v.enabled;
    // A mirror for the camera facing the person, as every phone shows it.
    this.r.preview.classList.toggle("back", facingOf(v) === "environment");
    if (this.shaper) {
      this.shaper.canvas.classList.toggle("back", facingOf(v) === "environment");
      const { w, h } = SHAPES[this.shaper.s.shape];
      this.r.picture.style.aspectRatio = `${w} / ${h}`;
    }
    this.meter.setTrack(this.tracks.audio);
    this.applyVisibility();
  }

  startPublishing() {
    const url = this.opts.url;
    this.session = new Session({
      url: typeof url === "function" ? url() : url,
      key: this.opts.key,
      tracks: { video: this.sendable("video", this.tracks.video), audio: this.tracks.audio },
      onChange: (s) => this.changed(s),
    });
    this.session.start();
  }

  stopPublishing() {
    if (this.session) this.session.stop();
  }

  changed(s) {
    paintState(this.r, s, this.opts.labels);
    if (this.awake) this.awake.want(this.session && this.session.active);
    this.applyVisibility();
    if (this.shapeUi) this.shapeUi.setLive(!!this.session && this.session.active);
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
    // With a canvas stage the canvas is the preview, and this video stays empty.
    const want = seen && v && !this.shaper ? new MediaStream([v]) : null;
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
    if (this.awake) this.awake.destroy();
    for (const t of Object.values(this.tracks)) if (t) t.stop();
    if (this.shaper) this.shaper.destroy();
    if (this.shapeUi) this.shapeUi.destroy();
    this.unwire();
    this.r.root.remove();
  }
}
