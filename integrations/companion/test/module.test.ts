// The Companion module against a fake core.
//
// `test/fake-core.ts` is the harness from `clients/typescript/test`, copied
// rather than imported so this package can be lifted out of the repository
// whole. It is a real HTTP server on a real port upgrading a real socket, so
// the module is exercised over the wire.
//
// Nothing here imports `@companion-module/base`. `src/index.ts` is the only
// file that does, and it holds no behaviour: it hands Companion the tables in
// `definitions.ts` and forwards its callbacks into `link.ts`. Both of those
// are what these tests drive.

import assert from "node:assert/strict";
import { after, describe, it, mock } from "node:test";

import { ACTIONS, FEEDBACKS, RGB, VARIABLES, presets, variableValues } from "../src/definitions.ts";
import { Link, clock, emptyView, viewOf, type View } from "../src/link.ts";
import { fakeCore, snapshot, type Connection, type FakeCore } from "./fake-core.ts";

const links: Link[] = [];
const cores: FakeCore[] = [];

after(async () => {
  for (const link of links) link.close();
  for (const core of cores) await core.stop();
});

async function core(opts: Parameters<typeof fakeCore>[0] = {}): Promise<FakeCore> {
  const made = await fakeCore(opts);
  cores.push(made);
  made.answer("core.subscribe", { seq: 41, events: ["*"], ignored_ext: [] });
  return made;
}

/**
 * A link that has connected, subscribed and seen the first flush.
 *
 * `link.open()` resolves on the socket opening, and the subscribe goes out
 * after that, so a test that only awaits `open()` is racing the session. This
 * waits for the first flush that carries a programme, which is the condition
 * every assertion below actually depends on.
 */
async function linked(
  fake: FakeCore,
  onChange?: (view: View) => void,
): Promise<Link> {
  let settled: () => void = () => {};
  const first = new Promise<void>((resolve) => {
    settled = resolve;
  });
  const link = new Link({
    base: fake.url,
    token: "t",
    onChange: (view) => {
      onChange?.(view);
      if (view.program !== null) settled();
    },
  });
  links.push(link);
  await link.open();
  await first;
  return link;
}

/**
 * The snapshot, a tally document, and the flush that ends the batch.
 *
 * Sent to the one client that subscribed. Broadcasting it would mean a second
 * panel connecting resets the first panel's view, and which of the two orders
 * that lands in is a race nobody should be asked to reason about.
 */
function serveSession(_fake: FakeCore, conn: Connection): void {
  conn.notify("event/snapshot", snapshot());
  conn.notify("event/tally", { sources: { cam1: "program", cam2: "off" } });
  conn.notify("event/flush", { seq: 44 });
}

// ---------------------------------------------------------------------------

describe("the link", () => {
  it("reads the mixer's state into the flat view a panel wants", async () => {
    const fake = await core({ onSubscribe: serveSession });
    const link = await linked(fake);

    assert.equal(link.view.connected, true);
    assert.equal(link.view.program, "cam1");
    assert.equal(link.view.tally.cam1, "program");
    assert.equal(link.view.tally.cam2, "off");
    assert.equal(link.view.outputs.twitch, "live");
    assert.deepEqual(
      link.view.sources.map((s) => s.id),
      ["cam1", "cam2"],
    );
    assert.equal(link.view.uptimeSecs, 12);
  });

  it("asks for the tally stream and for the events that carry it", async () => {
    const fake = await core({ onSubscribe: serveSession });
    const link = new Link({ base: fake.url, token: "t" });
    links.push(link);
    await link.open();

    // Waited for, not assumed. `open()` resolves on the socket, and the
    // subscribe goes out after it, so looking in `calls` straight away is a
    // race that passes on an idle machine and fails on a busy one.
    const subscribe = await fake.waitForCall("core.subscribe");
    const events = subscribe.params.events as string[];
    assert.ok(events.includes("tally"), `tally is in ${events.join(", ")}`);
    assert.ok(
      events.includes("program.*"),
      "the core sends the tally on the back of program.took, so that has to be asked for too",
    );
    const ext = subscribe.params.ext as Record<string, unknown>;
    assert.equal(ext.tally, true, "nothing runs in the core unless a client asks");
  });

  it("lights a tally from the programme when the core sends no tally document", () => {
    // A core built without the derived tally, or a client that did not ask for
    // it. The button must still be right.
    const view = viewOf(
      { program: "cam2", sources: [{ id: "cam1" }, { id: "cam2" }], tally: {}, outputs: [] } as never,
      true,
    );
    assert.equal(view.tally.cam2, "program");
    assert.equal(view.tally.cam1, "off");
  });

  it("reads an unknown tally value as off rather than guessing", () => {
    const view = viewOf({ program: null, sources: [], tally: { cam1: 7 }, outputs: [] } as never, true);
    assert.equal(view.tally.cam1, "off");
  });
});

