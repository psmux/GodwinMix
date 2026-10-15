// Programme on a display: the list of connected displays, and an output
// window on the one chosen showing the programme and nothing else.
//
// In the desktop app the shell owns windows and displays, so the page asks it
// with a `godwinmix://programme-output` navigation and the shell puts up a
// native menu of the displays it can see (tauri-app/src/output.rs).
//
// In a browser the displays come from the Window Management API,
// `getScreenDetails()`, which Chrome and Edge have and Firefox and Safari do
// not. Without it the page says so and offers a plain window the operator
// drags to the display. Either way the output is /programme, its own page
// with its own connection, so it carries on while this window is used,
// moved, or reloaded.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast } from "../../shell/toast.js";
import { shell } from "./fullscreen.js";

const DESKTOP = /GodwinMix-Desktop/.test(navigator.userAgent);
const KEY = "gmx.programme.display";
const NAME = "gmx-programme-output";

let output = null;

/** A display as the menu shows it: a name, its native size, and flags. */
export function describe(scr, index, remembered) {
  const ratio = scr.devicePixelRatio || 1;
  const width = Math.round(scr.width * ratio);
  const height = Math.round(scr.height * ratio);
  const name = scr.label || `Display ${index + 1}`;
  const key = `${name}@${scr.left ?? 0},${scr.top ?? 0}`;
  const notes = [];
  if (scr.isPrimary) notes.push("main display");
  if (key === remembered) notes.push("used last time");
  return { name, width, height, key, text: `${name}, ${width} × ${height}${notes.length ? ` (${notes.join(", ")})` : ""}` };
}

/** The displays, the one used last time first, or null with no API. */
export async function displays() {
  if (typeof window.getScreenDetails !== "function") return null;
  const details = await window.getScreenDetails();
  const remembered = localStorage.getItem(KEY);
  const list = details.screens.map((scr, i) => ({ scr, ...describe(scr, i, remembered) }));
  return list.sort((a, b) => (b.key === remembered) - (a.key === remembered));
}

/** View > Programme on a display, and the button on the Programme monitor. */
export async function chooseDisplay() {
  if (DESKTOP) return shell("programme-output");
  // Up at once: the first time, the browser asks the operator whether this
  // page may see the displays, and the dialog says so while it waits.
  const box = el("div.programme-displays", {}, [
    el("p.dim", { text: "Looking for displays. If the browser asks whether this page may manage windows on all your displays, answer Allow." }),
  ]);
  const dialog = modal({ title: "Programme on a display", body: box });
  const open = (item) => {
    dialog.close();
    openOutput(item && item.scr);
    if (item) localStorage.setItem(KEY, item.key);
  };
  let list = null;
  let why = "";
  try {
    list = await displays();
  } catch (e) {
    why = e && e.name === "NotAllowedError"
      ? "The browser was not allowed to list the displays. Allow window management for this page in the site settings, then try again, or use a window you drag there yourself."
      : `The browser could not list the displays: ${e && e.message ? e.message : e}.`;
  }
  box.replaceChildren();
  if (list && list.length) {
    box.append(el("p.dim", { text: "Choose the projector or monitor. The programme fills it with nothing else on it." }));
    for (const item of list) box.append(el("button.btn", { type: "button", text: item.text, onclick: () => open(item) }));
  } else {
    box.append(el("p.dim", {
      text: why || "This browser cannot list the displays. Chrome and Edge can, and so can the desktop app. Open the output in a window here, drag it to the projector, then double click it to fill the screen.",
    }));
    box.append(el("button.btn", { type: "button", text: "Open the output in a new window", onclick: () => open(null) }));
  }
  if (output && !output.closed) {
    box.append(el("button.btn", { type: "button", text: "Close the programme output", onclick: () => { dialog.close(); closeOutput(); } }));
  }
}

/** Open (or move) the output window onto `scr`, or as a plain window. */
export function openOutput(scr) {
  const url = new URL("/programme/", location.origin).href;
  const place = scr
    ? `left=${scr.availLeft ?? scr.left},top=${scr.availTop ?? scr.top},width=${scr.availWidth ?? scr.width},height=${scr.availHeight ?? scr.height},fullscreen`
    : "width=960,height=540";
  output = window.open(url, NAME, `popup,${place}`);
  if (!output) {
    toast({ text: "The browser blocked the output window. Allow pop ups for this page, then choose the display again." });
    return;
  }
  if (scr && output.moveTo) {
    try {
      output.moveTo(scr.availLeft ?? scr.left, scr.availTop ?? scr.top);
      output.resizeTo(scr.availWidth ?? scr.width, scr.availHeight ?? scr.height);
    } catch { /* the browser placed it where it could */ }
  }
}

export function closeOutput() {
  if (DESKTOP) return shell("programme-output-close");
  if (output && !output.closed) output.close();
  output = null;
}
