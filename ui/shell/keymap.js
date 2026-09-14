// The keyboard map.
//
// A map from a chord to a command id, saved and changeable, rather than a
// switch statement that indexes into status.sources. The old page took "3" to
// mean `status.sources[2]`, which moves when a camera is added; here "3" means
// the command `tray.take-slot` with the argument 3, and what slot 3 is stays
// the tray's business.
//
// Slot 3 is the third scene, which is what 05 section 3a asks for. With no
// scenes in the collection, or on a core with no scene server, it is the third
// input instead. "0" is the slate.
//
// Chords are written the way a person says them: "Ctrl+K", "Shift+Delete", "F2".
// Ctrl means Cmd on macOS, which is the one substitution every shortcut in 3b
// asks for.

import { accel, typing, IS_MAC } from "./dom.js";
import { run, get } from "./commands.js";

const KEY = "gmx.keys";

export const DEFAULT_MAP = {
  "Ctrl+K": "shell.palette",
  "Ctrl+N": "tray.add",
  "Ctrl+F": "tray.filter",
  "Ctrl+A": "tray.select-all",
  "Ctrl+Z": "shell.undo",
  "Ctrl+Shift+Z": "shell.redo",
  "Ctrl+Y": "shell.redo",
  "Ctrl+C": "tray.copy",
  "Ctrl+X": "tray.cut",
  "Ctrl+V": "tray.paste",
  Escape: "tray.escape",
  Delete: "tray.delete",
  Backspace: "tray.delete-mac",
  F2: "tray.rename",
  Enter: "tray.open",
  "0": "program.black",
  "1": "tray.take-slot",
  "2": "tray.take-slot",
  "3": "tray.take-slot",
  "4": "tray.take-slot",
  "5": "tray.take-slot",
  "6": "tray.take-slot",
  "7": "tray.take-slot",
  "8": "tray.take-slot",
  "9": "tray.take-slot",
  "Ctrl+,": "shell.settings",
  "?": "shell.shortcuts",
};

export function loadMap() {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return Object.assign({}, DEFAULT_MAP);
    return Object.assign({}, DEFAULT_MAP, JSON.parse(raw));
  } catch {
    return Object.assign({}, DEFAULT_MAP);
  }
}

export function saveMap(map) {
  try {
    localStorage.setItem(KEY, JSON.stringify(map));
  } catch {
    /* the map lasts the session */
  }
}

/** The chord a keydown represents, in the spelling the map uses. */
export function chordOf(e) {
  const parts = [];
  if (accel(e)) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey && e.key.length > 1) parts.push("Shift");
  let key = e.key;
  if (key === " ") key = "Space";
  // Shift+Z arrives as "Z" on most layouts and as "z" on a few. One spelling.
  if (key.length === 1 && /[a-z]/i.test(key)) {
    key = key.toUpperCase();
    if (e.shiftKey && accel(e)) parts.push("Shift");
  }
  parts.push(key);
  // Ctrl+Shift+Z can produce both branches above; fold the duplicate away.
  return [...new Set(parts)].join("+");
}

export class Keymap {
  constructor() {
    this.map = loadMap();
    this.enabled = true;
  }

  set(chord, commandId) {
    this.map[chord] = commandId;
    saveMap(this.map);
  }

  unset(chord) {
    delete this.map[chord];
    saveMap(this.map);
  }

  /** What a given command is bound to, for printing in menus. */
  keyFor(commandId) {
    const chord = Object.keys(this.map).find((k) => this.map[k] === commandId);
    if (!chord) return "";
    return IS_MAC ? chord.replace("Ctrl", "⌘").replace("Alt", "⌥") : chord;
  }

  /** Attach to a window. Returns the detach function. */
  attach(target) {
    const handler = (e) => this.handle(e);
    target.addEventListener("keydown", handler);
    return () => target.removeEventListener("keydown", handler);
  }

  /**
   * The three chords that still work while the operator is typing.
   *
   * Everything else belongs to the field: Ctrl+A selects the text, Ctrl+Z
   * undoes the typing, Delete deletes a character. Taking those from a text box
   * to drive the tray is the sort of thing that makes a person distrust a
   * keyboard.
   */
  static WHILE_TYPING = new Set(["Ctrl+K", "Ctrl+N", "Ctrl+,"]);

  handle(e) {
    if (!this.enabled) return false;
    const typed = typing(e.target);
    if (typed && !Keymap.WHILE_TYPING.has(chordOf(e))) return false;
    const chord = chordOf(e);
    let id = this.map[chord];
    // Cmd+Backspace is Delete on a Mac keyboard that has no Delete key.
    if (id === "tray.delete-mac") id = IS_MAC && accel(e) ? "tray.delete" : null;
    if (!id) return false;
    const cmd = get(id);
    if (!cmd) return false;
    if (cmd.enabled && !cmd.enabled()) return false;
    e.preventDefault();
    const arg = /^[0-9]$/.test(chord) ? Number(chord) : undefined;
    Promise.resolve(run(id, arg)).catch((err) => console.error(`command ${id} failed`, err));
    return true;
  }
}