describe("actions", () => {
  it("take sends program.take with the source the operator typed", async () => {
    const fake = await core({ onSubscribe: serveSession });
    fake.answer("program.take", { program: "cam2", running_time_ms: 1500 });
    const link = await linked(fake);

    await ACTIONS.take.run(link, { source: "cam2" });
    const call = fake.calls.find((c) => c.method === "program.take");
    assert.deepEqual(call?.params, { source: "cam2" });
  });

  it("an empty source cuts to the slate rather than taking a source called nothing", async () => {
    const fake = await core({ onSubscribe: serveSession });
    fake.answer("program.take", { running_time_ms: 0 });
    const link = await linked(fake);

    await ACTIONS.take.run(link, { source: "  " });
    const call = fake.calls.find((c) => c.method === "program.take");
    assert.deepEqual(call?.params, { source: null });
  });

  it("start_output adds the destination, because that is what starting one means", async () => {
    const fake = await core({ onSubscribe: serveSession });
    fake.answer("output.add", { id: "youtube", uri_host: "a.rtmp.youtube.com", state: "connecting", reconnects: 0, queue_secs: 0 });
    const link = await linked(fake);

    await ACTIONS.start_output.run(link, { uri: "rtmp://a.rtmp.youtube.com/live2/key", id: "youtube" });
    const call = fake.calls.find((c) => c.method === "output.add");
    assert.deepEqual(call?.params, { uri: "rtmp://a.rtmp.youtube.com/live2/key", id: "youtube" });
  });

  it("add_source leaves out the optional fields the operator did not fill in", async () => {
    const fake = await core({ onSubscribe: serveSession });
    fake.answer("source.add", { id: "cam3", state: "connecting", has_video: true, has_audio: true });
    const link = await linked(fake);

    await ACTIONS.add_source.run(link, { uri: "rtmp://localhost/live/a", id: "", name: "" });
    const call = fake.calls.find((c) => c.method === "source.add");
    assert.deepEqual(call?.params, { uri: "rtmp://localhost/live/a" });
  });

  it("the fader and the mute are separate calls, as the protocol has them", async () => {
    const fake = await core({ onSubscribe: serveSession });
    fake.answer("source.audio.set", { gain: 0.5, muted: false });
    const link = await linked(fake);

    await ACTIONS.set_gain.run(link, { id: "cam1", gain: 0.5 });
    await ACTIONS.set_mute.run(link, { id: "cam1", muted: true });
    const calls = fake.calls.filter((c) => c.method === "source.audio.set");
    assert.deepEqual(calls[0].params, { id: "cam1", gain: 0.5 });
    assert.deepEqual(calls[1].params, { id: "cam1", muted: true });
  });

  it("a refused call rejects rather than being swallowed", async () => {
    const fake = await core({ onSubscribe: serveSession });
    fake.answer("program.take", { error: { code: -32004, message: "there is no source 'cam9'" } });
    const link = await linked(fake);

    await assert.rejects(() => ACTIONS.take.run(link, { source: "cam9" }), /cam9/);
  });

  it("every action has a name and a run", () => {
    for (const [id, action] of Object.entries(ACTIONS)) {
      assert.ok(action.name.length > 0, `${id} has a name`);
      assert.equal(typeof action.run, "function", `${id} has a run`);
      assert.ok(Array.isArray(action.options), `${id} has options`);
    }
  });
});

