// /prompter/: the teleprompter, full screen on a phone, a tablet or the
// monitor under the glass. Any number of these follow one shared state, and
// each scrolls by itself from it (follow.js), so they stay together without
// the mixer sending anything while the words move.
//
// Keys (keys.js): Space plays and pauses, the arrows change the speed, Page
// Down and Page Up go to the next and the last marker, M mirrors this
// screen, F is full screen. A pedal that sends those keys works the same.

import { storedToken, takeTokenFromAddress } from "../client/index.js";
import { ScreenAwake } from "../join/wake.js";
import { MixerClock, positionAt } from "./clock.js";
import { controls, MIRROR_CHOICES } from "./controls.js";
import { Follower } from "./follow.js";
import { bindKeys } from "./keys.js";
import { admitted, Link } from "./link.js";
import { Stage } from "./stage.js";

const MIRROR_KEY = "gmx.prompter.mirror";
const RESYNC_MS = 60000;

function stored(key, fallback) {
  try {
    return localStorage.getItem(key) || fallback;
  } catch {
    return fallback;
  }
}

function keep(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* this screen forgets at the next load */
  }
}

class Prompter {
  constructor(root, token) {
    this.stage = new Stage(root);
    this.link = new Link(token, { open: () => this.opened(), close: () => this.closed(), event: (n, p) => this.event(n, p) });
    this.clock = new MixerClock(() => this.link.call("prompter.get"));
    this.follower = new Follower(this.stage, this.clock);
    this.bar = controls(root, (move) => this.run(move));
    this.awake = new ScreenAwake();
    this.state = null;
    this.rowsKey = "";
    this.mirror = stored(MIRROR_KEY, "desk");
    bindKeys(window, (move) => this.run(move));
    window.addEventListener("resize", () => this.relayout());
    this.stage.say("Connecting to the mixer");
  }

  async opened() {
    this.state = null;
    await this.clock.sync();
    this.take(await this.link.call("prompter.get").catch(() => null), true);
    this.awake.want(true);
    clearInterval(this.resync);
    this.resync = setInterval(() => this.clock.sync().then(() => this.follower.resync()), RESYNC_MS);
  }

  closed() {
    clearInterval(this.resync);
    this.awake.want(false);
    this.stage.say("Lost the mixer. Trying again");
  }

  event(name, params) {
    if (name === "prompter.changed") this.take(params);
    else if (name === "prompter.script.changed" && this.state && params.id === this.state.script) this.rowsKey = "";
  }

  /** A state from an answer or an event. An older one than we have is dropped. */
  async take(state, fresh) {
    if (!state || (!fresh && this.state && state.revision < this.state.revision)) return;
    this.state = state;
    const key = `${state.script}|${state.updated_ms}|${state.line_chars}`;
    if (key !== this.rowsKey) {
      this.rowsKey = key;
      const answer = state.script ? await this.link.call("prompter.rows").catch(() => null) : null;
      this.stage.setRows(answer ? answer.rows : []);
    }
    this.stage.say(state.script ? "" : "No script on the prompter. Load one from the desk's Prompter panel.");
    this.relayout();
  }

  relayout() {
    const s = this.state;
    const mirrored = this.mirror === "desk" ? !!(s && s.mirror) : this.mirror === "on";
    if (s) this.stage.setLook({ line_chars: s.line_chars, margin: s.margin, cue: s.cue }, mirrored);
    if (s) this.follower.apply(s);
    this.paint();
  }

  paint() {
    const s = this.state;
    const line = s && s.script
      ? `${s.title || s.script}, row ${Math.floor(positionAt(s, this.clock.now())) + 1} of ${s.rows}, ${s.speed.toFixed(1)} rows a second`
      : "";
    this.bar.paint({ playing: !!(s && s.playing), mirrorChoice: this.mirror, line });
  }

  async run(move) {
    this.bar.show();
    if (move.local === "mirror") {
      const at = MIRROR_CHOICES.findIndex(([k]) => k === this.mirror);
      this.mirror = MIRROR_CHOICES[(at + 1) % MIRROR_CHOICES.length][0];
      keep(MIRROR_KEY, this.mirror);
      return this.relayout();
    }
    if (move.local === "fullscreen") return full();
    const method = move.method === "toggle" ? (this.state && this.state.playing ? "prompter.pause" : "prompter.play") : move.method;
    try {
      this.take(await this.link.call(method, move.params));
    } catch (e) {
      const scope = e && e.code === -32002;
      this.bar.say(scope ? "This screen's token can follow the prompter but not drive it. Drive it from the desk, or open this page with an operator token." : (e && e.message) || String(e));
    }
  }
}

function full() {
  if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
  else document.documentElement.requestFullscreen?.({ navigationUI: "hide" }).catch(() => {});
}

async function main() {
  takeTokenFromAddress(location, history);
  const token = storedToken();
  const root = document.getElementById("prompter");
  const prompter = new Prompter(root, token);
  window.gmxPrompter = prompter;
  const door = await admitted(token);
  if (door === "token") {
    prompter.stage.say("This mixer wants a token. Open this page from the desk's Prompter panel, or add #token= and a viewer token to the address.");
    return;
  }
  prompter.link.open();
}

main().catch((e) => {
  console.error(e);
  document.body.textContent = `The prompter could not start: ${e && e.message ? e.message : e}`;
});
