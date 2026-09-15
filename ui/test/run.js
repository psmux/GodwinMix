// The test runner: forty lines, no dependencies, no toolchain. Open the page,
// read the console, or read the list. Everything testable without a mixer is
// here, including the legacy adapter against a stubbed server.

import { Selection, overlaps, rectFrom } from "../shell/selection.js";
import { dbToPos, FLOOR } from "../shell/meter.js";
import { posToGain, gainToPos, gainLabel, UNITY } from "../shell/fader.js";
import { parseFrame, sheetWidthFor, HEADER_BYTES, SheetPainter } from "../client/frames.js";
import { Store } from "../client/store.js";
import { SchemaForm } from "../client/schema-form.js";
import { Client } from "../client/index.js";
import { RpcError, CODES } from "../client/errors.js";
import { rank } from "../shell/palette.js";
import { chordOf, DEFAULT_MAP } from "../shell/keymap.js";
import { IS_MAC } from "../shell/dom.js";
import { kindOfUri } from "../client/kinds.js";
import { tagFor } from "../shell/registry.js";
import * as layout from "../shell/layout.js";
import { ART } from "../panels/welcome/tiles.js";
import { WelcomePanel } from "../panels/welcome/panel.js";
import { connect } from "../client/index.js";
import { shell } from "../shell/shell.js";
import { SceneMirror } from "../kits/protocol/mirror.js";
import { Prediction, mergeProps } from "../kits/protocol/predict.js";
import { gizmosFor, handlesFor, applyDrag } from "../kits/canvas/gizmos.js";
import { snapTargets, snapDelta } from "../kits/canvas/snap.js";
import { safeAreas } from "../kits/canvas/safe.js";
import { describeForm, valuesOf, readForm, missing } from "../kits/schema/describe.js";
import { layoutFor, applyRules, controlsOf } from "../kits/schema/ui-schema.js";

let passed = 0;
let failed = 0;
const out = document.getElementById("out");

function test(name, fn) {
  try {
    fn();
    passed += 1;
    line("ok", name);
  } catch (e) {
    failed += 1;
    line("fail", `${name}: ${e.message}`);
    console.error(name, e);
  }
}

function line(kind, text) {
  const row = document.createElement("div");
  row.className = "row sm";
  row.innerHTML = `<span class="dot ${kind === "ok" ? "live" : "failed"}"></span>`;
  row.appendChild(document.createTextNode(`${kind === "ok" ? "ok  " : "FAIL"}  ${text}`));
  if (out) out.appendChild(row);
  console[kind === "ok" ? "log" : "error"](`${kind === "ok" ? "PASS" : "FAIL"}  ${text}`);
}

function eq(a, b, what) {
  const x = JSON.stringify(a);
  const y = JSON.stringify(b);
  if (x !== y) throw new Error(`${what || "value"}: expected ${y}, got ${x}`);
}

function ok(v, what) {
  if (!v) throw new Error(what || "expected something truthy");
}

function near(a, b, tol, what) {
  if (Math.abs(a - b) > tol) throw new Error(`${what || "value"}: ${a} is not within ${tol} of ${b}`);
}

// ---------------------------------------------------------------- selection

const ORDER = ["a", "b", "c", "d", "e"];

test("a plain click replaces the selection", () => {
  const s = new Selection();
  s.click("b", ORDER, {});
  s.click("d", ORDER, {});
  eq(s.list(ORDER), ["d"]);
  eq(s.anchor, "d");
});

test("Ctrl click toggles one and moves the anchor", () => {
  const s = new Selection();
  s.click("b", ORDER, {});
  s.click("d", ORDER, { toggle: true });
  eq(s.list(ORDER), ["b", "d"]);
  eq(s.anchor, "d");
  s.click("b", ORDER, { toggle: true });
  eq(s.list(ORDER), ["d"]);
});

test("Shift click extends from the anchor in display order, both ways", () => {
  const s = new Selection();
  s.click("d", ORDER, {});
  s.click("b", ORDER, { extend: true });
  eq(s.list(ORDER), ["b", "c", "d"]);
  // The anchor stays put, so a second Shift click regrows rather than restarts.
  eq(s.anchor, "d");
  s.click("e", ORDER, { extend: true });
  eq(s.list(ORDER), ["d", "e"]);
});

test("pressing inside a multiple selection keeps it until release", () => {
  const s = new Selection();
  s.set(["a", "b", "c"], "a");
  const deferred = s.press("b", ORDER, {});
  eq(s.list(ORDER), ["a", "b", "c"], "the selection survives the press");
  ok(deferred, "release has something to do");
  s.release(deferred);
  eq(s.list(ORDER), ["b"], "a click with no drag collapses to the one pressed");
});

test("pressing outside the selection selects that one immediately", () => {
  const s = new Selection();
  s.set(["a"], "a");
  const deferred = s.press("c", ORDER, {});
  eq(deferred, null);
  eq(s.list(ORDER), ["c"]);
});

test("a sweep replaces, and Ctrl held adds to what was there", () => {
  const s = new Selection();
  s.marquee(["a", "b"], false, []);
  eq(s.list(ORDER), ["a", "b"]);
  s.marquee(["d"], true, ["a", "b"]);
  eq(s.list(ORDER), ["a", "b", "d"]);
});

test("Ctrl+A takes everything and Escape clears it", () => {
  const s = new Selection();
  s.selectAll(ORDER);
  eq(s.list(ORDER), ORDER);
  s.clear();
  eq(s.size, 0);
});

test("rectangles overlap by touching, and a sweep normalises its corners", () => {
  ok(overlaps({ x: 0, y: 0, w: 10, h: 10 }, { x: 9, y: 9, w: 5, h: 5 }));
  ok(!overlaps({ x: 0, y: 0, w: 10, h: 10 }, { x: 11, y: 0, w: 5, h: 5 }));
  eq(rectFrom(30, 40, 10, 20), { x: 10, y: 20, w: 20, h: 20 });
});

// ---------------------------------------------------------------- meters

