// The line under the preview: what is going out, read from getStats once a
// second while the publisher is on screen and live, and not otherwise.

/**
 * Read one getStats report into the numbers the line shows.
 * `prev` is the last answer, for the bit rate; `now` is in milliseconds.
 */
export function summarise(report, prev, now) {
  const out = { at: now, bytes: 0, width: 0, height: 0, fps: 0, rtt: null, kbps: 0 };
  for (const s of report.values()) {
    if (s.type === "outbound-rtp") {
      out.bytes += s.bytesSent || 0;
      if (s.kind === "video") {
        out.width = s.frameWidth || 0;
        out.height = s.frameHeight || 0;
        out.fps = s.framesPerSecond || 0;
      }
    } else if (s.type === "candidate-pair" && (s.nominated || s.selected) && s.currentRoundTripTime !== undefined) {
      out.rtt = s.currentRoundTripTime;
    }
  }
  if (prev && now > prev.at && out.bytes >= prev.bytes) {
    out.kbps = Math.round(((out.bytes - prev.bytes) * 8) / (now - prev.at));
  }
  return out;
}

/** "2480 kbps · 1280x720 at 30 fps · 4 ms round trip" */
export function statsText(s) {
  if (!s) return "";
  const parts = [`${s.kbps} kbps`];
  if (s.width) parts.push(`${s.width}x${s.height} at ${Math.round(s.fps)} fps`);
  else parts.push("no picture");
  if (s.rtt !== null) parts.push(`${Math.round(s.rtt * 1000)} ms round trip`);
  return parts.join(" · ");
}

export class StatsLine {
  /** @param {HTMLElement} node @param {() => RTCPeerConnection|null} pc */
  constructor(node, pc) {
    this.node = node;
    this.pc = pc;
    this.timer = 0;
    this.prev = null;
  }

  start() {
    if (this.timer) return;
    this.timer = setInterval(() => this.read(), 1000);
    this.read();
  }

  stop() {
    clearInterval(this.timer);
    this.timer = 0;
    this.prev = null;
    this.node.textContent = "";
  }

  async read() {
    const pc = this.pc();
    if (!pc) return;
    try {
      const now = performance.now();
      const s = summarise(await pc.getStats(), this.prev, now);
      if (this.prev && this.timer) this.node.textContent = statsText(s);
      this.prev = s;
    } catch {
      // A connection closed between the check and the read. The next tick
      // finds the new one.
    }
  }
}
