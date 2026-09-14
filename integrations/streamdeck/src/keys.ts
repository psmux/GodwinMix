// What a key does and what colour it is, with no Stream Deck in it.
//
// The Elgato SDK gives an action three things: a press, a settings object, and
// two ways to draw (a title, or an image). Everything interesting is deciding
// what to draw, and that is a function of the mixer's state and the key's
// settings, so it lives here where a test can call it.

import type { Link, View } from "./link.ts";

/** The settings an operator fills in on the key's inspector. */
export interface TakeSettings {
  source?: string;
  title?: string;
}

export interface OutputSettings {
  id?: string;
  uri?: string;
  title?: string;
}

/** How a key should look right now. */
export interface KeyLook {
  /** Background colour as `#rrggbb`. */
  background: string;
  /** Text colour as `#rrggbb`. */
  foreground: string;
  /** What is written on it. */
  title: string;
  /** A short word for the state, which the tests assert on and a log prints. */
  state: string;
}

/**
 * The tally colours.
 *
 * Red for on air and green for preview are the broadcast convention, and a
 * panel that uses anything else will be misread under pressure. Amber is the
 * halfway house a reconnecting output gets, and the dark grey is a key doing
 * nothing rather than a key that is broken.
 */
export const COLOURS = {
  program: "#cc0000",
  preview: "#00aa00",
  idle: "#1a1a1a",
  live: "#00aa00",
  connecting: "#cc8800",
  reconnecting: "#cc8800",
  failed: "#cc0000",
  offline: "#3a3a3a",
  text: "#ffffff",
} as const;

/** Trimmed, or the fallback. */
function label(settings: { title?: string }, fallback: string): string {
  const title = settings.title?.trim();
  return title && title.length > 0 ? title : fallback;
}

/**
 * A take key: red while its source is on air, green while it is on preview.
 *
 * A key whose source is not a source at all stays dark and says so, rather
 * than looking the same as one that is simply off air. An operator finding a
 * typo during a show should not have to read a log.
 */
export function takeLook(view: View, settings: TakeSettings): KeyLook {
  const id = (settings.source ?? "").trim();
  const title = label(settings, id || "?");
  if (!view.connected) {
    return { background: COLOURS.offline, foreground: COLOURS.text, title, state: "offline" };
  }
  if (!id) {
    return { background: COLOURS.offline, foreground: COLOURS.text, title: "set a\nsource", state: "unset" };
  }
  const known = view.sources.some((source) => source.id === id);
  if (!known) {
    return { background: COLOURS.offline, foreground: COLOURS.text, title: `${title}\n?`, state: "unknown" };
  }
  const tally = view.tally[id] ?? "off";
  if (tally === "program") {
    return { background: COLOURS.program, foreground: COLOURS.text, title, state: "program" };
  }
  if (tally === "preview") {
    return { background: COLOURS.preview, foreground: COLOURS.text, title, state: "preview" };
  }
  return { background: COLOURS.idle, foreground: COLOURS.text, title, state: "off" };
}

/** An output toggle: green live, amber connecting or reconnecting, red failed. */
export function outputLook(view: View, settings: OutputSettings): KeyLook {
  const id = (settings.id ?? "").trim();
  const title = label(settings, id || "output");
  if (!view.connected) {
    return { background: COLOURS.offline, foreground: COLOURS.text, title, state: "offline" };
  }
  if (!id) {
    return { background: COLOURS.offline, foreground: COLOURS.text, title: "set an\noutput", state: "unset" };
  }
  const state = view.outputs[id];
  if (state === undefined) {
    // Not added, which for an output toggle is the off position rather than an
    // error: pressing the key is what adds it.
    return { background: COLOURS.idle, foreground: COLOURS.text, title, state: "stopped" };
  }
  const colour =
    state === "live"
      ? COLOURS.live
      : state === "failed"
        ? COLOURS.failed
        : COLOURS.connecting;
  return { background: colour, foreground: COLOURS.text, title, state };
}

/** The slate key: red while the programme is black. */
export function slateLook(view: View): KeyLook {
  if (!view.connected) {
    return { background: COLOURS.offline, foreground: COLOURS.text, title: "SLATE", state: "offline" };
  }
  return view.program === null
    ? { background: COLOURS.program, foreground: COLOURS.text, title: "SLATE", state: "program" }
    : { background: COLOURS.idle, foreground: COLOURS.text, title: "SLATE", state: "off" };
}

// ---------------------------------------------------------------------------
// What a press does
// ---------------------------------------------------------------------------

/** A take key. Pressing a source that is already on air does nothing. */
export async function pressTake(link: Link, view: View, settings: TakeSettings): Promise<string> {
  const id = (settings.source ?? "").trim();
  if (!id) return "no source is set on this key";
  if (view.program === id) return `'${id}' is already on air`;
  await link.take(id);
  return `took ${id}`;
}

/**
 * An output key, which is a toggle.
 *
 * Stopped becomes started by adding the destination, and started becomes
 * stopped by removing it. There is no `output.start` in the protocol because
 * the encoder is already running: adding a destination is the whole of
 * starting one, and it costs nothing on air.
 */
export async function pressOutput(link: Link, view: View, settings: OutputSettings): Promise<string> {
  const id = (settings.id ?? "").trim();
  const uri = (settings.uri ?? "").trim();
  if (!id) return "no output is set on this key";
  if (view.outputs[id] !== undefined) {
    await link.stopOutput(id);
    return `stopped ${id}`;
  }
  if (!uri) return `'${id}' is not running and this key has no address to start it with`;
  await link.startOutput(uri, id);
  return `started ${id}`;
}

export async function pressSlate(link: Link): Promise<string> {
  await link.take(null);
  return "cut to the slate";
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

/**
 * A flat colour tile with the title on it, as an SVG data URI.
 *
 * The Stream Deck SDK has no colour API: a key is a title over an image, so a
 * background colour is an image you make yourself. SVG rather than a PNG
 * because it needs no encoder, it is a few hundred bytes, and the device
 * rescales it for whichever model is plugged in.
 */
export function tile(look: KeyLook): string {
  const lines = look.title.split("\n").slice(0, 3);
  const start = 72 - (lines.length - 1) * 22;
  const text = lines
    .map(
      (line, n) =>
        `<text x="72" y="${start + n * 30}" font-family="sans-serif" font-size="26" ` +
        `font-weight="600" fill="${look.foreground}" text-anchor="middle">${escapeText(line)}</text>`,
    )
    .join("");
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144">` +
    `<rect width="144" height="144" rx="14" fill="${look.background}"/>${text}</svg>`;
  return `data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}`;
}

/** A source name with an ampersand in it must not break the SVG. */
function escapeText(text: string): string {
  return text.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c] ?? c);
}
