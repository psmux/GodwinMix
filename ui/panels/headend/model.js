// The headend's arithmetic, with no DOM: a channel's one state word, its
// number, which shared input it hangs off, the groups those make, and what
// a filter keeps and in what order. Shapes are show.list and show.stats
// (docs/reference/shows.md); `input.tracks` and `input.scrambled` are read
// when a station sends them and are not needed.

import { healthOf, isLive, transport } from "../wall/model.js";

/** Every state a tile can be in, worst first, with its word. */
export const STATES = [
  ["offline", "Offline"], ["nopicture", "No picture"], ["scrambled", "Scrambled"], ["black", "Black"],
  ["frozen", "Frozen"], ["silent", "Silent"], ["starting", "Starting"], ["live", "Live"],
];
export const WORD = Object.fromEntries(STATES);
const RANK = Object.fromEntries(STATES.map(([k], i) => [k, i]));

const kinds = (show) => new Set(healthOf(show).alarms.map((a) => a.kind));

/** The one word a tile is coloured by. */
export function stateOf(show, st) {
  if (!isLive(show)) return show.state === "starting" ? "starting" : "offline";
  const k = kinds(show);
  const i = (st && st.input) || null;
  if (!i) return "starting";
  if (k.has("scrambled") || i.scrambled) return "scrambled";
  if (k.has("no-input") || (k.has("stall") && !i.kbps)) return "offline";
  if (k.has("stall") || (i.kbps && !i.video_codec)) return "nopicture";
  if (k.has("black")) return "black";
  if (k.has("freeze")) return "frozen";
  if (k.has("silence") || (i.video_codec && !i.audio_codec)) return "silent";
  if (!i.kbps) return "starting";
  return "live";
}

export const severity = (state) => RANK[state] ?? 0;

/** "117 Sport" or "CH117" read as a number and a name; else no number. */
export function numbered(show) {
  const name = String(show.name || show.id);
  const m = /^(?:ch\s?)?(\d{1,5})\b[\s:._-]*(.*)$/i.exec(name);
  return m ? { num: m[1], title: m[2] || name } : { num: "", title: name };
}

/** The address an input shares with others: its host and port, no program. */
export function groupKey(show) {
  const uri = String((show.input && show.input.uri) || "");
  const m = /^[a-z]+:\/\/(?:[^@/]*@)?([^/?#]+)/i.exec(uri);
  return m ? m[1] : uri;
}

/**
 * The shared inputs, each with what it feeds. Only an address two or more
 * channels read is a group; the rest are counted as single feeds.
 */
export function groups(shows, stats, series) {
  const by = new Map();
  for (const s of shows) {
    const key = groupKey(s);
    if (!key) continue;
    if (!by.has(key)) by.set(key, { key, transport: transport(s.input.uri), members: [], live: 0, kbps: 0, cc: 0, hist: [] });
    const g = by.get(key);
    const st = stats.get(s.id);
    g.members.push(s);
    const state = stateOf(s, st);
    if (state !== "offline" && state !== "starting") g.live += 1;
    g.kbps += (st && st.input && st.input.kbps) || 0;
    const h = series.get(s.id);
    if (h) {
      g.cc += h.ccMinute();
      h.inK.forEach((v, i) => { g.hist[i] = (g.hist[i] || 0) + v; });
    }
  }
  const all = [...by.values()];
  const shared = all.filter((g) => g.members.length > 1);
  for (const g of shared) g.dead = g.live === 0 && g.members.some(isLive);
  shared.sort((a, b) => Number(b.dead) - Number(a.dead) || a.key.localeCompare(b.key, undefined, { numeric: true }));
  return { shared, single: all.length - shared.length };
}

const hay = (show, state) => [show.name, show.id, show.input && show.input.uri, WORD[state]].join(" ").toLowerCase();

/** Which channels a filter keeps. `o` is {text, problems, group, state}. */
export function keeps(show, state, o) {
  if (o.problems && state === "live") return false;
  if (o.state && state !== o.state) return false;
  if (o.group && groupKey(show) !== o.group) return false;
  const words = String(o.text || "").toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return true;
  const h = hay(show, state);
  return words.every((w) => h.includes(w));
}

const byNumber = (a, b) => (Number(numbered(a).num) || 1e9) - (Number(numbered(b).num) || 1e9) || a.name.localeCompare(b.name);

/** Each sort's comparison, given the state and stats of each channel. */
export const SORTS = {
  number: (a, b) => byNumber(a.show, b.show),
  name: (a, b) => numbered(a.show).title.localeCompare(numbered(b.show).title) || byNumber(a.show, b.show),
  state: (a, b) => severity(a.state) - severity(b.state) || byNumber(a.show, b.show),
  rate: (a, b) => (b.kbps - a.kbps) || byNumber(a.show, b.show),
  errors: (a, b) => (b.cc - a.cc) || byNumber(a.show, b.show),
};

/** The channels to draw, in order, each with its state. */
export function channels(shows, stats, series, o) {
  const rows = shows.map((show) => {
    const st = stats.get(show.id);
    const h = series.get(show.id);
    return { show, st, state: stateOf(show, st), kbps: (st && st.input && st.input.kbps) || 0, cc: h ? h.ccMinute() : 0 };
  });
  const kept = rows.filter((r) => keeps(r.show, r.state, o));
  kept.sort(SORTS[o.sort] || SORTS.number);
  return { rows: kept, all: rows };
}

/** How many channels are in each state, for the legend. */
export function counts(rows) {
  const n = Object.fromEntries(STATES.map(([k]) => [k, 0]));
  for (const r of rows) n[r.state] += 1;
  return n;
}