test("the meter scale gives the working range most of the travel", () => {
  eq(dbToPos(0), 1);
  eq(dbToPos(-70), 0);
  eq(dbToPos(FLOOR), 0);
  near(dbToPos(-20), 0.35, 0.001, "-20 dBFS");
  ok(dbToPos(-20) > 0.3 && dbToPos(-20) < 0.4, "-20 dBFS is about a third up");
  ok(dbToPos(-6) > dbToPos(-12), "louder is higher");
});

// ---------------------------------------------------------------- faders

test("the fader is a straight line in dB with unity at three quarters", () => {
  near(posToGain(UNITY), 1, 0.0001, "unity");
  near(gainToPos(1), UNITY, 0.0001, "unity back again");
  eq(posToGain(0), 0);
  near(posToGain(1), 10, 0.01, "the top of the fader is the mixer's ceiling");
});

test("a gain round trips through the curve", () => {
  for (const gain of [0.05, 0.25, 0.5, 1, 2, 4, 9.9]) {
    near(posToGain(gainToPos(gain)), gain, gain * 0.02, `gain ${gain}`);
  }
});

test("the gain label reads the way an operator expects", () => {
  eq(gainLabel(0), "off");
  eq(gainLabel(1), "0.0");
  eq(gainLabel(2)[0], "+");
  ok(gainLabel(0.5).startsWith("-"));
});

// ---------------------------------------------------------------- frames

test("a frame header is sixteen little endian bytes then JPEG", () => {
  const buf = new ArrayBuffer(HEADER_BYTES + 3);
  const view = new DataView(buf);
  view.setUint32(0, 4821, true);
  view.setUint32(4, 7, true);
  view.setBigUint64(8, 1234567890n, true);
  new Uint8Array(buf).set([0xff, 0xd8, 0xff], HEADER_BYTES);
  const frame = parseFrame(buf);
  eq(frame.seq, 4821);
  eq(frame.layout, 7);
  ok(frame.runningTimeMs === 1234567890n);
  eq([...frame.jpeg], [0xff, 0xd8, 0xff]);
});

test("a frame shorter than its header is refused rather than misread", () => {
  eq(parseFrame(new ArrayBuffer(8)), null);
});

test("the sheet width asked for is the tile's real pixels, never more", () => {
  // Four tiles across at 168 CSS pixels on a plain screen: 168*4 = 672, up to 672.
  eq(sheetWidthFor(168, 4, 1), 672);
  // The same on a retina screen is twice that.
  eq(sheetWidthFor(168, 4, 2), 1344);
  // Never below the floor or above the ceiling the protocol allows.
  eq(sheetWidthFor(40, 1, 1), 320);
  eq(sheetWidthFor(1000, 4, 2), 1920);
});

test("reattaching a visible canvas restores the latest picture immediately", () => {
  const painter = new SheetPainter();
  const bitmap = document.createElement("canvas");
  bitmap.width = 2; bitmap.height = 2;
  const brush = bitmap.getContext("2d");
  brush.fillStyle = "#ff0000"; brush.fillRect(0, 0, 2, 2);
  painter.bitmap = bitmap;
  painter.setLayout({ cells: [{ index: 0, x: 0, y: 0, w: 2, h: 2 }] });
  const target = document.createElement("canvas");
  target.width = 2; target.height = 2;
  const detach = painter.attach(target, 0);
  eq([...target.getContext("2d").getImageData(0, 0, 1, 1).data], [255, 0, 0, 255]);
  detach(); painter.destroy();
});

test("a snapshot restores the frame layout without a separate layout event", () => {
  const client = new Client({ name: "test" }, new Store());
  const multiview = { width: 640, height: 360, cells: [{ index: 0, x: 0, y: 0, w: 640, h: 360 }] };
  client.handleEvent("snapshot", { state: { multiview }, seq: 1 });
  eq(client.sheet.layout, multiview);
});

// ---------------------------------------------------------------- store

test("the store renders at flush and not before", () => {
  const store = new Store();
  let renders = 0;
  store.subscribe(() => {
    renders += 1;
  });
  store.snapshot({ program: "cam1", sources: [{ id: "cam1", name: "Cam 1" }] }, 10);
  store.patchSource("cam1", { state: "live" });
  store.patch({ uptime_secs: 5 });
  eq(renders, 0, "nothing painted yet");
  store.flush();
  eq(renders, 1, "one paint for the whole batch");
  store.flush();
  eq(renders, 1, "a flush with nothing dirty paints nothing");
});

test("the store answers tally from programme and preview", () => {
  const store = new Store();
  store.snapshot({ program: "cam1", preview: "cam2", sources: [] }, 1);
  eq(store.tallyOf("cam1"), "program");
  eq(store.tallyOf("cam2"), "preview");
  eq(store.tallyOf("cam3"), "off");
});

test("meters do not dirty the store, because they arrive ten times a second", () => {
  const store = new Store();
  store.flush(true);
  let renders = 0;
  store.subscribe(() => {
    renders += 1;
  });
  store.setMeters([-12, -14], { cam1: [-20] });
  store.flush();
  eq(renders, 0);
  eq(store.state.meters.sources.cam1, [-20]);
});

// ---------------------------------------------------------------- ext union

test("the widest width and the highest rate win, and a release gives it back", () => {
  const client = new Client({ name: "test", subscribe: () => Promise.resolve({}) }, new Store());
  const small = client.want("multiview", { fps: 4, width: 640 });
  const big = client.want("multiview", { fps: 8, width: 1280 });
  eq(client.extSpec(), { multiview: { fps: 8, width: 1280 } });
  big.release();
  eq(client.extSpec(), { multiview: { fps: 4, width: 640 } });
  small.release();
  eq(client.extSpec(), {}, "nothing wanted means nothing subscribed");
});

test("a boolean ext stays a boolean", () => {
  const client = new Client({ name: "test", subscribe: () => Promise.resolve({}) }, new Store());
  const want = client.want("meters");
  eq(client.extSpec(), { meters: true });
  want.release();
  eq(client.extSpec(), {});
});

// ---------------------------------------------------------------- errors

test("an error names its next step and says whether a retry is honest", () => {
  const err = new RpcError(CODES.SAFETY, "held by min_hold_ms. Wait 3.2 seconds and take again.", { retry_after_ms: 3200 });
  eq(err.retryable, true);
  eq(err.retryAfterMs, 3200);
  eq(err.nextStep, "Wait 3.2 seconds and take again.");
  ok(err.title.length > 0);
});

