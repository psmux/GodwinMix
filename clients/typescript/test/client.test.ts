// The client against a fake core.

import assert from "node:assert/strict";
import { after, describe, it } from "node:test";

import { connect, RpcError, CODES, type Client, type ConnectOptions } from "../src/index.ts";
import { fakeCore, snapshot, type FakeCore } from "./fake-core.ts";

// Every client and every core made by a test, closed at the end. A client that
// is left open keeps reconnecting to a port nobody is listening on, and the
// test runner then never exits.
const open: Array<{ close(): void }> = [];
const cores: FakeCore[] = [];

after(async () => {
  for (const client of open) client.close();
  for (const core of cores) await core.stop();
});

async function core(opts: Parameters<typeof fakeCore>[0] = {}): Promise<FakeCore> {
  const made = await fakeCore(opts);
  cores.push(made);
  made.answer("core.subscribe", { seq: 41, events: ["*"], ignored_ext: [] });
  return made;
}

async function client(base: string, opts: ConnectOptions = {}): Promise<Client> {
  const made = await connect({ base, ...opts });
  open.push(made);
  return made;
}

/** The subscribe answer, then the snapshot, a delta, a layout and the flush. */
function serveSession(core: FakeCore): void {
  core.notify("event/snapshot", snapshot());
  core.notify("event/source.state", { source: "cam2", state: "live" });
  core.notify("event/multiview.layout", {
    id: 7,
    width: 640,
    height: 180,
    cells: [
      { index: 0, source: "cam1", x: 0, y: 0, w: 320, h: 180 },
      { index: 1, source: "cam2", x: 320, y: 0, w: 320, h: 180 },
    ],
  });
  core.frame(3, 7, 1500, new Uint8Array([0xff, 0xd8, 0xff, 0xd9]));
  core.notify("event/flush", { seq: 44 });
}

describe("connect, subscribe, render at flush", () => {
  it("applies the snapshot and the deltas, and fires once at flush", async () => {
    const fake = await core({ onSubscribe: serveSession });
    const gmx = await client(fake.url, { token: "t" });
    let renders = 0;
    gmx.onFlush(() => {
      renders += 1;
    });
    const state = await gmx.settled();

    assert.equal(state.program, "cam1");
    assert.equal(state.sources.length, 2);
    // The delta after the snapshot has been folded in.
    assert.equal(gmx.store.source("cam2")?.state, "live");
    assert.equal(state.seq, 44);
    assert.equal(state.layout?.id, 7);
    // One repaint for the whole batch, plus the first call onFlush makes.
    assert.equal(renders, 2);
  });

  it("puts the token in the query, because a WebSocket cannot set a header", async () => {
    const fake = await core();
    const gmx = await client(fake.url, { token: "sekrit" });
    assert.ok(gmx.socket.url.includes("token=sekrit"));
    assert.ok(gmx.socket.url.startsWith("ws://"));
  });

  it("asks for nothing expensive until a surface says it wants it", async () => {
    const fake = await core();
    const gmx = await client(fake.url);
    await settle();
    assert.deepEqual(fake.calls[0]?.params.ext, {});

    const want = gmx.want("multiview", { fps: 8, width: 960 });
    const second = gmx.want("multiview", { fps: 4, width: 1280 });
    await settle(80);
    // The widest width and the highest rate anyone asked for, in one subscribe.
    assert.deepEqual(fake.calls.at(-1)?.params.ext, { multiview: { fps: 8, width: 1280 } });

    want.release();
    second.release();
    await settle(80);
    assert.deepEqual(fake.calls.at(-1)?.params.ext, {});
  });
});

describe("calls", () => {
  it("answers a typed call with a typed result", async () => {
    const fake = await core();
    fake.answer("program.take", (params) => ({ program: params.source, running_time_ms: 1600 }));
    const gmx = await client(fake.url);
    const program = await gmx.programTake({ source: "cam2" });
    assert.equal(program.program, "cam2");
    assert.equal(program.running_time_ms, 1600);
  });

  it("keeps the shape of a refusal, and reads the next step out of it", async () => {
    const fake = await core();
    fake.answer("program.take", {
      error: {
        code: CODES.NOT_FOUND,
        message: "no source cam9. This core has cam1 and cam2.",
        data: { retryable: false },
      },
    });
    const gmx = await client(fake.url);
    await assert.rejects(
      () => gmx.programTake({ source: "cam9" }),
      (e: unknown) => {
        assert.ok(e instanceof RpcError);
        assert.equal(e.code, CODES.NOT_FOUND);
        assert.equal(e.retryable, false);
        assert.equal(e.title, "Not found");
        assert.equal(e.nextStep, "This core has cam1 and cam2.");
        return true;
      },
    );
  });

  it("fails an outstanding call when the core goes away, rather than hanging", async () => {
    const fake = await core();
    // The fake takes this one and never answers, which is what a core being
    // restarted mid call looks like.
    fake.silence("program.take");
    const gmx = await client(fake.url);
    const pending = gmx.programTake({ source: "cam1" });
    setTimeout(() => fake.drop(), 20);
    await assert.rejects(pending, (e: unknown) => e instanceof RpcError && e.retryable);
  });
});

describe("binary frames", () => {
  it("reads the 16 byte header and hands over the JPEG", async () => {
    const fake = await core({ onSubscribe: serveSession });
    const gmx = await client(fake.url);
    const frame = await new Promise<{ seq: number; layout: number; runningTimeMs: number; jpeg: Uint8Array }>(
      (resolve, reject) => {
        const timer = setTimeout(() => reject(new Error("no frame in two seconds")), 2000);
        gmx.on("frame", (f) => {
          clearTimeout(timer);
          resolve(f);
        });
      },
    );
    assert.equal(frame.seq, 3);
    assert.equal(frame.layout, 7);
    assert.equal(frame.runningTimeMs, 1500);
    assert.deepEqual([...frame.jpeg], [0xff, 0xd8, 0xff, 0xd9]);
  });
});

function settle(ms = 30): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