describe("feedbacks", () => {
  const view: View = {
    ...emptyView(),
    connected: true,
    program: "cam1",
    tally: { cam1: "program", cam2: "preview", cam3: "off" },
    outputs: { youtube: "live", backup: "reconnecting" },
  };

  it("tally is on for the source on air and off for the others", () => {
    assert.equal(FEEDBACKS.tally.check(view, { source: "cam1" }), true);
    assert.equal(FEEDBACKS.tally.check(view, { source: "cam2" }), false);
    assert.equal(FEEDBACKS.tally.check(view, { source: "cam3" }), false);
    assert.equal(FEEDBACKS.tally.check(view, { source: "nobody" }), false);
  });

  it("preview is its own feedback, so a button can be red or green", () => {
    assert.equal(FEEDBACKS.tally_preview.check(view, { source: "cam2" }), true);
    assert.equal(FEEDBACKS.tally_preview.check(view, { source: "cam1" }), false);
  });

  it("tally is red and preview is green by default", () => {
    assert.equal(FEEDBACKS.tally.defaultStyle.bgcolor, RGB.red);
    assert.equal(FEEDBACKS.tally_preview.defaultStyle.bgcolor, RGB.green);
  });

  it("an output feedback matches the state the operator chose", () => {
    assert.equal(FEEDBACKS.output_state.check(view, { id: "youtube", state: "live" }), true);
    assert.equal(FEEDBACKS.output_state.check(view, { id: "youtube", state: "failed" }), false);
    assert.equal(FEEDBACKS.output_state.check(view, { id: "backup", state: "reconnecting" }), true);
    assert.equal(FEEDBACKS.output_state.check(view, { id: "gone", state: "live" }), false);
  });

  it("the connection lamp follows the connection", () => {
    assert.equal(FEEDBACKS.connected.check(view, {}), true);
    assert.equal(FEEDBACKS.connected.check(emptyView(), {}), false);
  });
});

describe("variables", () => {
  it("carry the programme, its name and the uptime", () => {
    const view: View = {
      ...emptyView(),
      connected: true,
      program: "cam1",
      sources: [
        { id: "cam1", name: "Stage wide", state: "live" },
        { id: "cam2", name: "Lectern", state: "connecting" },
      ],
      outputs: { youtube: "live" },
      uptimeSecs: 3725,
      runningTimeMs: 65_000,
    };
    const values = variableValues(view);
    assert.equal(values.program, "cam1");
    assert.equal(values.program_name, "Stage wide");
    assert.equal(values.uptime, "1:02:05");
    assert.equal(values.running_time, "0:01:05");
    assert.equal(values.source_count, 2);
    assert.equal(values.live_source_count, 1);
    assert.equal(values.output_count, 1);
    assert.equal(values.connected, "yes");
  });

  it("read the slate as an empty programme rather than the word null", () => {
    const values = variableValues(emptyView());
    assert.equal(values.program, "");
    assert.equal(values.program_name, "");
    assert.equal(values.connected, "no");
  });

  it("every declared variable is given a value", () => {
    const values = variableValues(emptyView());
    for (const variable of VARIABLES) {
      assert.ok(variable.variableId in values, `${variable.variableId} has a value`);
    }
  });

  it("the clock reads the way an operator expects", () => {
    assert.equal(clock(0), "0:00:00");
    assert.equal(clock(59), "0:00:59");
    assert.equal(clock(3600), "1:00:00");
    assert.equal(clock(-5), "0:00:00");
  });
});

describe("presets", () => {
  it("ship four take buttons, a slate, a revert and a lamp", () => {
    const made = presets();
    for (let n = 1; n <= 4; n += 1) {
      assert.ok(`take_cam${n}` in made, `take_cam${n}`);
    }
    assert.ok("slate" in made);
    assert.ok("revert" in made);
    assert.ok("connected" in made);
  });

  it("every preset names an action and a feedback that exist", () => {
    for (const [id, preset] of Object.entries(presets())) {
      const button = preset as {
        steps: Array<{ down: Array<{ actionId: string }> }>;
        feedbacks: Array<{ feedbackId: string }>;
      };
      for (const step of button.steps) {
        for (const action of step.down) {
          assert.ok(action.actionId in ACTIONS, `${id} uses action ${action.actionId}`);
        }
      }
      for (const feedback of button.feedbacks) {
        assert.ok(feedback.feedbackId in FEEDBACKS, `${id} uses feedback ${feedback.feedbackId}`);
      }
    }
  });
});

// ---------------------------------------------------------------------------
// The acceptance criterion from the roadmap, measured
// ---------------------------------------------------------------------------

