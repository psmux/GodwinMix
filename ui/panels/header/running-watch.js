// The header's live pill: what is running, counted and timed, and the way to
// stop it. Pressing it opens What is running.
//
// It knows about channels as well as the programme's outputs, because a
// channel sending a stream on to YouTube is as much "streaming" to the person
// at the desk as the programme is. It hears `channel.*` events while the page
// is open, which cost the core nothing until something changes, and reads
// `channel.list` once at open, after each event, and every ten seconds only
// while a channel is sending something.
//
// The first time the page has both answers, anything already running is
// something the person did not start from this page: left in the background
// earlier, or started from another device. That is the banner's job.

import { fmtDuration } from "../../shell/dom.js";
import { destinationsPill } from "./destinations.js";
import { runningThings, longestLive, outgoing } from "./running-model.js";

const REREAD_MS = 10000;

export function watchRunning(client, pill) {
  let channels = [];
  let read = false;
  let lastRead = 0;
  let looked = false;
  let clock = { secs: undefined, at: 0 };
  const release = client.listen ? client.listen("channel.*") : () => {};

  async function load() {
    lastRead = Date.now();
    try {
      const answer = await client.call("channel.list", {});
      channels = (answer && answer.channels) || [];
    } catch {
      // A core with no channels, or one not answering: the outputs still count.
    }
    read = true;
    paint();
  }

  function paint() {
    const state = client.state || {};
    const items = runningThings(state, channels);
    const out = outgoing(items);
    const base = destinationsPill(state.outputs || []);
    const sent = out.filter((i) => i.key.startsWith("channel:"));
    let text = base.text;
    if (sent.length) text = base.kind === "none" ? `${sent.length} sent on from channels` : `${base.text}, ${sent.length} from channels`;
    const live = base.kind === "live" || sent.some((i) => i.live);
    const secs = live ? longestLive(out) : undefined;
    if (secs !== clock.secs) clock = { secs, at: performance.now() };
    const along = clock.secs === undefined ? "" : " · " + fmtDuration(clock.secs + (performance.now() - clock.at) / 1000);
    pill.textContent = text + along;
    pill.title = [base.title, "Press to see everything running and stop it."].filter(Boolean).join("\n");
    pill.setAttribute("aria-label", `${text}. What is running`);
    pill.classList.toggle("live", live);
    pill.classList.toggle("failed", base.kind === "failed");
    if (sent.length && Date.now() - lastRead > REREAD_MS) load();
    if (!looked && read && state.connected && state.outputs) {
      looked = true;
      if (items.length) import("./banner.js").then((m) => m.showBanner(client, items));
    }
  }

  // The desktop app says so when its window comes back from the background.
  const shown = () => {
    looked = false;
    load();
  };
  window.addEventListener("godwinmix-shown", shown);
  const timer = setInterval(paint, 1000);
  const offs = [
    client.onRender(() => paint()),
    client.on("event", (e) => /^channel\./.test((e && e.name) || "") && load()),
    client.on("open", () => load()),
  ];
  pill.onclick = () => import("./running.js").then((m) => m.openRunning(client));
  load();
  return () => {
    release();
    clearInterval(timer);
    window.removeEventListener("godwinmix-shown", shown);
    for (const off of offs) off();
  };
}