test("a not found is not retryable", () => {
  eq(new RpcError(CODES.NOT_FOUND, "no such source cam9.", {}).retryable, false);
});

// ---------------------------------------------------------------- schema

test("a schema becomes controls, and reading gives back what was typed", () => {
  const form = new SchemaForm(
    {
      type: "object",
      required: ["uri"],
      properties: {
        uri: { type: "string", title: "Address" },
        fps: { type: "integer", default: 30 },
        loop: { type: "boolean", default: true },
        secret: { type: "string", format: "secret" },
      },
    },
    { uri: "rtmp://x/y", secret: "already set" }
  );
  const values = form.read();
  eq(values.uri, "rtmp://x/y");
  eq(values.fps, 30);
  eq(values.loop, true);
  eq(values.secret, undefined, "an untouched secret is left out rather than blanked");
  eq(form.missing(), []);
});

test("a missing required field is named", () => {
  const form = new SchemaForm({ type: "object", required: ["uri"], properties: { uri: { type: "string" } } }, {});
  eq(form.missing(), ["uri"]);
  eq(form.validate(), false);
});

test("if and then hide the fields that do not apply", () => {
  // `if`/`then` decides what is shown, not what exists: every field is
  // declared in `properties` and the condition makes it appear. A property
  // that lives only inside a `then` block has no control in any of the three
  // readers (browser, TypeScript, Python), which is what `x-gmx-group` and a
  // plugin's own editor are for. `client/kinds.js` writes the page kind this
  // way, and so does every schema that ships.
  const form = new SchemaForm(
    {
      type: "object",
      properties: {
        kind: { type: "string", enum: ["file", "page"], default: "file" },
        superimpose: { type: "string", enum: ["off", "auto"], default: "off" },
      },
      allOf: [
        {
          if: { properties: { kind: { const: "page" } } },
          then: { properties: { superimpose: {} } },
        },
      ],
    },
    {}
  );
  eq(form.read().superimpose, undefined, "hidden while the kind is file");
  const select = form.el.querySelector("select");
  select.value = "page";
  select.dispatchEvent(new Event("change"));
  eq(form.read().superimpose, "off", "shown once the kind is page");
});

test("a unit annotation is printed beside the control", () => {
  const form = new SchemaForm({ type: "object", properties: { queue: { type: "number", "x-gmx-unit": "s" } } }, {});
  ok(form.el.querySelector(".unit"), "the unit is on screen");
  eq(form.el.querySelector(".unit").textContent, "s");
});

// ---------------------------------------------------------------- palette

test("the palette prefers a title that starts with what was typed", () => {
  const cmds = [
    { id: "a", title: "Take the armed tile", group: "Programme", run() {} },
    { id: "b", title: "Add an input", group: "Sources", run() {} },
    { id: "c", title: "Remove the selection", group: "Sources", run() {} },
  ];
  eq(rank(cmds, "take")[0].id, "a");
  eq(rank(cmds, "add")[0].id, "b");
  eq(rank(cmds, "zzz").length, 0);
});

// ---------------------------------------------------------------- keymap

test("a chord is spelled the way the map spells it", () => {
  // The map writes Ctrl and means the platform's own accelerator, so the event
  // this is given has to be the one that platform actually produces: Cmd on a
  // Mac, Ctrl everywhere else. Asserting a ctrlKey event spells "Ctrl+K" fails
  // on macOS, where Ctrl+K is a different chord from the one in the map.
  const accelKey = IS_MAC ? { metaKey: true, ctrlKey: false } : { ctrlKey: true, metaKey: false };
  eq(chordOf(Object.assign({ key: "k", altKey: false, shiftKey: false }, accelKey)), "Ctrl+K");
  eq(chordOf({ key: "F2", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false }), "F2");
  eq(chordOf({ key: "1", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false }), "1");
});

test("the map is commands, never indices into a source list", () => {
  for (const value of Object.values(DEFAULT_MAP)) ok(typeof value === "string" && value.includes("."), `${value} is a command id`);
  for (let n = 1; n <= 9; n += 1) eq(DEFAULT_MAP[String(n)], "tray.take-slot", `the key ${n}`);
  eq(DEFAULT_MAP["0"], "program.black");
});

// ---------------------------------------------------------------- kinds

test("a URL is guessed the way the server guesses it", () => {
  eq(kindOfUri("rtmp://a/b"), "stream");
  eq(kindOfUri("exec:ffmpeg -i x"), "exec");
  eq(kindOfUri("/srv/media/a.mp4"), "file");
  eq(kindOfUri("https://example.org/lyrics"), "page");
  eq(kindOfUri("web+https://example.org"), "page");
});

// ---------------------------------------------------------------- registry

test("a panel id becomes a legible custom element name", () => {
  eq(tagFor("ndi/senders"), "gmx-ndi-senders");
  eq(tagFor("core/header"), "gmx-core-header");
  eq(tagFor("gmx-thing"), "gmx-thing");
});

// ---------------------------------------------------------------- layout

test("a layout keeps every slot and refuses anything it does not know", () => {
  const l = layout.place(layout.defaultLayout(), "sidebar", "ndi/senders");
  ok(l.sidebar.includes("ndi/senders"));
  for (const slot of layout.SLOTS) ok(Array.isArray(l[slot]), `${slot} is a list`);
  const gone = layout.remove(l, "ndi/senders");
  ok(!gone.sidebar.includes("ndi/senders"));
});

// ------------------------------------------------- the legacy adapter

// The adapter is the thing that lets this page work against the mixer people
// are running today, so it is worth a test even though it is temporary. A
// stubbed fetch and a stubbed socket stand in for the server.

