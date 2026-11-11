// Two pretend ATEM switchers for the demo and the tests: their inputs,
// buses, keyers, aux outputs and macros, and what each command does to
// them. An auto transition runs for its rate in frames at 25 a second.

const input = (id, name, short) => ({ id, name, short });

function inputs(n, cams) {
  const out = cams.map((c, i) => input(i + 1, c.name, c.name.slice(0, 4).toUpperCase()));
  const extra = [["Slides", "SLDS"], ["Words", "WRDS"], ["Laptop", "LPTP"], ["Input 7", "IN 7"], ["Input 8", "IN 8"]];
  for (let i = out.length; i < n; i++) out.push(input(i + 1, ...extra[i - cams.length]));
  return [input(0, "Black", "BLK"), ...out, input(1000, "Colour bars", "BARS"), input(3010, "Media player 1", "MP1")];
}

export function switcher(id, name, model, n, cams, mode) {
  return {
    id, name, model, connected: true, mode,
    inputs: inputs(n, cams),
    map: Object.fromEntries(cams.map((c, i) => [c.source, i + 1])),
    mes: [{ program: 1, preview: 2, style: "mix", rate: 30, position: 0, running: 0 }],
    dsks: [{ on: false, rate: 25 }, { on: false, rate: 25 }],
    usks: [{ on: false }, { on: false }],
    ftb: { on: false, rate: 25 },
    auxes: n > 4 ? [{ source: 1 }, { source: 4 }, { source: 0 }] : [{ source: 1 }],
    macros: n > 4 ? ["Opening", "Song words", "Speaker", "Closing", "Reset"].map((m, i) => ({ index: i, name: m })) : [],
    lastMacro: null,
  };
}

/** The state an `atem/state` call answers with: a copy, so a caller can keep it. */
export const snapshot = (s) => JSON.parse(JSON.stringify(s));

export function cut(s, me = 0) {
  const m = s.mes[me];
  [m.program, m.preview] = [m.preview, m.program];
  m.position = 0;
  m.running = 0;
}

export function auto(s, me = 0) {
  const m = s.mes[me];
  if (!m.running) m.running = m.rate / 25;
}

/** Move running transitions and fades on by `dt` seconds. */
export function stepSwitcher(s, dt) {
  for (let i = 0; i < s.mes.length; i++) {
    const m = s.mes[i];
    if (!m.running) continue;
    m.position = Math.min(1, m.position + dt / m.running);
    if (m.position >= 1) cut(s, i);
  }
}

export function dsk(s, keyer, action) {
  const k = s.dsks[keyer];
  if (!k) throw new Error(`this switcher has ${s.dsks.length} downstream keyers, numbered from 0`);
  k.on = action === "on" ? true : action === "off" ? false : !k.on;
}

export function setBus(s, bus, inputId, me = 0) {
  if (!s.inputs.some((i) => i.id === inputId)) throw new Error(`input ${inputId} is not on ${s.id}; atem/state lists its inputs`);
  s.mes[me][bus] = inputId;
}

/** The camera source on a bus, through the switcher's map, or null. */
export function sourceOn(s, bus) {
  const id = s.mes[0][bus];
  return Object.keys(s.map).find((k) => s.map[k] === id) || null;
}
