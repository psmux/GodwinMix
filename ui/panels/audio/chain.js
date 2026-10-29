// The sound filters on one source: add, set, bypass and remove, live.
//
// Every change goes through filter.add, filter.set and filter.remove, the
// same methods an agent or a script uses. A slider sends its value at most
// ten times a second while it moves and once more when it is let go. The
// panel imports this when FX is first pressed, so the page a volunteer opens
// does not carry it.
import { el } from '../../shell/dom.js';
import { modal } from '../../shell/modal.js';
import { errorToast } from '../../shell/toast.js';

// Loaded the first time FX is pressed, so its styles come with it.
document.head.appendChild(el('link', { rel: 'stylesheet', href: new URL('./chain.css', import.meta.url).href }));

// What each kind shows: its name, and the settings a person reaches for.
export const KINDS = {
  'eq/filter': { name: 'EQ', knobs: [
    ['low_cut_hz', 'Low cut', 0, 300, 5, 0, 'Hz'], ['low_gain_db', 'Bass', -12, 12, 0.5, 0, 'dB'],
    ['mid_gain_db', 'Middle', -12, 12, 0.5, 0, 'dB'], ['high_gain_db', 'Treble', -12, 12, 0.5, 0, 'dB']] },
  'gate/filter': { name: 'Noise gate', knobs: [
    ['open_threshold_db', 'Opens at', -70, 0, 1, -26, 'dB'], ['close_threshold_db', 'Closes at', -80, 0, 1, -32, 'dB'],
    ['hold_ms', 'Hold', 0, 1000, 10, 200, 'ms'], ['release_ms', 'Release', 0, 1000, 10, 150, 'ms']] },
  'compressor/filter': { name: 'Compressor', knobs: [
    ['threshold_db', 'Threshold', -60, 0, 1, -18, 'dB'], ['ratio', 'Ratio', 1, 20, 0.5, 4, ':1'],
    ['attack_ms', 'Attack', 0, 100, 1, 6, 'ms'], ['release_ms', 'Release', 10, 1000, 10, 60, 'ms'],
    ['output_gain_db', 'Make up', -12, 24, 0.5, 0, 'dB']] },
  'limiter/filter': { name: 'Limiter', knobs: [
    ['threshold_db', 'Ceiling', -20, 0, 0.5, -1, 'dB'], ['release_ms', 'Release', 10, 1000, 10, 60, 'ms']] },
  'expander/filter': { name: 'Expander', knobs: [
    ['threshold_db', 'Threshold', -70, 0, 1, -40, 'dB'], ['ratio', 'Ratio', 1, 10, 0.5, 2, ':1']] },
  'denoise/filter': { name: 'Noise suppression', knobs: [['amount', 'Amount', 0, 1, 0.05, 1, '']] },
  'loudness/filter': { name: 'Loudness target', knobs: [
    ['target_lufs', 'Target', -30, -10, 1, -14, 'LUFS'], ['max_boost_db', 'Most lift', 0, 24, 1, 12, 'dB'],
    ['speed_db_per_s', 'Speed', 0.5, 10, 0.5, 2, 'dB/s']] },
  'delay/filter': { name: 'Delay', knobs: [['delay_ms', 'Delay', 0, 2000, 5, 0, 'ms']] },
  'gain/filter': { name: 'Gain', knobs: [['gain_db', 'Gain', -30, 30, 0.5, 0, 'dB']] },
};

// One press for a speaking voice: rumble out, the room quiet between words,
// level evened out, and a ceiling.
const VOICE = [
  ['eq/filter', { low_cut_hz: 90 }],
  ['gate/filter', {}],
  ['compressor/filter', {}],
  ['limiter/filter', {}],
];

function freeId(taken, source, type) {
  const base = `${source}-${type.split('/')[0]}`;
  let id = base;
  for (let n = 2; taken.has(id); n++) id = `${base}-${n}`;
  taken.add(id);
  return id;
}

async function listFor(client, source) {
  const { filters = [] } = await client.call('filter.list', {});
  return filters.filter(f => f.source === source && KINDS[f.type]);
}

// A slider for one setting that sends while it moves, throttled.
function knob(client, filter, [key, label, min, max, step, dflt, unit]) {
  const now = filter.params?.[key] ?? dflt;
  const shown = el('span.num.sm.dim', { text: `${now} ${unit}`.trim() });
  const input = el('input', { type: 'range', min: String(min), max: String(max), step: String(step), value: String(now), 'aria-label': label });
  let timer = null;
  const send = () => {
    timer = null;
    const value = Number(input.value);
    client.call('filter.set', { id: filter.id, params: { [key]: value } }).catch(e => errorToast(e, label));
  };
  input.addEventListener('input', () => {
    shown.textContent = `${input.value} ${unit}`.trim();
    if (!timer) timer = setTimeout(send, 100);
  });
  input.addEventListener('change', () => { clearTimeout(timer); send(); });
  return el('label.chain-knob', {}, [el('span', { text: label }), input, shown]);
}

function card(client, filter, redraw) {
  const kind = KINDS[filter.type];
  let bypassed = filter.params?.bypass === true;
  const bypass = el('button.btn.sm', { text: bypassed ? 'Off' : 'On', 'aria-pressed': String(!bypassed), onclick: async () => {
    try {
      await client.call('filter.set', { id: filter.id, params: { bypass: !bypassed } });
      bypassed = !bypassed;
      bypass.textContent = bypassed ? 'Off' : 'On';
      bypass.setAttribute('aria-pressed', String(!bypassed));
    } catch (e) { errorToast(e, kind.name); }
  } });
  const remove = el('button.btn.sm', { text: 'Remove', 'aria-label': `Remove ${kind.name}`, onclick: () => {
    client.call('filter.remove', { id: filter.id }).then(redraw).catch(e => errorToast(e, 'Remove'));
  } });
  return el('div.chain-card', {}, [
    el('div.row', {}, [el('strong', { text: kind.name }), el('span.grow'), bypass, remove]),
    ...kind.knobs.map(k => knob(client, filter, k)),
  ]);
}

async function add(client, source, items) {
  const taken = new Set((await client.call('filter.list', {})).filters?.map(f => f.id) || []);
  for (const [type, params] of items) {
    await client.call('filter.add', { id: freeId(taken, source, type), type, source, params });
  }
}

export function openChain(client, id) {
  const source = client.state.sources?.find(s => s.id === id) || { id };
  const name = source.name || source.id;
  const list = el('div.chain-list');
  const picker = el('select', { 'aria-label': 'Add a sound filter' }, [
    el('option', { value: '', text: 'Add a filter…' }),
    ...Object.entries(KINDS).map(([type, k]) => el('option', { value: type, text: k.name })),
  ]);
  const redraw = async () => {
    try {
      const filters = await listFor(client, source.id);
      list.replaceChildren(...(filters.length
        ? filters.map(f => card(client, f, redraw))
        : [el('p.dim', { text: 'No sound filters yet. Voice adds a low cut, a gate, a compressor and a limiter.' })]));
    } catch (e) { errorToast(e, 'Sound filters'); }
  };
  picker.addEventListener('change', () => {
    const type = picker.value;
    picker.value = '';
    if (type) add(client, source.id, [[type, {}]]).then(redraw).catch(e => errorToast(e, KINDS[type].name));
  });
  const voice = el('button.btn.sm', { text: 'Voice', onclick: () => add(client, source.id, VOICE).then(redraw).catch(e => errorToast(e, 'Voice')) });
  modal({ title: `Sound filters: ${name}`, body: el('div.chain', {}, [el('div.row', {}, [picker, voice]), list]) });
  redraw();
}
