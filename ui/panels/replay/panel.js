// The Replay panel: the angles with how much each holds, mark in and out,
// the events, play at a speed, change the angle while it plays, and save.
// It reads `replay.get` once a second while it is on screen and not at all
// otherwise.

import { el } from "../../shell/dom.js";
import { toast, errorToast } from "../../shell/toast.js";
import { setup } from "./setup.js";
import { angles } from "./angles.js";
import { events } from "./events.js";
import { controls } from "./controls.js";

const CSS_ID = "gmx-replay-css";
const SPEED_KEY = "gmx.replay.speed";

/** What the panel and the keys share: the speed and the angle picked. */
export const picked = { speed: readSpeed(), angle: "" };

function readSpeed() {
  try {
    const v = Number(localStorage.getItem(SPEED_KEY));
    return v >= 0.1 && v <= 1 ? v : 1;
  } catch {
    return 1;
  }
}

export function setSpeed(v) {
  picked.speed = v;
  try { localStorage.setItem(SPEED_KEY, String(v)); } catch { /* kept for the session */ }
}

const client = () => window.gmxClient;
const fail = (e) => errorToast(e, "Replay");

export function call(method, params) {
  return client().call(method, params || {}).catch((e) => { fail(e); throw e; });
}

/** A call whose refusal is expected and says nothing: a change of angle or
 * speed with no replay playing. */
export function quiet(method, params) {
  return client().call(method, params || {}).catch(() => null);
}

export function markIn() {
  return call("replay.mark", { point: "in", angle: picked.angle || undefined }).then(() => toast({ text: "Replay: in point marked" }), () => {});
}

export function markOut() {
  return call("replay.mark", { point: "out" }).then((e) => toast({ text: `Replay: ${e.name} marked` }), () => {});
}

export async function playLast() {
  const list = await call("replay.event.list").catch(() => null);
  const done = list && [...list.events].reverse().find((e) => e.out_ms);
  return play(done ? done.id : undefined);
}

/** Play an event, or the last ten seconds, at the speed picked. */
export function play(event, take) {
  const params = { event, speed: picked.speed, angle: picked.angle || undefined, take: !!take };
  return call("replay.play", params).catch(() => {});
}

class View {
  constructor(host) {
    this.client = host.client;
    this.timer = 0;
    this.setup = setup(() => this.refresh());
    this.angles = angles((id) => { picked.angle = id; this.refresh(); });
    this.events = events(() => this.refresh());
    this.controls = controls(() => this.refresh());
    this.root = el("div.col.pad.replay-panel", {}, [this.setup.root, this.controls.root, this.angles.root, this.events.root]);
    host.append(this.root);
  }

  start() {
    if (this.timer) return;
    this.refresh();
    this.timer = setInterval(() => this.refresh(), 1000);
  }

  stop() {
    clearInterval(this.timer);
    this.timer = 0;
  }

  async refresh() {
    let status;
    try {
      status = await this.client.call("replay.get", {});
    } catch {
      return;
    }
    if (picked.angle && !status.angles.some((a) => a.id === picked.angle)) picked.angle = "";
    this.setup.paint(status, this.client.state.sources || []);
    this.controls.paint(status);
    this.angles.paint(status, picked.angle);
    this.events.paint(status);
  }
}

export function mount(host) {
  if (!document.getElementById(CSS_ID)) {
    document.head.appendChild(el("link#" + CSS_ID, { rel: "stylesheet", href: new URL("./replay.css", import.meta.url).href }));
  }
  if (!host.view) host.view = new View(host);
  if (host.isConnected) host.view.start();
}
