// The audio panel's desk: the programme master, the buses and each source's
// cue, solo and send. Mounted by panel.js; everything it draws comes from the
// `audio` part of the status snapshot and goes back through the audio.*
// methods, the same ones the CLI and an agent use.
import { el } from '../../shell/dom.js';
import { takeLevel } from '../../shell/meter.js';
import { errorToast } from '../../shell/toast.js';
import { BusStrip, addForm } from './bus-strip.js';
import { LevelGestures, levelInput } from './level.js';
import { Listener } from './listen.js';

const PICK_KEY = 'gmx.audio.bus';

document.head.appendChild(el('link', { rel: 'stylesheet', href: new URL('./desk.css', import.meta.url).href }));

function remembered() {
  try { return localStorage.getItem(PICK_KEY) || ''; } catch { return ''; }
}

function remember(id) {
  try { localStorage.setItem(PICK_KEY, id || ''); } catch { /* a private window */ }
}

export class AudioDesk {
  constructor(client) {
    this.client = client;
    this.levels = new LevelGestures();
    this.listener = new Listener(client);
    this.picked = remembered();
    this.masterFader = levelInput('Programme master');
    this.masterLevel = el('span.num.sm.dim');
    this.levels.bind(this.masterFader, 'master/gain', gain => client.call('audio.master.set', { gain }));
    this.masterMute = el('button.btn.sm', { text: 'Mute', onclick: () => this.call('audio.master.set', { muted: !this.masterMuted }, 'Mute') });
    this.picker = el('select', { 'aria-label': 'Bus', onchange: () => { this.picked = this.picker.value; remember(this.picked); this.render(this.client.state); } });
    this.adding = el('div');
    this.strip = new BusStrip(this);
    this.node = el('div.audio-desk', {}, [
      el('div.audio-master-strip.row', {}, [el('span.sm.dim', { text: 'Master' }), this.masterFader, this.masterLevel, this.masterMute]),
      el('div.row.audio-bus-pick', {}, [el('span.sm.dim', { text: 'Bus' }), this.picker, el('button.btn.sm', { text: '+ Bus', onclick: () => this.openAdd() })]),
      this.adding,
      this.strip.node,
    ]);
  }

  call(method, params, what) {
    return this.client.call(method, params).catch(e => errorToast(e, what));
  }

  openAdd() {
    this.adding.replaceChildren(addForm(this, id => {
      this.adding.replaceChildren();
      if (id) { this.picked = id; remember(id); }
    }));
  }

  audio(state) {
    return state.audio || { master: { gain: 1, muted: false }, buses: [], cue: [] };
  }

  bus(state) {
    return this.audio(state).buses.find(b => b.id === this.picked);
  }

  render(state) {
    const audio = this.audio(state);
    this.masterMuted = !!audio.master.muted;
    this.levels.show(this.masterFader, this.masterLevel, 'master/gain', audio.master.gain ?? 1);
    this.masterMute.textContent = this.masterMuted ? 'Unmute' : 'Mute';
    this.masterMute.setAttribute('aria-pressed', String(this.masterMuted));
    const ids = audio.buses.map(b => b.id);
    if (this.picker.dataset.ids !== ids.join(',')) {
      this.picker.dataset.ids = ids.join(',');
      this.picker.replaceChildren(el('option', { value: '', text: 'Programme only' }), ...audio.buses.map(b => el('option', { value: b.id, text: b.name || b.id })));
    }
    if (!ids.includes(this.picked)) this.picked = '';
    this.picker.value = this.picked;
    this.strip.show(this.bus(state));
  }

  /** A source's row gets cue, solo and a send to the picked bus. */
  decorate(row, source) {
    const send = levelInput(`${source.name || source.id} send`);
    const sendLevel = el('span.num.sm.dim');
    this.levels.bind(send, 'send/' + source.id, level => this.client.call('audio.send.set', { bus: this.picked, source: source.id, level }));
    const cue = el('button.btn.sm', { text: 'Cue', onclick: () => this.cue(source.id, !row.cued, false) });
    const solo = el('button.btn.sm', { text: 'Solo', onclick: () => this.cue(source.id, !row.soloed, true) });
    const sends = el('div.row.audio-send', {}, [el('span.sm.dim', { text: 'Send' }), send, sendLevel]);
    row.desk = { send, sendLevel, cue, solo, sends };
    row.node.append(el('div.row.audio-cue', {}, [cue, solo]), sends);
  }

  cue(source, on, solo) {
    return this.call('audio.cue.set', { source, cue: on, solo: solo && on }, 'Cue');
  }

  update(row, source, state) {
    const audio = this.audio(state);
    const marks = audio.cue || [];
    row.cued = marks.includes(source.id);
    row.soloed = row.cued && marks.length === 1;
    const hasCue = audio.buses.some(b => b.cue);
    row.desk.cue.hidden = row.desk.solo.hidden = !hasCue;
    row.desk.cue.setAttribute('aria-pressed', String(row.cued));
    row.desk.solo.setAttribute('aria-pressed', String(row.soloed));
    const bus = this.bus(state);
    row.desk.sends.hidden = !bus || bus.kind !== 'sends';
    if (bus && bus.kind === 'sends') {
      row.desk.send.setAttribute('aria-label', `${source.name || source.id} to ${bus.name || bus.id}`);
      this.levels.show(row.desk.send, row.desk.sendLevel, 'send/' + source.id, (bus.sends || {})[source.id] || 0);
    }
  }

  drop() {
    this.offMeters?.();
    this.strip.drop();
    this.listener.stop();
  }
}

/** Put the desk into an audio panel that is on screen, once the panel has
 *  fetched this module. The panel calls the desk from then on. */
export function attach(panel) {
  if (!panel.isConnected || panel.desk) return;
  const desk = new AudioDesk(panel.client);
  panel.desk = desk;
  panel.list.before(desk.node);
  for (const [id, row] of panel.rows) desk.decorate(row, { id, name: row.name.textContent });
  // Bus peaks ride in the same meters message as the sources'.
  desk.offMeters = panel.client.on('meters', data => {
    if (panel.active) for (const [id, peaks] of Object.entries(data.buses || {})) takeLevel('bus:' + id, peaks);
  });
  panel.render(panel.client.state);
}
