// A fader for the desk: the programme master, a bus master, a send.
//
// The same curve and the same tug of war as a source's fader (see
// shell/fader.js): while a pointer is on the control the server never writes
// it, and for a short settle after it lifts a snapshot still carrying the old
// value is ignored. Only what is sent differs, so each fader is handed the
// function that sends it.
import { gainLabel, gainToPos, posToGain, UNITY } from '../../shell/fader.js';

const SEND_MS = 140;
const SETTLE_MS = 900;

export class LevelGestures {
  constructor() {
    this.active = new Set();
    this.settle = new Map();
    this.timers = new Map();
  }

  /** Bind a range input. `send(gain)` is called, debounced, as it moves. */
  bind(input, key, send) {
    const post = () => {
      this.timers.delete(key);
      Promise.resolve(send(posToGain(Number(input.value)))).catch(e => console.warn('the mixer refused a level', e.message));
    };
    const later = () => {
      clearTimeout(this.timers.get(key));
      this.timers.set(key, setTimeout(post, SEND_MS));
    };
    const release = () => {
      this.active.delete(key);
      this.settle.set(key, performance.now() + SETTLE_MS);
    };
    input.addEventListener('pointerdown', () => this.active.add(key));
    input.addEventListener('input', () => { this.active.add(key); later(); });
    input.addEventListener('change', () => { clearTimeout(this.timers.get(key)); post(); release(); });
    input.addEventListener('dblclick', () => { input.value = String(UNITY); post(); release(); });
    for (const ev of ['pointerup', 'pointercancel', 'blur']) input.addEventListener(ev, release);
  }

  /** Put the server's value on the fader, unless a hand is on it. */
  show(input, label, key, gain) {
    const held = this.active.has(key) || performance.now() < (this.settle.get(key) || 0);
    if (!held) input.value = String(gainToPos(gain));
    const text = gainLabel(held ? posToGain(Number(input.value)) : gain);
    if (label) label.textContent = text === 'off' ? 'Off' : text + ' dB';
    input.setAttribute('aria-valuetext', label ? label.textContent : text);
  }
}

/** A fader element of the desk's usual shape. */
export function levelInput(name) {
  const input = document.createElement('input');
  Object.assign(input, { type: 'range', min: '0', max: '1', step: '0.001', value: String(UNITY) });
  input.setAttribute('aria-label', name);
  return input;
}