async function legacySuite() {
  const { LegacyTransport } = await import("../client/transport-legacy.js");
  const STATUS = {
    program: "cam1",
    sources: [{ id: "cam1", name: "Cam 1", uri: "rtmp://x/1", state: "live" }],
    outputs: [],
    multiview: { enabled: true, cols: 2, rows: 1, cells: [{ index: 0, source: "cam1", x: 0, y: 0, w: 4, h: 3 }] },
  };
  const calls = [];
  const realFetch = window.fetch;
  const realSocket = window.WebSocket;
  let socket = null;
  window.fetch = async (url, init = {}) => {
    const path = new URL(url, location.origin).pathname;
    calls.push([init.method || "GET", path, init.body ? JSON.parse(init.body) : null]);
    if (path === "/api/status") return { ok: true, status: 200, text: async () => JSON.stringify(STATUS) };
    if (path === "/api/sources/cam9/audio") return { ok: false, status: 404, text: async () => '{"error":"no such source cam9"}' };
    return { ok: true, status: 200, text: async () => "" };
  };
  window.WebSocket = class {
    constructor(url) {
      this.url = url;
      socket = this;
      setTimeout(() => this.onopen && this.onopen(), 0);
    }
    close() {}
  };

  const events = [];
  const transport = new LegacyTransport({
    base: location.origin,
    token: "abc",
    hooks: { onOpen() {}, onClose() {}, onEvent: (n, p) => events.push([n, p]), onFrame: () => events.push(["frame"]) },
  });
  transport.open();
  await new Promise((r) => setTimeout(r, 30));

  test("the legacy status poll becomes a snapshot, a layout and a flush", () => {
    const names = events.map((e) => e[0]);
    ok(names.includes("snapshot"), names.join(","));
    ok(names.includes("multiview.layout"));
    ok(names.includes("flush"));
  });
  test("the token rides in the socket query, because a header cannot", () => {
    ok(socket.url.includes("token=abc"), socket.url);
  });

  events.length = 0;
  socket.onmessage({ data: JSON.stringify({ type: "took", source: "cam2", at_running_time_ms: 9 }) });
  test("took becomes program.took and is flushed", () => {
    eq(events.map((e) => e[0]), ["program.took", "flush"]);
    eq(events[0][1].source, "cam2");
  });

  events.length = 0;
  socket.onmessage({ data: JSON.stringify({ type: "source_audio_level", source: "cam1", peak_db: [-12] }) });
  socket.onmessage({ data: JSON.stringify({ type: "audio_level", peak_db: [-6] }) });
  test("both old level events become one meters event each", () => {
    eq(events.filter((e) => e[0] === "meters").map((e) => e[1]), [{ sources: { cam1: [-12] } }, { program: [-6] }]);
  });

  events.length = 0;
  socket.onmessage({ data: new Uint8Array([0xff, 0xd8, 1]).buffer });
  test("a bare JPEG is wrapped so the painter sees one shape", () => {
    eq(events.filter((e) => e[0] === "frame").length, 1);
  });
  await transport.subscribe({ ext: {} });
  events.length = 0;
  socket.onmessage({ data: new Uint8Array([0xff, 0xd8, 1]).buffer });
  test("with nothing wanting pictures the frames are dropped before the decode", () => {
    eq(events.filter((e) => e[0] === "frame").length, 0);
  });

  calls.length = 0;
  await transport.call("source.audio.set", { source: "cam1", media: [null, 0.5] });
  test("only the moved audio channel is sent, and the array stays sparse", () => {
    eq(calls[0][1], "/api/sources/cam1/audio");
    eq(calls[0][2].media, [null, 0.5]);
  });

  let thrown = null;
  try {
    await transport.call("source.set", { source: "cam1", name: "Wide" });
  } catch (e) {
    thrown = e;
  }
  test("source.set answers method not found, so the tray keeps the name locally", () => {
    eq(thrown.code, CODES.NO_METHOD);
    ok(thrown.message.includes("/rpc"), thrown.message);
  });

  thrown = null;
  try {
    await transport.call("source.audio.set", { source: "cam9", gain: 1 });
  } catch (e) {
    thrown = e;
  }
  test("a 404 keeps the server's own sentence rather than inventing one", () => {
    eq(thrown.code, CODES.NOT_FOUND);
    eq(thrown.message, "no such source cam9");
  });

  transport.close();
  window.fetch = realFetch;
  window.WebSocket = realSocket;
}

// ------------------------------------------------------- the welcome panel

test("every welcome tile is an inline SVG under two kilobytes", () => {
  const ids = Object.keys(ART);
  eq(ids, ["church", "classroom", "esports", "empty", "obs"], "the five choices");
  for (const id of ids) {
    ok(ART[id].startsWith("<svg"), `${id} is not an svg`);
    ok(ART[id].length < 2048, `${id} is ${ART[id].length} bytes, over the budget`);
    ok(!/https?:/.test(ART[id]), `${id} fetches something from outside the page`);
  }
});

async function welcomeSuite() {
  const calls = [];
  const client = {
    state: { sources: [] },
    on: () => () => {},
    call: async (method, params) => {
      calls.push([method, params]);
      if (method === "core.info") return { version: "test", ui: null };
      return {
        dry_run: false,
        plan: { steps: ["One.", "Two.", "Three."], plugins: [{ name: "camera", installed: false }] },
        needs_restart: [],
      };
    },
  };
  const panel = new WelcomePanel();
  panel.setClient(client);
  panel.connectedCallback();
  for (let i = 0; i < 100 && !panel.dialog; i += 1) await new Promise((r) => setTimeout(r, 10));

  test("the welcome tiles come up on a core with no sources and no preset", () => {
    ok(panel.dialog, "nothing opened");
    const tiles = panel.dialog.el.querySelectorAll(".welcome-tile");
    eq(tiles.length, 5, "five tiles");
  });

  const first = panel.dialog && panel.dialog.el.querySelector(".welcome-tile");
  if (first) first.click();
  for (let i = 0; i < 100 && !calls.some((c) => c[0] === "preset.apply"); i += 1) {
    await new Promise((r) => setTimeout(r, 10));
  }

  test("picking a tile applies that preset over the protocol", () => {
    const applied = calls.find((c) => c[0] === "preset.apply");
    ok(applied, "preset.apply was never called");
    eq(applied[1], { name: "church" }, "the preset it asked for");
  });
  panel.close();
}

// ------------------------------------------------------- the number keys

