// The Stream Deck plugin against a fake core.
//
// `test/fake-core.ts` is the harness from `clients/typescript/test`, copied so
// this package can be lifted out of the repository whole. It is a real HTTP
// server on a real port upgrading a real socket.
//
// Nothing here imports `@elgato/streamdeck`. `src/plugin.ts` is the only file
// that does, and it holds no behaviour: it turns the SDK's callbacks into the
// functions in `keys.ts` and the calls in `link.ts`, which is what these tests
// drive.

import assert from "node:assert/strict";
import { after, describe, it } from "node:test";

import {
  COLOURS,
  outputLook,
  pressOutput,
  pressSlate,
  pressTake,
  slateLook,
  takeLook,
  tile,
} from "../src/keys.ts";
import { Link, emptyView, type View } from "../src/link.ts";
import { fakeCore, snapshot, type FakeCore } from "./fake-core.ts";

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

function serveSession(fake: FakeCore): void {
  fake.notify("event/snapshot", snapshot());
  fake.notify("event/tally", { sources: { cam1: "program", cam2: "off" } });
  fake.notify("event/flush", { seq: 44 });
}

async function linked(fake: FakeCore, onChange?: (view: View) => void): Promise<Link> {
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

/** A view with two sources, cam1 on air. */
function view(overrides: Partial<View> = {}): View {
  return {
    ...emptyView(),
    connected: true,
    program: "cam1",
    tally: { cam1: "program", cam2: "preview", cam3: "off" },
    sources: [
      { id: "cam1", name: "Stage", state: "live" },
      { id: "cam2", name: "Lectern", state: "live" },
      { id: "cam3", name: "Wide", state: "live" },
    ],
    outputs: { youtube: "live", backup: "reconnecting", broken: "failed" },
    ...overrides,
  };
}

describe("a take key's colour", () => {
  it("is red on air, green on preview and dark otherwise", () => {
    assert.equal(takeLook(view(), { source: "cam1" }).background, COLOURS.program);
    assert.equal(takeLook(view(), { source: "cam2" }).background, COLOURS.preview);
    assert.equal(takeLook(view(), { source: "cam3" }).background, COLOURS.idle);
  });

  it("says so when the key names a source the mixer has never heard of", () => {
    const look = takeLook(view(), { source: "cam9" });
    assert.equal(look.state, "unknown");
    assert.ok(look.title.includes("?"), look.title);
  });

  it("asks for a source when the key has none", () => {
    assert.equal(takeLook(view(), {}).state, "unset");
  });

  it("goes grey when the mixer is not there, whatever it was showing", () => {
    const look = takeLook(view({ connected: false }), { source: "cam1" });
    assert.equal(look.background, COLOURS.offline);
    assert.equal(look.state, "offline");
  });

  it("uses the operator's own title when they set one", () => {
    assert.equal(takeLook(view(), { source: "cam1", title: "STAGE" }).title, "STAGE");
    assert.equal(takeLook(view(), { source: "cam1" }).title, "cam1");
    assert.equal(takeLook(view(), { source: "cam1", title: "   " }).title, "cam1");
  });
});

describe("an output key's colour", () => {
  it("follows the destination's state", () => {
    assert.equal(outputLook(view(), { id: "youtube" }).background, COLOURS.live);
    assert.equal(outputLook(view(), { id: "backup" }).background, COLOURS.connecting);
    assert.equal(outputLook(view(), { id: "broken" }).background, COLOURS.failed);
  });

  it("reads a destination that is not there as stopped, which is the off position", () => {
    const look = outputLook(view(), { id: "vimeo" });
    assert.equal(look.state, "stopped");
    assert.equal(look.background, COLOURS.idle);
  });
});

describe("the slate key", () => {
  it("is red while the programme is black", () => {
    assert.equal(slateLook(view({ program: null })).background, COLOURS.program);
    assert.equal(slateLook(view()).background, COLOURS.idle);
  });
});

describe("a tile", () => {
  it("is an SVG data URI carrying the colour and the title", () => {
    const url = tile({ background: "#cc0000", foreground: "#ffffff", title: "CAM 1", state: "program" });
    assert.ok(url.startsWith("data:image/svg+xml;base64,"), url.slice(0, 40));
    const svg = Buffer.from(url.split(",")[1], "base64").toString("utf8");
    assert.ok(svg.includes("#cc0000"), svg);
    assert.ok(svg.includes("CAM 1"), svg);
    assert.ok(svg.includes('width="144"'), svg);
  });

  it("escapes a name that would otherwise break the SVG", () => {
    const url = tile({ background: "#000000", foreground: "#ffffff", title: "A & B", state: "off" });
    const svg = Buffer.from(url.split(",")[1], "base64").toString("utf8");
    assert.ok(svg.includes("A &amp; B"), svg);
  });

  it("stacks two lines without running off the key", () => {
    const url = tile({ background: "#000000", foreground: "#ffffff", title: "set a\nsource", state: "unset" });
    const svg = Buffer.from(url.split(",")[1], "base64").toString("utf8");
    assert.equal((svg.match(/<text/g) ?? []).length, 2, svg);
  });
});

describe("pressing a key", () => {
  it("takes the source, over the same call the web UI makes", async () => {
    const fake = await core({ onSubscribe: serveSession });
    fake.answer("program.take", { program: "cam2", running_time_ms: 1600 });
    const link = await linked(fake);

    const said = await pressTake(link, link.view, { source: "cam2" });
    assert.equal(said, "took cam2");
    assert.deepEqual(fake.calls.find((c) => c.method === "program.take")?.params, { source: "cam2" });
  });

  it("does nothing when the source is already on air", async () => {
    const fake = await core({ onSubscribe: serveSession });
    const link = await linked(fake);

    const said = await pressTake(link, link.view, { source: "cam1" });
    assert.match(said, /already on air/);
    assert.equal(fake.calls.filter((c) => c.method === "program.take").length, 0);
  });

  it("an output key starts a destination that is not running", async () => {
    const fake = await core({ onSubscribe: serveSession });
    fake.answer("output.add", { id: "vimeo", uri_host: "rtmp.vimeo.com", state: "connecting", reconnects: 0, queue_secs: 0 });
    const link = await linked(fake);

    const said = await pressOutput(link, link.view, { id: "vimeo", uri: "rtmp://rtmp.vimeo.com/live/k" });
    assert.equal(said, "started vimeo");
    assert.deepEqual(fake.calls.find((c) => c.method === "output.add")?.params, {
      uri: "rtmp://rtmp.vimeo.com/live/k",
      id: "vimeo",
    });
  });

  it("an output key stops one that is running", async () => {
    const fake = await core({ onSubscribe: serveSession });
    fake.answer("output.remove", {});
    const link = await linked(fake);

    const said = await pressOutput(link, link.view, { id: "twitch", uri: "rtmp://a/b" });
    assert.equal(said, "stopped twitch");
    assert.deepEqual(fake.calls.find((c) => c.method === "output.remove")?.params, { id: "twitch" });
  });

  it("an output key with no address says so rather than failing silently", async () => {
    const fake = await core({ onSubscribe: serveSession });
    const link = await linked(fake);
    const said = await pressOutput(link, link.view, { id: "vimeo" });
    assert.match(said, /no address/);
  });

  it("the slate key cuts to black", async () => {
    const fake = await core({ onSubscribe: serveSession });
    fake.answer("program.take", { running_time_ms: 1700 });
    const link = await linked(fake);

    assert.equal(await pressSlate(link), "cut to the slate");
    assert.deepEqual(fake.calls.find((c) => c.method === "program.take")?.params, { source: null });
  });
});

describe("a key turns red within one frame of program.took", () => {
  it("flips inside a render tick, with the number printed", async () => {
    const fake = await core({ onSubscribe: serveSession });
    fake.answer("program.take", { program: "cam2", previous: "cam1", running_time_ms: 1600 });

    let redAt = 0;
    let sentAt = 0;
    let flipped: (() => void) | null = null;
    const turnedRed = new Promise<void>((resolve) => {
      flipped = resolve;
    });

    const link = await linked(fake, (current) => {
      if (sentAt && !redAt && takeLook(current, { source: "cam2" }).state === "program") {
        redAt = performance.now();
        flipped?.();
      }
    });

    assert.equal(takeLook(link.view, { source: "cam2" }).background, COLOURS.idle);

    sentAt = performance.now();
    await pressTake(link, link.view, { source: "cam2" });
    fake.notify("event/program.took", { source: "cam2", transition: "cut" });
    fake.notify("event/tally", { sources: { cam1: "off", cam2: "program" } });
    fake.notify("event/flush", { seq: 45 });

    await turnedRed;
    const elapsed = redAt - sentAt;
    assert.equal(takeLook(link.view, { source: "cam2" }).background, COLOURS.program);
    assert.equal(takeLook(link.view, { source: "cam1" }).background, COLOURS.idle);
    console.log(`    key turned red ${elapsed.toFixed(1)} ms after it was pressed`);
    assert.ok(elapsed < 250, `turned red in ${elapsed.toFixed(1)} ms`);
  });
});

describe("the plugin manifest", () => {
  it("declares the three actions the plugin registers", async () => {
    const path = new URL("../com.godwinmix.streamdeck.sdPlugin/manifest.json", import.meta.url);
    const manifest = JSON.parse(await (await import("node:fs/promises")).readFile(path, "utf8"));
    const uuids = (manifest.Actions as Array<{ UUID: string }>).map((a) => a.UUID);
    assert.deepEqual(uuids.sort(), [
      "com.godwinmix.streamdeck.output",
      "com.godwinmix.streamdeck.slate",
      "com.godwinmix.streamdeck.take",
    ]);
    assert.equal(manifest.UUID, "com.godwinmix.streamdeck");
    assert.equal(manifest.CodePath, "bin/plugin.js");
    assert.equal(manifest.SDKVersion, 2);
    for (const uuid of uuids) {
      assert.ok(uuid.startsWith(`${manifest.UUID}.`), `${uuid} is inside the plugin's namespace`);
    }
  });
});
