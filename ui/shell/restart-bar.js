// The bar at the top of the window that says settings are waiting for a
// restart, and restarts the mixer when the mixer can be restarted from here.
//
// What is waiting comes from `config.get`, whose `needs_restart` lists every
// key the file has that the running core does not use yet. So the bar is
// right whoever wrote the key: a preset, the settings dialog, another page,
// a person with a text editor. It asks once when the page connects and again
// whenever something that writes config calls `checkRestart`.
//
// Whether the restart is possible is `core.info` `restart.possible`. When it
// is not, the bar says why in one line and offers nothing to press, because
// there is nothing this page can do that would be true.

import { el, clear } from "./dom.js";
import { confirmModal } from "./modal.js";
import { errorToast } from "./toast.js";

let bar = null;
let dismissed = "";
let whyNot = "";

/** Ask the core what is waiting, and show or hide the bar to match. */
export async function checkRestart(client) {
  let pending = [];
  let info = null;
  try {
    [pending, info] = await Promise.all([
      client.call("config.get", {}).then((c) => c.needs_restart || []),
      client.call("core.info", {}),
    ]);
  } catch {
    // An older core, or a token without the admin scope: nothing to say.
    return hide();
  }
  if (!pending.length || pending.join() === dismissed) return hide();
  const possible = !!(info && info.restart && info.restart.possible);
  const why = possible ? "" : await reasonItCannot(client);
  show(client, { pending, possible, why });
}

/** Mount once. Every reconnect asks again, which is how it goes away after a restart. */
export function mountRestartBar(client) {
  client.on("open", () => checkRestart(client));
  checkRestart(client);
}

/** Which settings, in a line a person can read. */
export function pendingLine(keys) {
  const shown = keys.slice(0, 3).join(", ");
  const more = keys.length > 3 ? ` and ${keys.length - 3} more` : "";
  const noun = keys.length === 1 ? "setting takes" : "settings take";
  return `${keys.length} ${noun} effect when the mixer restarts: ${shown}${more}.`;
}

/**
 * The core's own reason, cut to its first sentence. The rest of it is
 * instructions for a terminal, and the person reading this bar is not in one.
 */
export function firstSentence(message) {
  const text = String(message || "").trim();
  const end = text.search(/\.(\s|$)/);
  return end < 0 ? text : text.slice(0, end + 1);
}

async function reasonItCannot(client) {
  if (whyNot) return whyNot;
  try {
    const answer = await client.call("core.restart", {});
    if (!answer.restarting) whyNot = firstSentence(answer.message);
  } catch {
    /* refused for scope or confirm; the fallback below says the same thing */
  }
  return whyNot || "This mixer cannot restart itself from here, so they apply the next time it is started.";
}

function show(client, state) {
  if (!bar) {
    bar = el("div.restart-bar", { role: "status" });
    document.body.prepend(bar);
  }
  clear(bar);
  bar.hidden = false;
  bar.appendChild(el("span.grow", { text: pendingLine(state.pending) }));
  if (state.possible) {
    bar.appendChild(el("button.btn.primary.sm", { text: "Restart now", onclick: () => restart(client) }));
  } else {
    bar.appendChild(el("span.sm.dim.why", { text: state.why }));
  }
  bar.appendChild(
    el("button.btn.icon", {
      text: "×",
      title: "Dismiss",
      "aria-label": "Dismiss",
      onclick: () => {
        dismissed = state.pending.join();
        hide();
      },
    })
  );
}

function hide() {
  if (bar) bar.hidden = true;
}

async function restart(client) {
  const sure = await confirmModal(
    "The programme goes off air while the mixer restarts, usually for a few seconds. This page reconnects by itself.",
    "Restart now"
  );
  if (!sure) return;
  try {
    const answer = await client.call("core.restart", {});
    clear(bar);
    bar.appendChild(el("span.grow", { text: answer.restarting ? "Restarting. This page reconnects when the mixer is back." : firstSentence(answer.message) }));
  } catch (e) {
    errorToast(e, "Restarting the mixer");
  }
}