/**
 * 1 to 9 count the scenes, and count the inputs only when there are none.
 *
 * Driven through the tray's own `takeSlot`, DOM lookup and all, because the
 * lookup is the part that decides which of the two lists a number means.
 */
async function numberKeySuite() {
  window.godwinmixPanels = window.godwinmixPanels || [];
  const { default: SourcesPanel } = await import("../panels/sources/panel.js");

  const taken = [];
  const tray = {
    order: () => ["cam1", "cam2", "cam3"],
    activate: (id) => taken.push(["input", id]),
    scenesPanel: SourcesPanel.prototype.scenesPanel,
    takeSlot: SourcesPanel.prototype.takeSlot,
  };

  test("with no scenes panel on the page a number is an input", () => {
    taken.length = 0;
    tray.takeSlot(2);
    eq(taken, [["input", "cam2"]]);
  });

  const node = document.createElement("gmx-scenes");
  // The real element would build itself on append and it has no client here.
  node.built = true;
  node.activate = (id) => taken.push(["scene", id]);
  node.scenes = { supported: true, scenes: () => [] };
  document.body.appendChild(node);

  test("a core with no scene server leaves the numbers on the inputs", () => {
    taken.length = 0;
    node.scenes = { supported: false, scenes: () => [{ id: "s1" }] };
    tray.takeSlot(1);
    eq(taken, [["input", "cam1"]]);
  });

  test("an empty collection leaves the numbers on the inputs", () => {
    taken.length = 0;
    node.scenes = { supported: true, scenes: () => [] };
    tray.takeSlot(3);
    eq(taken, [["input", "cam3"]]);
  });

  test("with scenes in the collection a number is a scene", () => {
    taken.length = 0;
    node.scenes = { supported: true, scenes: () => [{ id: "wide" }, { id: "two-box" }] };
    tray.takeSlot(2);
    eq(taken, [["scene", "two-box"]]);
  });

  test("a number past the end of the scenes does nothing at all", () => {
    taken.length = 0;
    node.scenes = { supported: true, scenes: () => [{ id: "wide" }] };
    tray.takeSlot(9);
    eq(taken, [], "it must not fall through to the ninth input");
  });

  node.remove();
}

// ------------------------------------------------------------- the kits

/**
 * The browser kit against the fixtures it generated.
 *
 * The TypeScript and Python ports replay this same file in their own test
 * runs. Replaying it here as well means a change to `ui/kits` that moves the
 * behaviour fails in the browser first, where it was made, rather than in
 * somebody else's language an hour later.
 */
async function kitSuite() {
  let fixtures;
  try {
    const res = await fetch("./fixtures.json");
    if (!res.ok) throw new Error(`fixtures.json answered ${res.status}`);
    fixtures = await res.json();
  } catch (e) {
    line("ok", `skipped the kit fixtures: ${e.message}`);
    return;
  }

  const r2 = (v) => (typeof v === "number" ? Math.round(v * 100) / 100 : v);
  const round = (v) => {
    if (Array.isArray(v)) return v.map(round);
    if (v && typeof v === "object") return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, round(x)]));
    return r2(v);
  };

  for (const c of fixtures.mirror) {
    test(`mirror: ${c.name}`, () => {
      const mirror = new SceneMirror({ clientId: c.clientId });
      const steps = [];
      for (const step of c.steps) {
        if (step.view) {
          mirror.applyView(step.view);
          steps.push({ kind: "view" });
        } else {
          const r = mirror.applyPatch(step.patch);
          steps.push({ kind: "patch", applied: r.applied, echo: r.echo, gap: r.gap, seq: r.seq });
        }
      }
      eq(steps, c.out.steps, "the step by step answers");
      eq(mirror.seq, c.out.seq, "the sequence number");
      eq(mirror.unknown, c.out.unknown, "unknown record kinds");
      eq(mirror.scenes().map((x) => x.id), c.out.scenes, "the scenes");
    });
  }

  for (const c of fixtures.predict) {
    test(`predict: ${c.name}`, () => {
      const p = new Prediction();
      const seqs = [];
      const settled = [];
      for (const [op, a, b] of c.ops) {
        if (op === "predict") seqs.push(p.predict(a, b));
        else settled.push(p.settle(a));
      }
      eq(seqs, c.out.seqs, "the numbers handed out");
      eq(settled, c.out.settled, "what each settle let go of");
      eq(p.acked, c.out.acked, "the last number the core applied");
      eq([...p.pending.keys()].sort(), c.out.pending, "what is still in flight");
      for (const [item, server] of Object.entries(c.resolve || {})) {
        eq(p.resolve(item, server), c.out.resolved[item], `what ${item} draws`);
      }
      eq((c.accepts || []).map(([item, echo]) => p.accepts(item, echo)), c.out.accepted, "which echoes are drawn");
    });
  }

  for (const c of fixtures.merge) {
    test(`merge: ${c.name}`, () => eq(mergeProps(c.base, c.next), c.out));
  }

  for (const c of fixtures.gizmos) {
    test(`gizmos: ${c.name}`, () => {
      const got = gizmosFor(c.designer).map((g) => ({ kind: g.kind, action: g.action, target: g.target, anchor: g.anchor }));
      eq(got, c.out);
    });
  }

  for (const c of fixtures.handles) {
    test(`handles: ${c.name}`, () => {
      const got = handlesFor(c.box, gizmosFor(c.designer)).map((h) => ({
        kind: h.kind,
        action: h.action,
        x: r2(h.x),
        y: r2(h.y),
        dir: h.dir,
        cursor: h.cursor,
      }));
      eq(got, c.out);
    });
  }

  for (const c of fixtures.drag) {
    test(`drag: ${c.name}`, () => {
      const got = applyDrag(c.handle, c.start, c.dx, c.dy, c.mods);
      eq({ props: round(got.props), box: got.box ? round(got.box) : null }, c.out);
    });
  }

  for (const c of fixtures.snap) {
    test(`snap: ${c.name}`, () => {
      const d = snapDelta(c.box, snapTargets(c.targets), c.opts);
      eq(
        { dx: r2(d.dx), dy: r2(d.dy), guides: d.guides.map((g) => ({ axis: g.axis, at: r2(g.at), span: g.span.map(r2) })) },
        c.out
      );
    });
  }

  for (const c of fixtures.safe) {
    test(`safe areas at ${c.canvas.width}x${c.canvas.height}`, () => eq(round(safeAreas(c.canvas)), c.out));
  }

  for (const c of fixtures.schema) {
    test(`schema: ${c.name}`, () => {
      const form = describeForm(c.schema, c.value);
      eq(form.groups, c.out.groups, "the groups");
      eq(
        form.fields.map((f) => ({
          name: f.name,
          kind: f.kind,
          group: f.group,
          required: f.required,
          visible: f.visible,
          unit: f.unit === undefined ? null : f.unit,
          value: f.value === undefined ? null : f.value,
          choices: f.choices ? f.choices.map((x) => x.value) : null,
        })),
        c.out.fields,
        "the fields"
      );
    });
  }

  for (const c of fixtures.read) {
    test(`read: ${c.name}`, () => {
      const form = describeForm(c.schema, c.value);
      const values = Object.assign({}, valuesOf(form), c.values);
      eq(readForm(form, values, new Set(c.touched)), c.out.read, "what the form sends");
      eq(missing(form, values), c.out.missing, "what is required and empty");
    });
  }

  for (const c of fixtures.uiSchema) {
    test(`ui schema: ${c.name}`, () => {
      const form = describeForm(c.schema, c.value);
      const layoutTree = applyRules(layoutFor(form, c.ui), valuesOf(form));
      eq(
        controlsOf(layoutTree).map((n) => ({
          field: n.field.name,
          control: n.control,
          visible: n.visible !== false,
          enabled: n.enabled !== false,
        })),
        c.out.controls
      );
    });
  }

  test("every fixture section is replayed here", () => {
    const sections = Object.keys(fixtures).filter((k) => k !== "note");
    eq(sections.sort(), ["drag", "gizmos", "handles", "merge", "mirror", "predict", "read", "safe", "schema", "snap", "uiSchema"]);
  });
}


