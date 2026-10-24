// Closing a show from its tab: the ×, a middle click, and Close show in the
// tab's menu. Closing a show is what quitting the desktop app is to the
// whole machine (tauri-app/src/close.rs): nothing is deleted, its scenes,
// sources and outputs are kept for when it is opened again, and a question
// is asked only when something of it is live. With nothing live it closes at
// once. Remove show, which does delete, stays its own item with its own
// question.
//
// The station keeps which shows are closed (`show.close`, `show.open`), so a
// show closed here is closed in every browser and after a restart, and File,
// Switch show, Reopen closed show and the monitoring wall open it again.

import { el } from "./dom.js";
import { modal } from "./modal.js";
import { toast, errorToast } from "./toast.js";
import { switchTo, nav } from "./show-actions.js";

/** Whether the desktop app is around this page; a test says so itself. */
export const shell = { desktop: /GodwinMix-Desktop/.test(navigator.userAgent) };
export const LAST = "gmx.shows.closed";

export async function close(view, show) {
  if (view.shows.length < 2) return last(show);
  const running = await live(view, show);
  if (!running.length) return closeIt(view, show, false);
  const keep = await ask(show, running);
  if (keep !== null) return closeIt(view, show, keep);
}

/**
 * The last tab. In the desktop app it is the app's own Quit, with its own
 * check of what is live, the way a browser closes its window with its last
 * tab. A page in a browser has no app to quit, so it says what to do.
 */
function last(show) {
  if (shell.desktop) return nav.go("godwinmix://quit");
  toast({ text: `${show.name} is the only open show, so its tab stays. Closing this page leaves the station and the show running; to stop the show, use Stop show in its tab's menu.` });
}

/** What of this show is live, as lines in the words the Quit question uses. */
export async function live(view, show) {
  const lines = [];
  if (show.state !== "running" && show.state !== "starting") return lines;
  if (show.compositing === false) {
    if (show.on_air) lines.push("Receiving its input and sending it on");
    return lines.concat(outputLines(show.outputs));
  }
  if (show.on_air) lines.push(`On air: ${show.on_air}`);
  const own = show.id === view.current;
  const link = own ? null : (await import("../panels/routing/link.js")).showLink(view.client, show.id, view.current);
  const call = (m) => (link ? link.call(m, {}) : view.client.call(m, {})).then((a) => (Array.isArray(a) ? a : (a && (a.outputs || a.sources)) || []));
  try {
    const [outputs, sources] = await Promise.all([call("output.list"), call("source.list").catch(() => [])]);
    lines.push(...outputLines(outputs));
    for (const s of sources) {
      if ((s.type === "ingest/source" || s.type === "whip/source") && s.state === "live") lines.push(`An encoder sending in to ${s.name || s.id}`);
    }
  } catch (e) {
    // Not knowing is not the same as nothing live: ask, and say so.
    lines.push(`The show did not say what it is sending (${(e && e.message) || "no answer"}), so it may still be streaming.`);
  } finally {
    if (link) link.close();
  }
  return lines;
}

const SENDING = new Set(["live", "connecting", "reconnecting"]);

/**
 * An output of a mixed show (`output.list`: `type`) or of a direct one
 * (`show.list`: `platform`), the same rules as the desktop app's Quit: a
 * recording or a watch link counts while it is on at all, a stream while it
 * is live or dialling with a key.
 */
export function outputLines(outputs) {
  const lines = [];
  for (const o of outputs || []) {
    if (o.enabled === false || o.state === "stopped" || o.state === "off") continue;
    if (o.type === "record/output" || o.platform === "file") lines.push("Recording");
    else if (o.type === "hls/output" || o.platform === "hls") {
      const n = (o.playback && o.playback.viewers) || 0;
      lines.push(`Serving the watch link ${o.id}${n ? `, ${n} watching` : ""}`);
    } else if (o.state === "live" || (SENDING.has(o.state) && o.has_key !== false)) {
      lines.push(`Streaming to ${o.label || platform(o.uri_host) || o.id}${o.state === "live" ? ", live" : ", connecting"}`);
    }
  }
  return lines;
}

const PLATFORMS = [[/youtube/i, "YouTube"], [/facebook|fbcdn/i, "Facebook"], [/twitch|live-video\.net/i, "Twitch"], [/kick/i, "Kick"], [/linkedin/i, "LinkedIn"], [/vimeo/i, "Vimeo"], [/restream/i, "Restream"]];
const platform = (host) => (PLATFORMS.find(([re]) => re.test(String(host || ""))) || [])[1];

/** The question, naming what is live. true keeps it running, false stops it, null is Cancel. */
function ask(show, lines) {
  return new Promise((resolve) => {
    let answered = false;
    const done = (v) => {
      answered = true;
      m.close();
      resolve(v);
    };
    const cancel = el("button.btn", { text: "Cancel", onclick: () => done(null) });
    const m = modal({
      title: `${show.name} is still running`,
      body: el("div.show-close", {}, [
        el("ul", {}, lines.map((t) => el("li", { text: t }))),
        el("p", { text: "Stop it and close ends every stream and recording of this show and stops it. Close and keep it running takes its tab away and leaves it on air. Either way nothing is deleted: its scenes, sources and outputs are kept, and File, Switch show opens it again." }),
      ]),
      footer: [
        el("button.btn.danger", { text: "Stop it and close", onclick: () => done(false) }),
        el("button.btn", { text: "Close and keep it running", onclick: () => done(true) }),
        cancel,
      ],
      onClose: () => {
        if (!answered) resolve(null);
      },
    });
    cancel.focus();
  });
}

/**
 * `show.close`. Stopping the page's own show closes this page's socket
 * before the answer comes back, so that is asked over a line to the show the
 * page moves to, as Remove show does, and the page then moves there: the tab
 * to the right, or the one to the left at the end, as a browser does.
 */
async function closeIt(view, show, keep) {
  const at = view.shows.findIndex((s) => s.id === show.id);
  const next = view.shows[at + 1] || view.shows[at - 1];
  const own = show.id === view.current;
  const via = own ? (await import("../panels/routing/link.js")).showLink(view.client, next.id, view.current) : { call: (m, p) => view.client.call(m, p), close() {} };
  try {
    await via.call("show.close", { id: show.id, keep_running: keep });
  } catch (e) {
    return errorToast(e, `Closing ${show.name}`);
  } finally {
    via.close();
  }
  try { localStorage.setItem(LAST, show.id); } catch { /* Reopen closed show lists them instead */ }
  if (own) return switchTo(next.id);
  view.read();
  toast({ text: `${show.name} closed${keep ? ", and still running" : ""}.`, action: { label: "Reopen", run: () => reopen(view.client, show.id) } });
}

/** `show.open`, then the page goes there. */
export async function reopen(client, id) {
  try {
    await client.call("show.open", { id });
  } catch (e) {
    return errorToast(e, "Reopening the show");
  }
  switchTo(id);
}
