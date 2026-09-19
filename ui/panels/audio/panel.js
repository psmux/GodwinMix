// A dockable audio desk using the same gestures and meters as source tiles.
import { el } from '../../shell/dom.js';
import { audioFor, gainLabel, gainToPos } from '../../shell/fader.js';
import { addView, dropViews, meterElement, takeMeters } from '../../shell/meter.js';
import { errorToast } from '../../shell/toast.js';

class AudioPanel extends HTMLElement {
  static get panel() { return { id: 'core/audio', title: 'Audio', slots: ['main'], tag: 'gmx-audio' }; }
  setClient(client) { this.client = client; this.audio = audioFor(client); }
  connectedCallback() {
    this.rows = new Map();
    this.active = true;
    this.metered = false;
    this.master = meterElement('h');
    this.peak = el('span.num.sm.dim');
    this.list = el('div.audio-channels');
    this.empty = el('p.pad.dim', { text: 'Add a microphone or a source with audio to mix its level here.' });
    this.replaceChildren(el('div.audio-master', {}, [el('strong', { text: 'Programme' }), this.master, this.peak]), this.list, this.empty);
    this.offRender = this.client.onRender(s => this.render(s));
    this.offMeters = this.client.on('meters', data => { if (this.active) takeMeters(data); });
    this.render(this.client.state);
  }
  disconnectedCallback() {
    this.offRender?.();
    this.offMeters?.();
    dropViews('audio:');
  }
  setWorkspaceActive(active) {
    this.active = active;
    if (active) this.render(this.client.state);
    else { dropViews('audio:'); this.metered = false; for (const row of this.rows.values()) row.metered = false; }
  }
  render(state) {
    if (!this.active) return;
    const sources = (state.sources || []).filter(s => s.has_audio !== false);
    const wanted = new Set(sources.map(s => s.id));
    for (const [id, row] of this.rows) {
      if (!wanted.has(id)) { row.node.remove(); this.rows.delete(id); dropViews('audio:' + id + ':'); }
    }
    if (!this.metered) { addView('audio:master', 'program', this.master, 'h', this.peak); this.metered = true; }
    this.empty.hidden = sources.length > 0;
    for (const source of sources) {
      let row = this.rows.get(source.id);
      if (!row) { row = this.channel(source); this.rows.set(source.id, row); this.list.append(row.node); }
      row.name.textContent = source.name || source.id;
      row.muted = !!source.muted;
      row.mute.textContent = row.muted ? 'Unmute' : 'Mute';
      row.mute.setAttribute('aria-pressed', String(row.muted));
      row.mute.setAttribute('aria-label', `${row.muted ? 'Unmute' : 'Mute'} ${source.name || source.id}`);
      row.node.classList.toggle('muted', row.muted);
      const key = source.id + '/gain';
      const gain = this.audio.shown(key, source.gain === undefined ? 1 : source.gain);
      if (!this.audio.active.has(key)) row.fader.value = String(gainToPos(gain));
      const label = gainLabel(gain);
      row.level.textContent = label === 'off' ? 'Off' : label + ' dB';
      row.fader.setAttribute('aria-label', `${source.name || source.id} level`);
      row.fader.setAttribute('aria-valuetext', row.level.textContent);
      if (!row.metered) { addView('audio:' + source.id + ':meter', 'src:' + source.id, row.meter, 'h'); row.metered = true; }
    }
  }
  channel(source) {
    const name = el('strong.ellipsis', { text: source.name || source.id });
    const level = el('span.num.sm.dim');
    const meter = meterElement('h');
    const fader = el('input', { type: 'range', min: '0', max: '1', step: '0.001', value: '0.75', 'aria-label': `${source.name || source.id} level` });
    this.audio.bindFader(fader, source.id, 'gain');
    const row = { name, level, meter, fader, muted: false };
    row.mute = el('button.btn.sm', { text: 'Mute', 'aria-label': `Mute ${source.name || source.id}`, onclick: () => {
      this.audio.setMuted(source.id, !row.muted).catch(e => errorToast(e, 'Mute'));
    } });
    row.node = el('div.audio-channel', {}, [el('div.row', {}, [name, el('span.grow'), level, row.mute]), meter, fader]);
    return row;
  }
}
customElements.define('gmx-audio', AudioPanel);
if (window.godwinmixPanels) window.godwinmixPanels.push(AudioPanel);
export default AudioPanel;