// ------------------------------------- a plugin with a schema and no code

/**
 * 07 Phase 3: "a plugin that ships only a data schema and a `designer` block
 * gets an inspector and handles in the web designer and the Tkinter example
 * with no HTML in the plugin".
 *
 * The fixture is `tests/fixtures/designer/lower-third.json`, which is what
 * `plugin.describe` answers for such a plugin. The Python suite reads the same
 * file against the same kits, so the acceptance is one file and two toolkits.
 */
async function designerFixtureSuite() {
  let fixture;
  try {
    const res = await fetch("./designer-fixture.json");
    if (!res.ok) throw new Error(`answered ${res.status}`);
    fixture = await res.json();
  } catch (e) {
    line("ok", `skipped the designer fixture: ${e.message}`);
    return;
  }

  const asked = [];
  const client = {
    call: async (method, params) => {
      asked.push(method);
      if (method === "plugin.list") return { plugins: [fixture.plugin] };
      if (method === "plugin.describe") return fixture.describe;
      throw new RpcError(CODES.NO_METHOD, `no ${method} here`, {});
    },
  };

  const { Catalogue } = await import("../panels/composer/catalogue.js");
  const catalogue = new Catalogue(client);
  await catalogue.load();
  const type = catalogue.typeOf(fixture.record);

  test("the composer finds the plugin behind an item from plugin.list and plugin.describe", () => {
    ok(type, "the item's type was not resolved");
    eq(type.plugin, "lower-third");
    ok(type.schema && type.schema.properties, "no schema came back");
    ok(!type.designer.editor, "this fixture ships no editor, which is the point of it");
  });

  const gizmos = gizmosFor(type.designer);
  test("its handles come from its designer block, with the cage added for it", () => {
    eq(gizmos.map((g) => g.kind), fixture.expect.gizmo_kinds);
  });

  const handles = handlesFor(fixture.box, gizmos);
  test("the handles land on the item's own box", () => {
    const corners = handles.filter((h) => h.kind === "corner").map((h) => [h.x, h.y]);
    const box = fixture.box;
    ok(corners.some(([x, y]) => x === box.x && y === box.y), "no handle on the top left");
    ok(
      corners.some(([x, y]) => x === box.x + box.width && y === box.y + box.height),
      "no handle on the bottom right"
    );
    const rotate = handles.find((h) => h.kind === "rotate");
    ok(rotate && rotate.y < box.y, "the rotation handle is not above the item");
  });

  const { editorFor } = await import("../kits/schema/index.js");
  const editor = await editorFor({
    plugin: "lower-third",
    designer: type.designer,
    schema: type.schema,
    value: fixture.record.content.params,
  });

  test("it gets an inspector generated from its data schema", () => {
    eq(editor.mode, fixture.expect.editor_mode, "which link of the fallback chain");
    ok(editor.why.includes("data schema"), editor.why);
  });

  test("every property in the schema has a control, and the plugin wrote no HTML", () => {
    const controls = editor.el.querySelectorAll("input, select, textarea");
    ok(controls.length >= fixture.expect.controls.length, `only ${controls.length} controls`);
    const text = editor.el.textContent;
    for (const label of ["Name", "Role", "Bar colour", "Hold", "Slide in", "Side"]) {
      ok(text.includes(label), `no control labelled ${label}`);
    }
    ok(editor.el.querySelector(".unit"), "the unit from x-gmx-unit is not shown");
    ok(editor.el.querySelector("details"), "the advanced group is not a section");
  });

  test("the values it was given are in the form, and it reads them back", () => {
    const read = editor.read();
    eq(read.name, "Jane Okonjo", "the name it was handed");
    eq(read.hold_secs, 6, "the number it was handed, as a number");
    eq(read.colour, "#2f6f4f", "the schema's own default");
  });

  test("it asked the core only what the protocol has", () => {
    eq([...new Set(asked)].sort(), ["plugin.describe", "plugin.list"]);
  });
}

// ------------------------------------------------------- against a live core

/**
 * The Phase 3 acceptance gestures, driven through the real modules against the
 * core that served this page.
 *
 * Not a mock anywhere: the panel is the panel, the commands go over `/rpc`, and
 * the drag is pointer events on the composer's own overlay. It skips itself
 * when there is no core answering, so the page still runs from a file.
 *
 *   /test/?token=<token>       run it
 *   /test/?live=0              skip it
 */
