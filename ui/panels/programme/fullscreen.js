// Full screen programme in this window: the button on the Programme monitor,
// the F key, and View > Full screen programme.
//
// Esc or a double click leaves. Leaving takes the view down and gives the
// feed back; it never touches a stream, a recording or the programme, which
// carry on in the core whatever this page does.
//
// In a browser this is the Fullscreen API on the view. In the desktop app the
// page asks the shell to make its window full screen with a `godwinmix://`
// navigation, which is the one way a page served by the core reaches it.

import { on } from "../../shell/dom.js";
import { programmeView } from "./view.js";

const DESKTOP = /GodwinMix-Desktop/.test(navigator.userAgent);

let current = null;

/** Whether the programme is full screen in this window. */
export function isFull() {
  return !!current;
}

export function toggleFullscreen(client) {
  if (current) leave();
  else enter(client);
}

function enter(client) {
  const view = programmeView(client, { hint: "Esc or double click to leave full screen" });
  document.body.appendChild(view.root);
  const offs = [
    on(view.root, "dblclick", () => leave()),
    // Capture, so Esc here is not also the tray's Escape underneath.
    on(window, "keydown", (e) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      leave();
    }, true),
    // The browser's own Esc leaves its full screen without a key event.
    on(document, "fullscreenchange", () => {
      if (!document.fullscreenElement && current && current.wentFull) leave();
      else if (document.fullscreenElement === view.root && current) current.wentFull = true;
    }),
  ];
  current = { view, offs, wentFull: false };
  if (DESKTOP) shell("fullscreen?on=1");
  else if (view.root.requestFullscreen) view.root.requestFullscreen({ navigationUI: "hide" }).catch(() => {});
}

function leave() {
  const was = current;
  if (!was) return;
  current = null;
  was.offs.forEach((off) => off());
  was.view.release();
  was.view.root.remove();
  if (DESKTOP) shell("fullscreen?on=0");
  else if (document.fullscreenElement && document.exitFullscreen) document.exitFullscreen().catch(() => {});
}

/** Ask the desktop shell for something. It cancels the navigation itself. */
export function shell(what) {
  location.href = `godwinmix://${what}`;
}
