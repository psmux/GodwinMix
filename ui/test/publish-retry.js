// The publisher page's retry rules: what stops it for good, what it tries
// again, and the network and page events that make it try at once. Fake
// peer connections, no network.

import { WhipError } from "../join/whip.js";
import { Session } from "../join/session.js";

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

function fakePc() {
  const pc = new EventTarget();
  pc.connectionState = "new";
  pc.close = () => { pc.closed = true; };
  pc.go = (s) => { pc.connectionState = s; pc.dispatchEvent(new Event("connectionstatechange")); };
  return pc;
}

/** connect() answers from a list: an error to throw, `"hang"` for an offer that never answers, else a pc. */
function rig(answers) {
  const made = [];
  const states = [];
  const connect = () => {
    const next = answers.shift();
    if (next === "hang") return new Promise(() => {});
    if (next instanceof Error) return Promise.reject(next);
    const live = { pc: fakePc(), location: `/whip/c/s/s${made.length + 1}`, senders: {} };
    made.push(live);
    return Promise.resolve(live);
  };
  const s = new Session({ url: "/whip/c/s", key: "k", connect, end: () => {}, onChange: (x) => states.push(x) });
  return { s, made, states, last: () => states.at(-1) };
}

export async function publishRetryTests(test, eq, ok) {
  const gone = rig([new WhipError(404, "no channel called c"), null]);
  gone.s.start();
  await wait(0);
  test("a 404 from a mixer that is restarting is tried again, not the end", () => {
    eq([gone.last().state, gone.last().retryIn], ["reconnecting", 1000]);
  });
  gone.s.stop();

  const key = rig([new WhipError(401, "the key is wrong")]);
  key.s.start();
  await wait(0);
  test("a wrong key still stops for good", () => eq(key.last().state, "stopped"));

  const back = rig([new TypeError("Failed to fetch"), new TypeError("Failed to fetch"), null]);
  back.s.start();
  await wait(1050);
  test("with the network down, the waits grow", () => eq(back.last().retryIn, 2000));
  back.s.nudge(true);
  await wait(0);
  test("the network coming back ends the wait and publishes at once", () => {
    eq(back.made.length, 1, "a new offer went without waiting the two seconds");
    back.made[0].pc.go("connected");
    eq(back.last().state, "live");
  });
  back.s.nudge(false);
  await wait(0);
  test("a nudge leaves a connection that is up alone", () => eq(back.made.length, 1));
  back.s.stop();

  const blip = rig([null, null]);
  blip.s.start();
  await wait(0);
  blip.made[0].pc.go("connected");
  blip.made[0].pc.go("disconnected");
  await wait(100);
  test("a blip of disconnected is waited out, not re-offered", () => {
    eq(blip.last().state, "live");
    ok(!blip.made[0].pc.closed);
  });
  blip.s.nudge(true);
  await wait(0);
  test("a change of network replaces a disconnected connection at once", () => {
    ok(blip.made[0].pc.closed, "the old one is closed");
    eq(blip.made.length, 2, "and a new offer has gone");
  });
  blip.s.stop();

  const moved = rig([null, null]);
  moved.s.start();
  await wait(0);
  moved.made[0].pc.go("connected");
  moved.s.nudge(true);
  moved.made[0].pc.go("disconnected");
  await wait(20);
  test("after a change of network, disconnected is not given the grace", () => {
    eq(moved.last().state, "reconnecting");
  });
  moved.s.stop();

  const hung = rig(["hang"]);
  hung.s.start();
  hung.s.nudge(true);
  await wait(0);
  test("a nudge while an offer is on its way sends no second one", () => eq(hung.states.length, 1));
  hung.s.stop();
}
