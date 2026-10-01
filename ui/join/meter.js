// The microphone's level, drawn as a bar.
//
// One AnalyserNode read once a frame. It runs only between `start` and
// `stop`, and the page stops it whenever the publisher is not on screen; the
// audio context is suspended then too, so a hidden meter costs nothing.

/** RMS of a block of samples as a 0 to 1 bar width, over a 60 dB range. */
export function levelOf(samples) {
  let sum = 0;
  for (let i = 0; i < samples.length; i++) sum += samples[i] * samples[i];
  const rms = Math.sqrt(sum / Math.max(1, samples.length));
  const db = 20 * Math.log10(rms || 1e-6);
  return Math.max(0, Math.min(1, (db + 60) / 60));
}

export class LevelMeter {
  /** @param {HTMLElement} bar its width is the level */
  constructor(bar) {
    this.bar = bar;
    this.ctx = null;
    this.source = null;
    this.frame = 0;
    this.running = false;
  }

  /** Measure this track from now on. Null clears it. */
  setTrack(track) {
    if (this.source) this.source.disconnect();
    this.source = null;
    this.track = track;
    if (this.running) this.wire();
    this.paint(0);
  }

  start() {
    if (this.running) return;
    this.running = true;
    this.wire();
    if (this.ctx) this.ctx.resume().catch(() => {});
    this.loop();
  }

  stop() {
    this.running = false;
    cancelAnimationFrame(this.frame);
    if (this.ctx) this.ctx.suspend().catch(() => {});
    this.paint(0);
  }

  destroy() {
    this.stop();
    if (this.source) this.source.disconnect();
    if (this.ctx) this.ctx.close().catch(() => {});
    this.ctx = null;
    this.source = null;
  }

  wire() {
    if (this.source || !this.track) return;
    if (!this.ctx) {
      const Ctx = window.AudioContext || window.webkitAudioContext;
      if (!Ctx) return;
      this.ctx = new Ctx();
      this.analyser = this.ctx.createAnalyser();
      this.analyser.fftSize = 512;
      this.samples = new Float32Array(this.analyser.fftSize);
    }
    this.source = this.ctx.createMediaStreamSource(new MediaStream([this.track]));
    this.source.connect(this.analyser);
  }

  loop() {
    if (!this.running) return;
    if (this.source && this.track && this.track.enabled) {
      this.analyser.getFloatTimeDomainData(this.samples);
      this.paint(levelOf(this.samples));
    } else {
      this.paint(0);
    }
    this.frame = requestAnimationFrame(() => this.loop());
  }

  paint(level) {
    this.bar.style.width = `${Math.round(level * 100)}%`;
    this.bar.classList.toggle("hot", level > 0.92);
  }
}
