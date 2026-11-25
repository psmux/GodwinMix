// Hear a bus in this browser: `/pcm/bus:<id>` over a WebSocket, played through
// Web Audio. 48 kHz stereo F32LE in 10 ms frames, each behind a 16 byte
// header, as docs/how-to/preview-and-audio.md describes. The mixer builds the
// branch when this opens and takes it down when it closes.
import { withShow } from '../../client/transport-rpc.js';

const HEADER = 16;
const RATE = 48000;
const CHANNELS = 2;
// How far ahead of the clock playback is scheduled. Enough to ride out a late
// frame or two, short enough that headphones do not lag the room.
const LEAD_S = 0.08;

function socketUrl(client, path) {
  const transport = client.transport || {};
  const url = new URL(path, transport.base || location.origin);
  if (transport.token) url.searchParams.set('token', transport.token);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  return withShow(url).toString();
}

export class Listener {
  constructor(client) {
    this.client = client;
    this.bus = null;
  }

  get on() { return !!this.socket; }

  /** Start hearing `bus`, or stop if it is the one already playing. */
  toggle(bus, onEnd) {
    const same = this.bus === bus;
    this.stop();
    if (!same) this.start(bus, onEnd);
  }

  start(bus, onEnd) {
    this.bus = bus;
    this.ctx = new AudioContext({ sampleRate: RATE });
    this.next = 0;
    this.socket = new WebSocket(socketUrl(this.client, `/pcm/bus:${encodeURIComponent(bus)}`));
    this.socket.binaryType = 'arraybuffer';
    this.socket.onmessage = e => this.play(e.data);
    this.socket.onclose = () => { this.stop(); onEnd?.(); };
  }

  play(data) {
    const samples = new Float32Array(data, HEADER);
    const frames = samples.length / CHANNELS;
    if (!frames) return;
    const buffer = this.ctx.createBuffer(CHANNELS, frames, RATE);
    for (let c = 0; c < CHANNELS; c += 1) {
      const out = buffer.getChannelData(c);
      for (let i = 0; i < frames; i += 1) out[i] = samples[i * CHANNELS + c];
    }
    const node = this.ctx.createBufferSource();
    node.buffer = buffer;
    node.connect(this.ctx.destination);
    const now = this.ctx.currentTime;
    if (this.next < now) this.next = now + LEAD_S;
    node.start(this.next);
    this.next += buffer.duration;
  }

  stop() {
    const socket = this.socket;
    this.socket = null;
    this.bus = null;
    if (socket) { socket.onclose = null; socket.close(); }
    this.ctx?.close();
    this.ctx = null;
  }
}
