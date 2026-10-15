// /programme/: the programme alone, for a projector, an LED wall or a second
// monitor. Opened on a chosen display by View > Programme on a display, in a
// browser (panels/programme/displays.js) or by the desktop app's shell.
//
// Its own page and its own connection, so it keeps going while the operator
// works, reloads or moves the main window. It holds `ext.programme` and
// nothing else, and closing it gives that back, which takes the feed off the
// core. Streams, recordings and the programme never notice.
//
// Keys: F or a double click for full screen, Esc to close. In a browser the
// first Esc leaves full screen (the browser keeps that key) and the second
// closes the window.

import { connect, storedToken, takeTokenFromAddress } from "../client/index.js";
import { on } from "../shell/dom.js";
import { programmeView } from "../panels/programme/view.js";

const DESKTOP = /GodwinMix-Desktop/.test(navigator.userAgent);

function close() {
  if (DESKTOP) location.href = "godwinmix://programme-output-close";
  else window.close();
}

function toggleFull(root) {
  if (DESKTOP) return;
  if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
  else if (root.requestFullscreen) root.requestFullscreen({ navigationUI: "hide" }).catch(() => {});
}

async function main() {
  takeTokenFromAddress(location, history);
  const client = await connect({ token: storedToken() });
  await client.opened();
  // The same handle the operator page has, for a console or a test.
  window.gmxClient = client;
  const hint = DESKTOP ? "Esc closes this output" : "Double click or F for full screen, Esc to close";
  const view = programmeView(client, { hint, hintAtStart: false });
  document.body.appendChild(view.root);
  on(view.root, "dblclick", () => toggleFull(view.root));
  on(window, "keydown", (e) => {
    if (e.key === "Escape") close();
    else if (e.key === "f" || e.key === "F") toggleFull(view.root);
  });
  // A popup opened with the Window Management permission may go full screen
  // on its own; elsewhere the browser wants a click first, and says so.
  if (!DESKTOP) toggleFull(view.root);
  on(window, "pagehide", () => view.release());
}

main().catch((e) => {
  console.error(e);
  document.body.textContent = `The programme output could not start: ${e && e.message ? e.message : e}`;
});