async function liveSuite() {
  const params = new URLSearchParams(location.search);
  if (params.get("live") === "0") {
    line("ok", "the live suite was switched off with ?live=0");
    return;
  }
  let client;
  try {
    client = await connect({ token: params.get("token") });
    await waitFor(() => client.state.connected, 5000, "the socket to open");
  } catch (e) {
    line("ok", `skipped the live suite: no core answering (${e.message})`);
    return;
  }

  const made = [];
  const sources = [];
  for (const [id, uri] of [["t-bars", "test://smpte"], ["t-ball", "test://ball"]]) {
    try {
      await client.call("source.add", { id, uri, name: id === "t-bars" ? "Bars" : "Ball" });
      made.push(id);
    } catch (e) {
      // Already there from an earlier run is not a failure.
      if (!/exist|conflict/i.test(e.message || "")) throw e;
    }
    sources.push(id);
  }
  await waitFor(() => sources.every((id) => client.store.source(id)), 5000, "both test sources");

  window.godwinmixPanels = window.godwinmixPanels || [];
  const { default: ScenesPanel } = await import("../panels/scenes/panel.js");
  const panel = new ScenesPanel();
  panel.setClient(client);
  // A real size, because a marquee over a zero height grid selects nothing.
  panel.style.cssText = "display:block;width:900px;height:320px";
  document.body.appendChild(panel);
  panel.connectedCallback();
  await waitFor(() => panel.scenes.supported !== undefined && panel.scenes.summaries !== null, 5000, "scene.list");

  if (!panel.scenes.supported) {
    line("ok", "skipped the live suite: this core has no scene server");
    panel.remove();
    return;
  }

  const before = panel.scenes.scenes().length;

  // --- two tiles dragged onto empty space make a two box scene --------------

  window.dispatchEvent(
    new CustomEvent("gmx:tiles-dropped", {
      detail: { ids: sources, target: "scenes:empty", from: "sources", copy: false },
    })
  );
  await waitFor(() => panel.scenes.scenes().length > before, 8000, "the new scene");
  const scene = panel.scenes.scenes()[panel.scenes.scenes().length - 1];
  const view = panel.scenes.view(scene.id);

  test("dragging two inputs onto empty space makes a scene with no dialog", () => {
    eq(scene.items, 2, "two items");
    ok(view && view.geometry.length === 2, "the answer carried the flattened geometry");
  });

  test("the scene the count chose is a two box", () => {
    const [a, b] = view.geometry;
    const canvas = view.canvas;
    near(a.width, b.width, 2, "the two boxes are the same width");
    near(a.y, b.y, 2, "they sit at the same height");
    ok(Math.abs(a.x - b.x) > canvas.width / 4, "they are side by side, not stacked");
    // The layout leaves a gap and margins, which is why this is a fraction and
    // not an equality: 0.02 of the canvas three times over, by default.
    const covered = (a.width + b.width) / canvas.width;
    ok(covered > 0.9 && covered <= 1, `the two boxes cover ${(covered * 100).toFixed(1)}% of the width`);
  });

  test("a tile appeared for it, with its name on it", () => {
    const tile = panel.tiles.get(scene.id);
    ok(tile, "no tile");
    eq(tile.name.textContent, scene.name, "the tile's name");
  });

  // --- F2, then a colour ---------------------------------------------------

  const renamed = "Wide and guest";
  panel.beginRename(scene.id);
  const tile = panel.tiles.get(scene.id);
  tile.name.textContent = renamed;
  tile.name.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await waitFor(() => (panel.scenes.summary(scene.id) || {}).name === renamed, 5000, "the rename to land");

  const fresh = await client.call("scene.get", { scene: scene.id });
  test("F2 renames the scene on the document, not on this device", () => {
    eq(fresh.name, renamed, "the core's own copy of the name");
  });

  await panel.setColour([scene.id], "#2f6f4f");
  await waitFor(() => (panel.scenes.summary(scene.id) || {}).color === "#2f6f4f", 5000, "the colour");
  test("a colour from the menu is on the document too", () => {
    eq(panel.scenes.summary(scene.id).color, "#2f6f4f");
  });

  // --- a sweep over the tiles ----------------------------------------------

  const grid = panel.grid;
  const box = grid.getBoundingClientRect();
  sweep(grid, box.left + 2, box.top + 2, box.right - 2, box.bottom - 2);
  test("a sweep over empty space selects every tile it touches", () => {
    ok(panel.selection.size >= 1, `the sweep selected ${panel.selection.size} tiles`);
  });

  // --- on air --------------------------------------------------------------

  const tookAt = performance.now();
  let took = null;
  const offTook = client.on("event", (e) => {
    if (e.name === "program.took") took = e.params;
  });
  await panel.activate(scene.id);
  await waitFor(() => took, 5000, "event/program.took");
  offTook();
  test("tapping a scene tile puts it on air", () => {
    ok(took.scene === scene.id || took.scene === renamed, `the take names the scene: ${JSON.stringify(took)}`);
  });
  line("ok", `take to event/program.took: ${(performance.now() - tookAt).toFixed(1)} ms`);

  // --- copy a layout onto another scene ------------------------------------

  const more = await import("../panels/scenes/more.js");
  await more.duplicate(panel, [scene.id]);
  await waitFor(() => panel.scenes.scenes().length > before + 1, 8000, "the duplicate");
  const copy = panel.scenes.scenes().find((s) => s.id !== scene.id && s.name.includes(renamed.slice(0, 6)));

  const first = view.geometry[0].item;
  await panel.scenes.itemSet(scene.id, first, { transform: { position: { x: 240, y: 180 }, frame: { w: 480, h: 270 } } }, { duration_ms: 0 });
  await more.copyLayout(panel, scene.id);
  await more.pasteLayout(panel, [copy.id]);
  const pasted = await client.call("scene.get", { scene: copy.id });

  test("pasting a layout moves the matching items and leaves the rest alone", () => {
    const moved = pasted.geometry.find((g) => Math.abs(g.x - 240) < 2 && Math.abs(g.y - 180) < 2);
    ok(moved, `nothing landed where the layout says: ${JSON.stringify(pasted.geometry.map((g) => [g.x, g.y]))}`);
    eq(pasted.geometry.length, 2, "the other item is still there");
  });

  // --- the composer, and a drag at input rate ------------------------------

  await panel.open(scene.id);
  const composer = document.querySelector(".composer");
  test("a double tap opens the composer on a draft, off air", () => {
    ok(composer, "no composer dialog");
  });
  const live = panel.composer;
  ok(live, "the panel kept no handle on the composer it opened");
  await waitFor(() => live.canvas && live.canvas.entries().length === 2, 5000, "the composer's items");

  const item = live.canvas.entries()[0];
  live.canvas.setSelection([item.id]);
  const rect = live.canvas.overlay.getBoundingClientRect();
  const centre = live.canvas.viewport.toSurface(item.box.x + item.box.width / 2, item.box.y + item.box.height / 2);
  const startX = rect.left + centre.x;
  const startY = rect.top + centre.y;

  const wasX = item.box.x;
  point(live.canvas.overlay, "pointerdown", startX, startY);
  for (let i = 1; i <= 12; i += 1) point(live.canvas.overlay, "pointermove", startX + i * 6, startY + i * 2);
  point(live.canvas.overlay, "pointerup", startX + 72, startY + 24);
  await waitFor(() => !live.canvas.prediction.busy, 5000, "the core to catch up with the drag");

  test("a drag moves the item, locally first and in the core after", () => {
    const now = live.canvas.boxes.get(item.id);
    ok(now.x !== wasX, `the item did not move (was ${wasX}, is ${now.x})`);
  });

  const redraw = live.canvas.timings.stats("redraw");
  const echo = live.canvas.timings.stats("echo");
  for (const row of live.canvas.report()) line("ok", `drag timing, ${row}`);
  test("a drag redraws locally within 16 ms", () => {
    ok(redraw && redraw.p95 < 16, `p95 redraw was ${redraw ? redraw.p95.toFixed(2) : "not measured"} ms`);
  });
  test("the core's echo arrives under 25 ms on this host", () => {
    ok(echo && echo.p95 < 25, `p95 echo was ${echo ? echo.p95.toFixed(2) : "not measured"} ms`);
  });

  // --- Apply, then undo ----------------------------------------------------

  await live.apply();
  const applied = await client.call("scene.get", { scene: scene.id });
  test("Apply writes the draft back to the scene", () => {
    ok(applied.geometry.some((g) => Math.abs(g.x - wasX) > 1), "the move reached the scene");
  });

  // --- Delete, with the undo the toast offers ------------------------------

  const count = panel.scenes.scenes().length;
  await panel.remove([copy.id]);
  await waitFor(() => panel.scenes.scenes().length === count - 1, 8000, "the scene to go");
  test("Delete removes a scene", () => {
    eq(panel.scenes.summary(copy.id), null, "it is gone");
  });

  await shell.undo.undo();
  await panel.scenes.refresh();
  test("Ctrl+Z is the core's own history, so the scene comes back", () => {
    ok(panel.scenes.scenes().length === count, `there are ${panel.scenes.scenes().length} scenes, expected ${count}`);
  });

  // --- tidy up -------------------------------------------------------------

  for (const id of panel.scenes.scenes().filter((s) => s.name.includes(renamed.slice(0, 6))).map((s) => s.id)) {
    await client.call("scene.remove", { scene: id }).catch(() => {});
  }
  for (const id of made) await client.call("source.remove", { source: id }).catch(() => {});
  panel.remove();
  client.close();
}