describe("a Companion button's feedback turns red within one frame of program.took", () => {
  // What "within one frame" has to mean in a test that cannot be timed.
  //
  // The old assertion read a stopwatch and allowed 250 ms, which is a number
  // about the machine the suite happens to be running on rather than about the
  // module. A loaded box misses it and a fast one hides a regression that
  // added a 200 ms delay.
  //
  // The property worth holding is that no clock stands between the event
  // arriving and the lamp changing: the flip happens on the socket callback
  // that delivered the flush, not on a later tick. So the test freezes time,
  // pushes the events, and requires the flip while the clock is stopped. A
  // module that batched its updates behind a timer would hang here, and
  // hanging is exactly the right answer: it never turns red at all.
  it("flips with the clock stopped, so no timer stands between the event and the lamp", async () => {
    const fake = await core({ onSubscribe: serveSession });
    fake.answer("program.take", { program: "cam2", previous: "cam1", running_time_ms: 1600 });

    // What Companion does on a change: recompute the feedback for each button.
    // Counting the flushes tells us the lamp moved on the flush that carried
    // the tally, and not two or three batches later.
    let flushesAfterPress = 0;
    let pressed = false;
    let red = false;
    let flipped: () => void = () => {};
    const turnedRed = new Promise<void>((resolve) => {
      flipped = resolve;
    });

    const link = await linked(fake, (view) => {
      if (!pressed) return;
      flushesAfterPress += 1;
      if (!red && FEEDBACKS.tally.check(view, { source: "cam2" })) {
        red = true;
        flipped();
      }
    });

    assert.equal(FEEDBACKS.tally.check(link.view, { source: "cam2" }), false, "cam2 starts off");

    // The button is pressed. The take goes out and the core answers it; the
    // round trip is real work and is left on the real clock.
    pressed = true;
    await ACTIONS.take.run(link, { source: "cam2" });

    // From here to the lamp, time does not move.
    mock.timers.enable({ apis: ["setTimeout", "setInterval", "setImmediate"] });
    try {
      fake.notify("event/program.took", { source: "cam2", transition: "cut", at_running_time_ms: 1600 });
      fake.notify("event/tally", { sources: { cam1: "off", cam2: "program" } });
      fake.notify("event/flush", { seq: 45 });
      await turnedRed;
    } finally {
      mock.timers.reset();
    }

    assert.equal(FEEDBACKS.tally.check(link.view, { source: "cam2" }), true, "cam2 is red");
    assert.equal(FEEDBACKS.tally.check(link.view, { source: "cam1" }), false, "cam1 went dark");
    assert.equal(flushesAfterPress, 1, "the lamp moved on the flush that carried the tally");
  });

  it("follows a take somebody else made, not just its own button", async () => {
    const fake = await core({ onSubscribe: serveSession });

    // Two panels on one mixer. Each waits for the other's lamp as well as its
    // own, because the order two sockets are read in is the operating system's
    // business and asserting on one of them alone is a coin toss.
    const saw = { first: false, watcher: false };
    let both: () => void = () => {};
    const done = new Promise<void>((resolve) => {
      both = resolve;
    });
    const settle = (): void => {
      if (saw.first && saw.watcher) both();
    };

    const link = await linked(fake, (view) => {
      if (view.tally.cam2 === "program") {
        saw.first = true;
        settle();
      }
    });
    assert.equal(FEEDBACKS.tally.check(link.view, { source: "cam2" }), false);

    // The second panel is connected and subscribed before the take goes out.
    // Waiting for its own first flush is what puts its snapshot ahead of the
    // events below; awaiting `open()` alone leaves the two in whichever order
    // the socket happened to deliver them.
    const watcher = await linked(fake, (view) => {
      if (view.tally.cam2 === "program") {
        saw.watcher = true;
        settle();
      }
    });
    assert.equal(FEEDBACKS.tally.check(watcher.view, { source: "cam2" }), false, "cam2 starts off here too");

    // Nobody pressed a button here: the web UI took cam2. This one is a
    // broadcast, because that is the point: every panel sees it.
    fake.notify("event/program.took", { source: "cam2", transition: "cut" });
    fake.notify("event/tally", { sources: { cam1: "off", cam2: "program" } });
    fake.notify("event/flush", { seq: 46 });

    await done;
    assert.equal(FEEDBACKS.tally.check(watcher.view, { source: "cam2" }), true);
    assert.equal(FEEDBACKS.tally.check(link.view, { source: "cam2" }), true, "the first panel saw it as well");
  });
});
