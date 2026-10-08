// The step before a code: letting other devices reach this mixer at all.
//
// A mixer that answers on this computer only gives a phone nothing to open.
// The card used to say which setting to change and how to start the mixer
// again; now it does it. network.share restarts the mixer on the same port,
// listening on the network (the desktop app does the restart when it is the
// app's mixer), this page reconnects by itself, and the card opens again with
// a code to scan. Where the mixer cannot be restarted from here its answer
// says why, in one sentence.

import { el } from "./dom.js";

const LOOPBACK = /^https?:\/\/(localhost|127\.[\d.]+|\[::1\])(:|\/|$)/i;

/** Whether core.info names an address other devices can open. */
function reachable(info) {
  return ((info && info.tls && info.tls.urls) || []).some((u) => !LOOPBACK.test(u));
}

/**
 * Resolves true once a mixer started after `before` says it is reachable the
 * way it was asked to be, false after `ms`.
 *
 * Not "the socket dropped and came back": the old process keeps its port
 * while it stops its shows, the page reconnects to it at once, and that read
 * as back several seconds before it was. So core.info is asked until it
 * answers from a new process (`started_ms` moved) as the new mixer would.
 */
export async function untilRestarted(client, before, wanted, ms = 60000, every = 1000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    await new Promise((r) => setTimeout(r, every));
    const info = await client.call("core.info", {}).catch(() => null);
    const fresh = info && (before === undefined || info.started_ms !== before);
    if (fresh && reachable(info) === wanted) return true;
  }
  return false;
}

/** Ask for the change, wait for the mixer to come back, then `after()`. */
async function change(client, enabled, button, status, after) {
  button.disabled = true;
  status.textContent = "";
  try {
    const before = (await client.call("core.info", {}).catch(() => ({}))).started_ms;
    const answer = await client.call("network.share", { enabled });
    if (!answer.restarting) {
      status.textContent = answer.message;
      button.disabled = false;
      return;
    }
    status.textContent = "Restarting the mixer. The programme is off air for a few seconds, and this card carries on when it is back.";
    if (!(await untilRestarted(client, before, enabled))) {
      status.textContent = "The mixer has not come back as asked within a minute. Open this card again in a moment.";
      button.disabled = false;
      return;
    }
    after();
  } catch (e) {
    status.textContent = (e && e.message) || String(e);
    button.disabled = false;
  }
}

/** The card's body while only this computer can reach the mixer. */
export function sharePrompt(client, reopen) {
  const status = el("p.sm", { role: "status" });
  const allow = el("button.btn.primary", {
    text: "Allow other devices",
    onclick: () => change(client, true, allow, status, reopen),
  });
  return el("div.col", {}, [
    el("p", { text: "Only this computer can reach the mixer right now, so a phone has nothing to open." }),
    el("p.dim", { text: "Allow other devices and phones and tablets on the same network can open it, each signed in with a code from this card. The mixer restarts once on the same address, so the programme is off air for a few seconds." }),
    el("div.row", {}, [allow]),
    status,
  ]);
}

/** At the foot of a card that already shows codes: put the mixer back to this computer only. */
export function stopSharing(client, closed) {
  const status = el("p.sm", { role: "status" });
  const stop = el("button.btn.sm", {
    text: "Stop other devices connecting",
    title: "Restart the mixer for this computer only. Phones lose it until this is allowed again; their codes are kept.",
    onclick: () => change(client, false, stop, status, closed),
  });
  return el("div.col", {}, [el("div.row", {}, [stop]), status]);
}