/** A synthetic pointer event that the page's own handlers cannot tell apart. */
function point(node, type, x, y) {
  node.dispatchEvent(
    new PointerEvent(type, { clientX: x, clientY: y, pointerId: 1, isPrimary: true, button: 0, buttons: type === "pointerup" ? 0 : 1, bubbles: true })
  );
}

/** Press on empty space, drag, release: the marquee. */
function sweep(node, x0, y0, x1, y1) {
  point(node, "pointerdown", x0, y0);
  point(node, "pointermove", x0 + 8, y0 + 8);
  point(node, "pointermove", x1, y1);
  point(node, "pointerup", x1, y1);
}

/** Wait for something to become true, or say what it was waiting for. */
function waitFor(predicate, ms, what) {
  const started = Date.now();
  return new Promise((resolve, reject) => {
    const tick = () => {
      let done = false;
      try {
        done = predicate();
      } catch {
        done = false;
      }
      if (done) return resolve(true);
      if (Date.now() - started > ms) return reject(new Error(`timed out after ${ms} ms waiting for ${what || "something"}`));
      setTimeout(tick, 25);
    };
    tick();
  });
}

// ---------------------------------------------------------------- summary

function summarise() {
  const summary = `${passed} passed, ${failed} failed`;
  line(failed ? "fail" : "ok", summary);
  document.title = (failed ? "FAIL " : "ok ") + summary;
  if (out) out.dataset.result = failed ? "fail" : "pass";
  console.log(failed ? `FAILED: ${summary}` : `ALL PASSED: ${summary}`);
}

legacySuite()
  .catch((e) => {
    failed += 1;
    line("fail", "the legacy suite threw: " + e.message);
    console.error(e);
  })
  .then(welcomeSuite)
  .catch((e) => {
    failed += 1;
    line("fail", "the welcome suite threw: " + e.message);
    console.error(e);
  })
  .then(numberKeySuite)
  .catch((e) => {
    failed += 1;
    line("fail", "the number key suite threw: " + e.message);
    console.error(e);
  })
  .then(kitSuite)
  .catch((e) => {
    failed += 1;
    line("fail", "the kit suite threw: " + e.message);
    console.error(e);
  })
  .then(designerFixtureSuite)
  .catch((e) => {
    failed += 1;
    line("fail", "the designer fixture suite threw: " + e.message);
    console.error(e);
  })
  .then(liveSuite)
  .catch((e) => {
    failed += 1;
    line("fail", "the live suite threw: " + e.message);
    console.error(e);
  })
  .then(summarise);
