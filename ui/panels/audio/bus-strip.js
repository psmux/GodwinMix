// One bus on the desk: its meter, master, mute, sound card and a listen
// button, and the small form that adds a bus.
import { el } from '../../shell/dom.js';
import { addView, dropViews, meterElement } from '../../shell/meter.js';
import { errorToast } from '../../shell/toast.js';
import { levelInput } from './level.js';

const KINDS = { sends: 'Sends', minus: 'Mix minus', cue: 'Cue' };

export class BusStrip {
  constructor(desk) {
    this.desk = desk;
    this.client = desk.client;
    this.label = el('strong.ellipsis');
    this.kind = el('span.num.sm.dim');
    this.meter = meterElement('h');
    this.level = el('span.num.sm.dim');
    this.fader = levelInput('Bus level');
    desk.levels.bind(this.fader, 'bus/gain', gain => this.client.call('audio.bus.set', { id: this.id, gain }));
    this.mute = el('button.btn.sm', { text: 'Mute', onclick: () => this.call('audio.bus.set', { id: this.id, muted: !this.muted }, 'Mute') });
    this.listen = el('button.btn.sm', { text: 'Listen', title: 'Hear this bus in this browser', onclick: () => this.toggleListen() });
    this.device = el('select.sm', { 'aria-label': 'Sound card', onfocus: () => this.loadDevices(), onchange: () => this.call('audio.bus.set', { id: this.id, device: this.device.value }, 'Sound card') });
    this.state = el('p.sm.dim');
    this.remove = el('button.btn.sm', { text: 'Remove', onclick: () => this.call('audio.bus.remove', { id: this.id }, 'Remove bus') });
    this.node = el('div.audio-bus', {}, [
      el('div.row', {}, [this.label, this.kind, el('span.grow'), this.level, this.mute]),
      this.meter,
      this.fader,
      el('div.row.wrap', {}, [this.listen, this.device, el('span.grow'), this.remove]),
      this.state,
    ]);
  }

  call(method, params, what) {
    return this.client.call(method, params).catch(e => errorToast(e, what));
  }

  show(bus) {
    if (!bus) { this.node.hidden = true; this.drop(); return; }
    this.node.hidden = false;
    if (this.id !== bus.id) { this.drop(); addView('audio:bus', 'bus:' + bus.id, this.meter, 'h'); }
    this.id = bus.id;
    this.muted = !!bus.muted;
    this.label.textContent = bus.name || bus.id;
    this.kind.textContent = bus.kind === 'minus' ? `without ${bus.minus}` : KINDS[bus.kind] || bus.kind;
    this.mute.textContent = this.muted ? 'Unmute' : 'Mute';
    this.mute.setAttribute('aria-pressed', String(this.muted));
    this.desk.levels.show(this.fader, this.level, 'bus/gain', bus.gain ?? 1);
    this.showDevice(bus.device || '');
    const outs = (bus.outputs || []).length ? `Sound of ${bus.outputs.join(', ')}. ` : '';
    this.state.textContent = outs + (bus.device_state || '');
    this.listen.setAttribute('aria-pressed', String(this.desk.listener.bus === bus.id));
  }

  showDevice(current) {
    if (![...this.device.options].some(o => o.value === current)) {
      this.device.replaceChildren(...this.baseOptions(), ...(current && current !== 'default' ? [el('option', { value: current, text: current })] : []));
    }
    if (document.activeElement !== this.device) this.device.value = current;
  }

  baseOptions() {
    return [el('option', { value: '', text: 'No sound card' }), el('option', { value: 'default', text: 'System default' })];
  }

  async loadDevices() {
    if (this.loaded) return;
    this.loaded = true;
    try {
      const { devices } = await this.client.call('audio.device.list', {});
      const keep = this.device.value;
      this.device.replaceChildren(...this.baseOptions(), ...devices.map(d => el('option', { value: d.id, text: d.name })));
      this.device.value = keep;
    } catch (e) { this.loaded = false; errorToast(e, 'Sound cards'); }
  }

  toggleListen() {
    this.desk.listener.toggle(this.id, () => this.listen.setAttribute('aria-pressed', 'false'));
    this.listen.setAttribute('aria-pressed', String(this.desk.listener.bus === this.id));
  }

  drop() { dropViews('audio:bus'); }
}

/** The form that adds a bus: an id and what kind of mix it is. */
export function addForm(desk, done) {
  const id = el('input', { placeholder: 'monitor', 'aria-label': 'Bus id', pattern: '[a-z0-9]+(-[a-z0-9]+)*', required: true });
  const kind = el('select', { 'aria-label': 'Kind' }, Object.entries(KINDS).map(([v, t]) => el('option', { value: v, text: t })));
  const minus = el('select', { 'aria-label': 'Leave out', hidden: true });
  kind.addEventListener('change', () => {
    minus.hidden = kind.value !== 'minus';
    minus.replaceChildren(...(desk.client.state.sources || []).map(s => el('option', { value: s.id, text: s.name || s.id })));
  });
  const submit = async e => {
    e.preventDefault();
    const params = { id: id.value.trim() };
    if (kind.value === 'cue') params.cue = true;
    if (kind.value === 'minus') params.minus = minus.value;
    try { await desk.client.call('audio.bus.add', params); done(params.id); } catch (err) { errorToast(err, 'Add bus'); }
  };
  return el('form.audio-bus-add.row.wrap', { onsubmit: submit }, [id, kind, minus, el('button.btn.sm', { type: 'submit', text: 'Add' }), el('button.btn.sm', { type: 'button', text: 'Cancel', onclick: () => done(null) })]);
}
