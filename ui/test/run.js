import { recordingState } from "../panels/outputs/recording.js";
import { sourceChooserTests } from "./source-chooser.js";
import { studioTests } from "./studio.js";
import { dockTests } from "./dock.js";
// The test runner: forty lines, no dependencies, no toolchain. Open the page,
// read the console, or read the list. Everything testable without a mixer is
// here, including the legacy adapter against a stubbed server.

import { Selection, overlaps, rectFrom } from "../shell/selection.js";
import { dbToPos, FLOOR } from "../shell/meter.js";
import { posToGain, gainToPos, gainLabel, UNITY, AudioGestures, ScrubGestures, audioFor } from "../shell/fader.js";
import {
  parseFrame,
  sheetWidthFor,
  HEADER_BYTES,
  PREVIEW_STREAM,
  SheetPainter,
  PicturePainter,
} from "../client/frames.js";
import { Store } from "../client/store.js";
import { SchemaForm } from "../client/schema-form.js";
import { Client } from "../client/index.js";
import { RpcError, CODES } from "../client/errors.js";
import { rank, paramsSchema, methodForm } from "../shell/palette.js";
import { chordOf, DEFAULT_MAP } from "../shell/keymap.js";
import { IS_MAC } from "../shell/dom.js";
import { kindOfUri, PLATFORMS, platformOfHost, joinKey } from "../client/kinds.js";
import { schemaFor, paramsFor } from "../panels/outputs/destination.js";
import { stateLabel, dotClass } from "../panels/outputs/panel.js";
import { tagFor } from "../shell/registry.js";
import * as layout from "../shell/layout.js";
import { ART } from "../panels/welcome/tiles.js";
import { WelcomePanel } from "../panels/welcome/panel.js";
import { mosaicWanted } from "../panels/multiview/wanted.js";
import { connect } from "../client/index.js";
import { shell, panelSection } from "../shell/shell.js";
import { buildTile, syncTile } from "../panels/sources/tile.js";
import { settableOnly, setRequest } from "../panels/sources/setreq.js";
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

dockTests(test, eq, ok);

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

test("recording startup is shown as preparing rather than a failure", () => {
  eq(recordingState({ state: "connecting" }), { label: "Preparing recording", dot: "connecting" });
  eq(recordingState({ state: "live" }).label, "Recording");
  eq(recordingState({ state: "failed" }).dot, "failed");
});

test("releasing an unchanged fader unblocks shared panel rendering", () => {
  const audio = new AudioGestures({ call: () => Promise.resolve({}) });
  const input = document.createElement("input");
  input.type = "range";
  audio.bindFader(input, "cam-wide", "gain");
  input.dispatchEvent(new PointerEvent("pointerdown", { detail: 1 }));
  ok(audio.busy);
  let rendered = false;
  audio.defer(() => { rendered = true; });
  input.dispatchEvent(new PointerEvent("pointerup"));
  ok(!audio.busy);
  ok(rendered);
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

test("audio controls and seeking send the API source id", () => {
  const calls = [];
  const client = { call: (method, params) => { calls.push({ method, params }); return Promise.resolve({}); } };
  const audio = new AudioGestures(client);
  audio.setMuted("cam1", true);
  audio._post("cam1", "gain", UNITY);
  const scrub = new ScrubGestures(client);
  scrub._post("cam1", 500);
  eq(calls.map((call) => call.params.id), ["cam1", "cam1", "cam1"]);
  ok(calls.every((call) => !("source" in call.params)));
});

test("audio panels share one gesture state and display local gain in gain units", () => {
  const client = { call: async () => ({}) };
  const audio = audioFor(client);
  eq(audio === audioFor(client), true);
  audio.active.add("camera/gain");
  eq(audio.shown("camera/gain", 1), 1);
  audio.local.set("camera/gain", UNITY);
  eq(audio.shown("camera/gain", 0.5), 1);
});

test("saved layouts keep control panels outside the monitor", () => {
  const old = { main: ["core/program", "core/sources"], footer: ["core/outputs", "core/media"] };
  const migrated = layout.place(old, "main", "core/scenes");
  eq(migrated.monitor, ["core/program"]);
  ok(!migrated.main.includes("core/program"));
  ok(migrated.main.includes("core/outputs") && migrated.main.includes("core/media"));
  // And the scenes lead the sources, whatever order the saved layout had.
  eq(layout.place({ main: ["core/sources", "core/scenes"] }, "sidebar", "ndi/senders").main, ["core/scenes", "core/sources"]);
});

test("the mute button toggles the latest source state", () => {
  const muted = [];
  const source = { id: "cam1", uri: "test://smpte", muted: false, gain: 1 };
  const tile = buildTile(source, { audio: { bindFader() {} }, scrub: {}, onMute: (id, value) => muted.push(value) });
  syncTile(tile, source, {});
  tile.mute.click();
  syncTile(tile, { ...source, muted: true }, {});
  tile.mute.click();
  eq(muted, [true, false]);
});

test("control sections collapse without destroying their panels", () => {
  const panel = document.createElement("div");
  const section = panelSection("core/sources", panel);
  eq(section.tagName, "DETAILS");
  eq(section.querySelector("summary").textContent, "Sources");
  section.open = false;
  ok(section.contains(panel));
  section.open = true;
  ok(section.contains(panel));
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

test("a live scene survives the snapshot a reconnect brings", () => {
  // The core reports a multi item scene as `scene`, with `program` null,
  // because there is no single source to name. The take event used to be
  // folded into `program`, so the two disagreed and any reconnect under a
  // live scene left the desk reading "black" while it was on air.
  const store = new Store();
  store.patch({ program: null, scene: "Two box" });
  eq(store.state.scene, "Two box");
  store.snapshot({ program: null, scene: "Two box", sources: [] }, 1);
  eq(store.state.scene, "Two box", "the snapshot still knows what is on air");
  eq(store.state.program, null, "and does not invent a source for it");

  // A single source take still names the source, which is what tally reads.
  store.snapshot({ program: "cam1", scene: null, sources: [] }, 2);
  eq(store.tallyOf("cam1"), "program");
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

// ---------------------------------------------------------------- outputs

test("a platform is recognised from the masked host the core hands out", () => {
  eq(platformOfHost("rtmp://a.rtmp.youtube.com/\u2026").id, "youtube");
  eq(platformOfHost("rtmps://live-api-s.facebook.com/\u2026").id, "facebook");
  eq(platformOfHost("rtmp://live.twitch.tv/\u2026").id, "twitch");
  // A Twitch regional ingest, which is not the default one on the table.
  eq(platformOfHost("rtmp://lhr03.contribute.live-video.net/\u2026").id, "twitch");
  // Anything else falls back to something the form can still open with.
  eq(platformOfHost("rtmp://rtmp.church.example/\u2026").id, "custom");
  eq(platformOfHost("srt://192.168.1.50/\u2026").id, "srt");
  eq(platformOfHost("").id, "custom");
});

test("a stream key pasted with whitespace round it is trimmed onto the server", () => {
  eq(joinKey("rtmp://a.rtmp.youtube.com/live2", "  abcd-efgh-ijkl \n"), "rtmp://a.rtmp.youtube.com/live2/abcd-efgh-ijkl");
  // A trailing slash on the server must not double up.
  eq(joinKey("rtmp://a.rtmp.youtube.com/live2/", "abcd"), "rtmp://a.rtmp.youtube.com/live2/abcd");
  // No key is a server on its own, not a trailing slash.
  eq(joinKey("srt://192.168.1.50:9000", ""), "srt://192.168.1.50:9000");
});

test("adding a destination asks for the key and builds the whole address", () => {
  const yt = PLATFORMS.find((p) => p.id === "youtube");
  const form = new SchemaForm(schemaFor(yt, null), {});
  // The ingest is filled in and the id defaults to the platform, so a
  // volunteer has one box to touch.
  eq(form.read().id, "youtube");
  eq(form.read().server, "rtmp://a.rtmp.youtube.com/live2");
  eq(form.read().key, undefined, "an untyped secret is never sent");
  eq(form.missing(), ["key"], "the key is the one thing still wanted");

  const key = form.fields.find((f) => f.name === "key").input;
  eq(key.type, "password", "a stream key is never on screen in the clear");
  key.value = " abcd-efgh-ijkl ";
  key.dispatchEvent(new Event("input"));

  const asked = paramsFor(yt, null, form.read());
  eq(asked.error, undefined);
  eq(asked.params.uri, "rtmp://a.rtmp.youtube.com/live2/abcd-efgh-ijkl");
  eq(asked.params.id, "youtube");
  eq(asked.params.policy, "cdn", "a platform gets the backing off policy by default");
});

test("editing a destination sends no address unless the key was retyped", () => {
  const yt = PLATFORMS.find((p) => p.id === "youtube");
  const output = { id: "youtube", uri_host: "rtmp://a.rtmp.youtube.com/\u2026", state: "reconnecting", reconnects: 47, queue_secs: 0, has_key: false };
  const form = new SchemaForm(schemaFor(yt, output), {});
  eq(form.read().key, undefined, "the key field starts empty");
  eq(form.fields.find((f) => f.name === "key").input.placeholder, "kept");
  ok(!form.fields.find((f) => f.name === "id"), "the id is not editable here");

  // Only the buffer moved. Sending a uri here would need a key nobody has.
  const queue = form.fields.find((f) => f.name === "queue_secs").input;
  queue.value = "8";
  queue.dispatchEvent(new Event("input"));
  let asked = paramsFor(yt, output, form.read());
  eq(asked.params.uri, undefined, "an untouched key must not be rebuilt");
  eq(asked.params.queue_secs, 8);
  eq(asked.params.policy, undefined, "keep means keep");

  // Now the key, which rebuilds the whole address from the ingest on file.
  const key = form.fields.find((f) => f.name === "key").input;
  key.value = "wxyz-1234";
  key.dispatchEvent(new Event("input"));
  asked = paramsFor(yt, output, form.read());
  eq(asked.params.uri, "rtmp://a.rtmp.youtube.com/live2/wxyz-1234");
  eq(asked.params.id, "youtube");
});

test("a server changed on its own says what else it needs", () => {
  const twitch = PLATFORMS.find((p) => p.id === "twitch");
  const output = { id: "twitch", uri_host: "rtmp://live.twitch.tv/\u2026", state: "live", reconnects: 0, queue_secs: 1, has_key: true };
  // A regional ingest pasted over the default, with the key left alone. The
  // key is half the address and the core will not hand it back, so this has
  // to be refused out loud rather than half applied or silently dropped.
  const asked = paramsFor(twitch, output, { server: "rtmp://lhr03.contribute.live-video.net/app" });
  ok(asked.error, "a half address must not go through");
  ok(asked.error.includes("Replace key"), asked.error);
  eq(asked.params, undefined);
});

test("an SRT destination has an address and no key at all", () => {
  const srt = PLATFORMS.find((p) => p.id === "srt");
  const form = new SchemaForm(schemaFor(srt, null), {});
  ok(!form.fields.find((f) => f.name === "key"), "there is no stream key in SRT");
  const server = form.fields.find((f) => f.name === "server").input;
  server.value = "srt://192.168.1.50:9000";
  server.dispatchEvent(new Event("input"));
  const asked = paramsFor(srt, null, form.read());
  eq(asked.params.uri, "srt://192.168.1.50:9000");
  eq(asked.params.policy, "own");
});

test("an output's state is spelled out as the next thing to do about it", () => {
  eq(stateLabel({ state: "live", has_key: true }), "Live");
  eq(stateLabel({ state: "reconnecting", reconnects: 12, has_key: true }), "Reconnecting, attempt 12");
  eq(stateLabel({ state: "connecting", has_key: true }), "Connecting");
  eq(stateLabel({ state: "failed", has_key: true }), "Stopped");
  // The placeholder a preset wrote, which is the whole reason this reads in
  // words: "Reconnecting, attempt 47" tells nobody to go and paste a key.
  eq(stateLabel({ state: "reconnecting", reconnects: 47, has_key: false }), "Needs a stream key");
  eq(dotClass({ state: "reconnecting", has_key: false }), "failed");
  // An older core says nothing about keys, and must not be read as missing one.
  eq(stateLabel({ state: "live" }), "Live");
  eq(stateLabel({ state: "reconnecting", reconnects: 2 }), "Reconnecting, attempt 2");
});

// ---------------------------------------------------------------- palette

test("the palette prefers a title that starts with what was typed", () => {
  const cmds = [
    { id: "a", title: "Take the armed tile", group: "Programme", run() {} },
    { id: "b", title: "Add a source", group: "Sources", run() {} },
    { id: "c", title: "Remove the selection", group: "Sources", run() {} },
  ];
  eq(rank(cmds, "take")[0].id, "a");
  eq(rank(cmds, "add")[0].id, "b");
  eq(rank(cmds, "zzz").length, 0);
});

test("a method whose params are a $ref still gets its fields", () => {
  // How nearly every method in the protocol is written: params point into the
  // document's $defs. Reading `properties` off the reference finds nothing,
  // which is what made the palette send {} for 91 of the 123 methods.
  const method = {
    name: "plugin.add",
    params: { $ref: "#/$defs/AddPluginRequest" },
    $defs: {
      AddPluginRequest: {
        type: "object",
        required: ["source"],
        properties: { source: { type: "string", title: "Source" } },
      },
    },
  };
  const schema = paramsSchema(method);
  ok(schema.properties && schema.properties.source, "the field came through");
  const form = new SchemaForm(schema, {});
  ok(form.el.querySelector("input"), "and the form drew a control for it");
});

test("a destructive method asks before it calls", () => {
  // core.shutdown takes no parameters, so its palette form has always worked
  // and its Call button was one click from the programme going off air.
  let calls = 0;
  const client = { call: async () => { calls += 1; return {}; } };
  const form = methodForm(
    client,
    { name: "core.shutdown", summary: "Stop the mixer.", destructive: true, params: { type: "object", properties: {} } },
    SchemaForm
  );
  const button = (label) => [...document.querySelectorAll("button")].find((b) => b.textContent === label);
  // The handler is async, but confirmModal is reached before its first await,
  // so the question is on screen by the time click() returns.
  button("Call").click();
  ok(document.body.textContent.includes("Are you sure?"), "it asked first");
  eq(calls, 0, "and called nothing while it waited");
  button("Cancel").click();
  eq(calls, 0, "Cancel means no");
  form.close();
});

test("a method that takes nothing still gets an empty form, not a broken one", () => {
  const schema = paramsSchema({ name: "core.shutdown", params: { type: "object", properties: {} } });
  eq(Object.keys(schema.properties || {}).length, 0);
});

// ------------------------------------------------ add source picker

/**
 * The picker that replaced four address boxes.
 *
 * Driven against a stubbed client rather than the core, because what is being
 * checked is what the modal draws and what it sends: that the categories are
 * on screen before discovery answers, that a found camera becomes a row whose
 * button sends the params the device already handed over, and that a category
 * whose plugin is missing offers to install it rather than naming a command.
 */
async function addSourcePickerSuite() {
  const { openPicker } = await import("../shell/picker-loader.js");
  const kinds = await import("../client/kinds.js");

  /** A client that answers from a table and remembers what it was asked. */
  function stub(over) {
    const answers = Object.assign(
      {
        "core.api": {
          kinds: {
            source: [
              { id: "file/source" },
              { id: "browser/source" },
              { id: "rtmp/source" },
              { id: "hls/source" },
              { id: "exec/source" },
              { id: "test/source" },
            ],
          },
        },
        "plugin.list": { plugins: [] },
        "device.discover": { candidates: [] },
        "media.list": { items: [] },
        "source.add": (p) => ({ id: "added", uri: p.uri, name: p.name }),
      },
      over
    );
    return {
      calls: [],
      state: { sources: [] },
      call(method, params) {
        this.calls.push([method, params]);
        const answer = answers[method];
        if (answer === undefined) return Promise.reject(new Error(`no stub for ${method}`));
        return Promise.resolve(typeof answer === "function" ? answer(params) : answer);
      },
      sent(method) {
        const hit = this.calls.find((c) => c[0] === method);
        return hit ? hit[1] : null;
      },
    };
  }

  const CAMERA_PLUGIN = {
    name: "camera",
    version: "0.1.0",
    description: "A USB or built in camera as a source",
    enabled: true,
    provides: ["camera/source", "camera/devices"],
  };
  const railTitles = (m) => [...m.el.querySelectorAll(".picker-rail button")].map((b) => b.textContent.trim());
  const panelText = (m) => m.el.querySelector(".picker-panel").textContent;
  const buttonSaying = (m, text) =>
    [...m.el.querySelectorAll(".picker-panel button")].find((b) => b.textContent.includes(text));

  // ------------------------------------------------------------ the table

  // The bug: `loadKinds` answered the moment `core.api` yielded anything, and
  // `core.api` always yields something, so a camera plugin could be installed
  // and running and the picker still showed four address boxes.
  const catalogue = await kinds.loadKinds(stub({ "plugin.list": { plugins: [CAMERA_PLUGIN] } }), "source");

  test("a plugin's source kind reaches the picker even though the core named its own", () => {
    ok(catalogue.some((k) => k.id === "camera/source"), "the plugin's kind is in the catalogue");
    ok(catalogue.some((k) => k.id === "file"), "and the built in kinds are still there");
    ok(!catalogue.some((k) => k.id === "camera/devices"), "a device provide is not something to add");
  });

  test("a kind lands in the category an operator would look in", () => {
    const camera = { id: "camera/source", provides: ["camera/source"] };
    eq(kinds.categoryOf(camera), "cameras");
    eq(kinds.categoryOf({ id: "page", provides: ["browser/source"] }), "web");
    eq(kinds.categoryOf({ id: "test", provides: ["test/source"] }), "test");
    // Anything nobody has placed is still offered, at the bottom.
    eq(kinds.categoryOf({ id: "odd/source", provides: ["odd/source"] }), "more");
  });

  test("a candidate is already a source.add request", () => {
    const req = kinds.addRequestFor({
      type: "camera/source",
      name: "Logitech BRIO",
      params: { device: "/dev/video0", label: "Logitech BRIO" },
    });
    // A kind named outright has no address, and the core still keys an id off
    // `uri`, so the type goes there. `gmx ctl source add --type` does the same.
    eq(req.uri, "camera/source");
    eq(req.type, "camera/source");
    eq(req.name, "Logitech BRIO");
    eq(req.device, "/dev/video0");
    eq(kinds.addRequestFor({ type: "ndi/source", params: { uri: "ndi://hall" } }).uri, "ndi://hall");
  });

  test("a device that is already a source is not offered twice", () => {
    const candidate = { type: "camera/source", name: "Logitech BRIO", params: { device: "/dev/video0" } };
    const sources = [{ id: "logitech-brio", name: "Logitech BRIO", uri: "camera/source" }];
    ok(kinds.alreadyAdded(sources, candidate), "the name it was added under");
    const other = { type: "camera/source", name: "MacBook Pro Camera", params: { device: "1" } };
    // Every camera on a machine shares the one URI, so the URI must not be
    // allowed to answer this on its own.
    ok(!kinds.alreadyAdded(sources, other), "a second camera is still on offer");
  });

  test("the size a device advertises is read wherever it put it", () => {
    eq(kinds.candidateSize({ params: { width: 1920, height: 1080 } }), "1920 x 1080");
    eq(kinds.candidateSize({ params: { best_size: [1280, 720] } }), "1280 x 720");
    eq(kinds.candidateSize({ params: { device: "/dev/video0" } }), "");
  });

  const listed = await kinds.pluginSourceFor(
    stub({ "plugin.search": { results: [{ name: "camera", source: "./plugins/camera" }] } }),
    "camera"
  );
  const unlisted = await kinds.pluginSourceFor(stub({}), "camera");

  test("a marketplace decides what plugin.add is sent, and there is a fallback", () => {
    eq(listed, "./plugins/camera");
    eq(unlisted, "psmux/godwinmix", "a mixer that knows no marketplace still has somewhere to go");
  });

  // ------------------------------------------------------------ the modal

  let held = null;
  const looking = stub({
    "plugin.list": { plugins: [CAMERA_PLUGIN] },
    "device.discover": () => new Promise((resolve) => (held = resolve)),
  });
  const cameras = await openPicker(looking, "source", { category: "cameras" });

  test("every category is on screen before any hardware has answered", () => {
    eq(railTitles(cameras), [
      "Cameras",
      "Screens and windows",
      "Microphones and audio",
      "Video and images",
      "Web pages",
      "Streams and feeds",
      "Test patterns",
      "More",
    ]);
    ok(panelText(cameras).includes("Looking for devices"), "and it says what it is doing");
    ok(held, "discovery was asked, and the modal did not wait for it");
  });

  held({
    candidates: [
      {
        type: "camera/source",
        name: "Fake Camera 1",
        params: { device: "/dev/video0", label: "Fake Camera 1", width: 1920, height: 1080 },
      },
    ],
  });
  await waitFor(() => panelText(cameras).includes("Fake Camera 1"), 2000, "the camera row to appear");

  test("a found camera is a row with its name, its size and one button", () => {
    ok(panelText(cameras).includes("1920 x 1080"), "the size it advertises");
    const row = cameras.el.querySelector(".picker-row");
    ok(row.querySelector("svg"), "a kind icon");
    eq(row.querySelector("button").textContent, "Add");
  });

  cameras.el.querySelector(".picker-row button").click();
  await waitFor(() => looking.sent("source.add"), 2000, "the add to be sent");

  test("Add sends the params the device handed over, and the name defaults to the device", () => {
    const sent = looking.sent("source.add");
    eq(sent.type, "camera/source");
    eq(sent.uri, "camera/source");
    eq(sent.name, "Fake Camera 1");
    eq(sent.device, "/dev/video0");
  });

  await waitFor(() => panelText(cameras).includes("Added"), 2000, "the row to settle");

  test("a device that has just been added says so and cannot be added again", () => {
    const button = cameras.el.querySelector(".picker-row button");
    eq(button.textContent, "Added");
    ok(button.disabled, "and it is disabled");
  });

  test("typing searches across every category at once", () => {
    const search = cameras.el.querySelector(".picker-head input");
    search.value = "bars";
    search.dispatchEvent(new Event("input"));
    const text = panelText(cameras);
    ok(text.includes("Test patterns"), "the category it was found in is named");
    ok(text.includes("Colour bars"), "and the pattern is there");
    ok(!text.includes("Fake Camera 1"), "what does not match is gone");
    search.value = "rtmp";
    search.dispatchEvent(new Event("input"));
    ok(panelText(cameras).includes("Incoming stream"), "a kind's description is searched too");
    search.value = "";
    search.dispatchEvent(new Event("input"));
  });

  [...cameras.el.querySelectorAll(".picker-rail button")]
    .find((b) => b.textContent.includes("Test patterns"))
    .click();
  buttonSaying(cameras, "Add").click();
  await waitFor(() => looking.calls.filter((c) => c[0] === "source.add").length === 2, 2000, "the second add");

  test("a test pattern is one click and needs no typing", () => {
    const sent = looking.calls.filter((c) => c[0] === "source.add")[1][1];
    eq(sent.uri, "test://smpte");
    eq(sent.name, "Colour bars");
  });

  cameras.close();

  // ------------------------------------------------------------ installing

  const bare = stub({
    "plugin.list": { plugins: [] },
    "plugin.search": { results: [{ name: "camera", source: "./plugins/camera" }] },
    "plugin.add": { name: "camera", version: "0.1.0" },
  });
  const missing = await openPicker(bare, "source", { category: "cameras" });

  test("a category whose plugin is missing still appears, and offers to install it", () => {
    ok(panelText(missing).includes("Cameras need the camera plugin"), "one plain sentence");
    ok(buttonSaying(missing, "Install camera support"), "and a button, not a terminal command");
  });

  buttonSaying(missing, "Install camera support").click();
  await waitFor(() => bare.sent("plugin.add"), 2000, "the install to be sent");

  test("Install calls plugin.add with what the marketplace named", () => {
    eq(bare.sent("plugin.add").source, "./plugins/camera");
    ok(bare.calls.filter((c) => c[0] === "device.discover").length >= 1, "and it looks for devices again");
  });

  missing.close();

  // ------------------------------------------------------------ the library

  const withMedia = stub({
    "media.list": { items: [{ name: "opener.mp4", path: "/srv/media/opener.mp4", size_bytes: 4096 }] },
  });
  const files = await openPicker(withMedia, "source", { category: "files" });
  await waitFor(() => panelText(files).includes("opener.mp4"), 2000, "the library listing");

  test("the files category offers the library and a way to a path", () => {
    ok(panelText(files).includes("A file somewhere else"), "the file that is not in the library");
    ok(buttonSaying(files, "Browse"), "which is the file kind's own form");
  });

  buttonSaying(files, "Add").click();
  await waitFor(() => withMedia.sent("source.add"), 2000, "the clip to be added");

  test("a library clip is added by its path, under its own name", () => {
    eq(withMedia.sent("source.add").uri, "/srv/media/opener.mp4");
    eq(withMedia.sent("source.add").name, "opener.mp4");
  });

  files.close();
}

// ------------------------------------------------------- scoped sources

/**
 * The tray scoped to one scene: what it shows, what it does with a source it
 * has just added, and what it asks before one replaces a scene on air.
 *
 * Driven through the panel's own prototype against a stubbed `gmx-scenes` on
 * the page, the way the number keys are, because the DOM lookup is half of
 * what these methods do.
 */
async function scopedSourcesSuite() {
  window.godwinmixPanels = window.godwinmixPanels || [];
  const { default: SourcesPanel } = await import("../panels/sources/panel.js");
  const { openForm } = await import("../shell/picker-loader.js");
  const { setFocusedScene } = await import("../shell/focus.js");
  const { setSetting } = await import("../shell/settings.js");

  let summaries = [];
  const added = [];
  const node = document.createElement("gmx-scenes");
  // The real element would build itself on append and it has no client here.
  node.built = true;
  node.scenes = {
    supported: true,
    scenes: () => summaries,
    summary: (id) => summaries.find((x) => x.id === id || x.name === id) || null,
    itemAdd: async (scene, content) => {
      added.push([scene, content]);
    },
    reread: async () => {},
    undo: { record: () => {} },
  };
  document.body.appendChild(node);

  const state = { sources: [{ id: "cam1", uri: "a" }, { id: "cam2", uri: "b" }, { id: "slides", uri: "c" }] };
  const tray = {
    scope: "scene",
    filter: "",
    client: { state: { scene: null } },
    render: () => {},
    sources: SourcesPanel.prototype.sources,
    scopedTo: SourcesPanel.prototype.scopedTo,
    focusedSummary: SourcesPanel.prototype.focusedSummary,
    sceneClient: SourcesPanel.prototype.sceneClient,
    untouchedScene: SourcesPanel.prototype.untouchedScene,
    place: SourcesPanel.prototype.place,
    askBeforeTake: SourcesPanel.prototype.askBeforeTake,
  };
  const shown = () => tray.sources(state).map((x) => x.id);

  summaries = [
    { id: "wide", name: "Wide", items: 2, sources: ["cam1", "slides"] },
    { id: "two", name: "Two box", items: 2, sources: ["cam1", "cam2"] },
  ];
  setFocusedScene("wide");

  test("in scene scope the tray keeps only what the focused scene draws", () => {
    eq(shown(), ["cam1", "slides"]);
  });

  test("the filter still applies inside the scope", () => {
    tray.filter = "slides";
    eq(shown(), ["slides"]);
    tray.filter = "";
  });

  test("an old all sources preference cannot bypass the selected scene", () => {
    tray.scope = "all";
    eq(shown(), ["cam1", "slides"]);
    tray.scope = "scene";
  });

  test("a deleted scene focus falls back to the first remaining scene", () => {
    setFocusedScene("deleted");
    eq(shown(), ["cam1", "slides"]);
  });

  test("a collection with no scenes in it falls back to all", () => {
    summaries = [];
    setFocusedScene("wide");
    eq(shown(), ["cam1", "cam2", "slides"]);
  });

  // ---------------------------------------------- adding from inside a scope

  const order = [];
  const client = {
    call: async (method, params) => {
      order.push(method);
      return { id: "cam9", name: "Cam 9", uri: params.uri };
    },
  };
  const kind = {
    id: "stub",
    title: "Stub source",
    description: "A kind that exists for this test and nowhere else.",
    schema: { type: "object", properties: { uri: { type: "string", title: "Address" } } },
    build: (values) => ({ uri: values.uri }),
  };
  const answers = [];
  const form = await openForm(client, "source", kind, { uri: "rtmp://x/y" }, {
    onAdded: (status) => {
      order.push("onAdded");
      answers.push(status);
    },
  });
  [...form.el.querySelectorAll("button")].find((b) => b.textContent === "Add").click();
  await waitFor(() => answers.length > 0, 2000, "the picker to call onAdded");

  test("the picker hands the source it just made to whoever opened it", () => {
    eq(order, ["source.add", "onAdded"], "and only once source.add has answered");
    eq(answers[0].id, "cam9", "with the id the mixer assigned");
  });

  summaries = [{ id: "default", name: "Default", items: 0, sources: [] }];
  added.length = 0;
  await tray.place(null, { id: "cam9" });
  const untouched = added.slice();

  summaries = [{ id: "default", name: "Default", items: 1, sources: ["cam1"] }];
  added.length = 0;
  await tray.place(null, { id: "cam9" });
  const started = added.slice();

  summaries = [
    { id: "a", name: "A", items: 0, sources: [] },
    { id: "b", name: "B", items: 0, sources: [] },
  ];
  added.length = 0;
  await tray.place(null, { id: "cam9" });
  const two = added.slice();

  test("the one empty scene a fresh mixer boots with takes the first source", () => {
    eq(untouched, [["default", { source: "cam9" }]]);
  });

  test("a scene somebody has already put something in is left alone", () => {
    eq(started, [], "All means all once the scene is not untouched");
  });

  test("and a collection of more than one scene is never guessed at", () => {
    eq(two, []);
  });

  // ----------------------------------------------- taking a bare source

  summaries = [{ id: "wide", name: "Wide", items: 3, sources: [] }];
  tray.client = { state: { scene: "Wide" } };
  const asked = tray.askBeforeTake();
  const wasAsked = document.body.textContent.includes("Are you sure?");
  const cancel = [...document.querySelectorAll(".dialog button")].find((b) => b.textContent === "Cancel");
  if (cancel) cancel.click();
  const answer = await asked;

  test("a source that would replace a live scene of three asks first", () => {
    ok(wasAsked, "nothing was asked");
    eq(answer, false, "Cancel means the take does not happen");
  });

  summaries = [{ id: "solo", name: "Solo", items: 1, sources: [] }];
  tray.client = { state: { scene: "Solo" } };
  const oneItem = await tray.askBeforeTake();

  summaries = [{ id: "wide", name: "Wide", items: 3, sources: [] }];
  tray.client = { state: { scene: null } };
  const nothingLive = await tray.askBeforeTake();

  tray.client = { state: { scene: "Wide" } };
  setSetting("confirmTake", false);
  const switchedOff = await tray.askBeforeTake();
  setSetting("confirmTake", true);

  test("nothing is asked when there is nothing to lose", () => {
    ok(oneItem, "a scene of one item is that one source, so there is nothing to replace");
    ok(nothingLive, "a source on air rather than a scene");
    ok(switchedOff, "and the setting switches the question off entirely");
    ok(!document.body.textContent.includes("Are you sure?"), "no question was left on screen");
  });

  // The page goes on to drive the real panels against a real core, and the
  // focus is remembered on the device the tests run on.
  setFocusedScene(null);
  node.remove();
}

// ------------------------------------------------------- the source drawer

test("the drawer does not offer an address source.set cannot change", () => {
  const kind = {
    type: "object",
    required: ["uri"],
    properties: { uri: { type: "string" }, name: { type: "string" } },
  };
  const settable = settableOnly(kind);
  ok(!("uri" in settable.properties), "uri is gone");
  ok(settable.required.indexOf("uri") === -1, "and it is not required either");
  ok("name" in settable.properties, "what can be set is still there");
});

test("what the drawer sends is split the way source.set reads it", () => {
  const req = setRequest("cam1", { name: "Wide", color: "#fff", superimpose: "auto", uri: undefined });
  eq(req.id, "cam1");
  eq(req.name, "Wide");
  eq(req.color, "#fff");
  // Anything the request has no field for goes to params, which source.set
  // merges, rather than to the top level, where serde drops it in silence.
  eq(req.params.superimpose, "auto");
  ok(!("superimpose" in req), "it did not go out at the top level");
  ok(!("uri" in req), "an undefined value is left out");
});

// -------------------------------------------------- producer preview

test("the stream bit tells the armed scene from the mosaic", () => {
  const frame = (seq) => {
    const buf = new ArrayBuffer(HEADER_BYTES + 2);
    const view = new DataView(buf);
    view.setUint32(0, seq, true);
    view.setUint32(4, 99, true);
    new Uint8Array(buf).set([0xff, 0xd8], HEADER_BYTES);
    return parseFrame(buf);
  };
  const mosaic = frame(12);
  ok(!mosaic.preview, "a mosaic frame is what it always was");
  eq(mosaic.seq, 12);
  eq(mosaic.layout, 99);
  // The same counter with the top bit set is the preview, and the counter
  // still reads 12: the bit is the stream, not part of the number.
  const preview = frame((12 | PREVIEW_STREAM) >>> 0);
  ok(preview.preview, "the stream bit was not read");
  eq(preview.seq, 12);
});

test("a preview frame goes to the preview, never onto the mosaic tiles", () => {
  const client = new Client({ name: "test" }, new Store());
  const seen = [];
  client.on("preview-frame", () => seen.push("preview"));
  client.on("frame", () => seen.push("mosaic"));
  const pushed = [];
  client.sheet.push = (f) => pushed.push(["sheet", f]);
  client.preview.push = (f) => pushed.push(["picture", f]);
  client.handleFrame({ preview: true, seq: 1, layout: 0, jpeg: new Uint8Array([0xff]) });
  client.handleFrame({ preview: false, seq: 2, layout: 7, jpeg: new Uint8Array([0xff]) });
  eq(seen, ["preview", "mosaic"]);
  eq(pushed.map((p) => p[0]), ["picture", "sheet"]);
});

test("the preview is painted whole, with no layout to wait for", () => {
  const painter = new PicturePainter();
  const bitmap = document.createElement("canvas");
  bitmap.width = 2;
  bitmap.height = 2;
  const brush = bitmap.getContext("2d");
  brush.fillStyle = "#00ff00";
  brush.fillRect(0, 0, 2, 2);
  painter.bitmap = bitmap;
  const target = document.createElement("canvas");
  target.width = 2;
  target.height = 2;
  const detach = painter.attach(target);
  eq([...target.getContext("2d").getImageData(0, 0, 1, 1).data], [0, 255, 0, 255]);
  detach();
  ok(!painter.wanted, "letting go stops the painting");
  painter.destroy();
});

test("a snapshot does not take the armed scene away again", () => {
  // The status document has no field for the armed scene, and asking for the
  // preview stream re-subscribes, which brings a fresh snapshot with it. A
  // snapshot that cleared what was armed made the pane appear and go dark
  // again in the same second.
  const client = new Client({ name: "test", subscribe: () => Promise.resolve({}) }, new Store());
  client.handleEvent("preview.changed", { scene: "Two box" });
  client.handleEvent("snapshot", { state: { program: "cam1", sources: [] }, seq: 4 });
  eq(client.state.preview, "Two box");
  // Disarming still clears it, and a status that does name one still wins.
  client.handleEvent("preview.changed", { scene: null });
  eq(client.state.preview, null);
  client.handleEvent("snapshot", { state: { preview: "cam2", sources: [] }, seq: 5 });
  eq(client.state.preview, "cam2");
});

test("the pane beside the programme asks the core for ext.preview", () => {
  // `want` keys are `ext` keys: the armed scene is only composited while a
  // client has asked for it by name, so a pane that asked for the wrong one
  // would show black for ever.
  const client = new Client({ name: "test", subscribe: () => Promise.resolve({}) }, new Store());
  const want = client.want("preview", { fps: 8, width: 640 });
  eq(client.extSpec(), { preview: { fps: 8, width: 640 } });
  // Two askers are one subscription at the widest of them, as the mosaic is.
  const second = client.want("preview", { fps: 8, width: 960 });
  eq(client.extSpec().preview.width, 960);
  second.release();
  want.release();
  eq(client.extSpec(), {});
});

// ------------------------------------------------------- programme monitor

test("the monitor subscribes before the mosaic has told it which cell is the programme", () => {
  // Before anybody subscribes the core reports no cells at all, because the
  // mosaic only exists while something is subscribed. Waiting for the cell
  // before subscribing was waiting for ever whenever the source tiles were
  // showing icons, which is what the church preset ships.
  const cold = { multiview: { enabled: true, cols: 0, rows: 0, cells: [] } };
  ok(mosaicWanted(cold, true), "wanted with no cells reported yet");
  ok(!mosaicWanted(cold, false), "but not while the pane is scrolled out of view");
  ok(!mosaicWanted({ multiview: { enabled: false, cells: [] } }, true), "and not with multiview switched off");
});

// ---------------------------------------------------------------- scene tabs

/**
 * The tab strip in the Scenes panel, against a stubbed scene list.
 *
 * What is worth testing here is the gesture, not the markup. A tile takes on a
 * single click; a tab must not, or a strip under the operator's thumb cuts the
 * programme every time somebody looks at a scene. The only thing in this view
 * allowed to reach `program.take` is the button that says Take.
 */
async function sceneTabsSuite() {
  window.godwinmixPanels = window.godwinmixPanels || [];
  const { default: ScenesPanel } = await import("../panels/scenes/panel.js");
  const { focusedScene, setFocusedScene } = await import("../shell/focus.js");

  test("the focused scene is remembered, and a scene that went away is not", () => {
    setFocusedScene("wide");
    eq(focusedScene(), "wide");
    eq(focusedScene(["wide", "two-box"]), "wide");
    eq(focusedScene(["two-box"]), null, "an id that is not in the collection any more");
    setFocusedScene(null);
    eq(focusedScene(), null);
  });

  const summaries = [
    { id: "wide", name: "Wide", items: 2 },
    { id: "two-box", name: "Two box", items: 1 },
  ];
  const calls = [];
  const client = {
    state: { connected: true },
    call: (method, params) => {
      calls.push({ method, params });
      return Promise.resolve({});
    },
    on: () => () => {},
    onRender: () => () => {},
  };
  const panel = new ScenesPanel();
  panel.setClient(client);
  // The list `scene.list` would have answered with, filed by hand so the panel
  // has scenes without a core behind it.
  panel.scenes.summaries = summaries;
  panel.scenes.start = () => Promise.resolve(summaries);
  panel.scenes.refresh = () => Promise.resolve(summaries);
  document.body.appendChild(panel);
  panel.setView("tabs");

  test("a tab per scene, with its item count, and the focus seeded from the first", () => {
    eq(panel.tabs.size, 2);
    eq(panel.tabs.get("two-box").textContent, "Two box1", "the name and then the count");
    eq(focusedScene(), "wide", "nothing was remembered, so the first scene is the one in hand");
  });

  test("a click on a tab moves the focus and puts nothing on air", () => {
    calls.length = 0;
    panel.tabs.get("two-box").click();
    eq(focusedScene(), "two-box");
    ok(!calls.some((c) => c.method === "program.take"), `it called ${JSON.stringify(calls.map((c) => c.method))}`);
    ok(panel.tabs.get("two-box").classList.contains("on"), "the focused tab reads as the one in hand");
  });

  test("a double click on a tab opens the composer on that scene", () => {
    const opened = [];
    panel.open = (id) => opened.push(id);
    panel.tabs.get("wide").dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    eq(opened, ["wide"]);
  });

  test("the Take button puts the focused scene on air", () => {
    calls.length = 0;
    setFocusedScene("two-box");
    panel.take.click();
    eq(calls.filter((c) => c.method === "program.take").map((c) => c.params.scene), ["two-box"]);
  });

  panel.remove();
  setFocusedScene(null);
}

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

  const audio = new AudioGestures(client);
  await audio.setMuted("t-bars", true);
  await waitFor(() => client.store.source("t-bars").muted, 3000, "mute status");
  await audio.setMuted("t-bars", false);
  await waitFor(() => !client.store.source("t-bars").muted, 3000, "unmute status");
  test("mute and unmute work against the real API", () => ok(!client.store.source("t-bars").muted));
  await client.call("program.take", { source: "t-bars" });
  const switchAt = performance.now();
  await client.call("program.take", { source: "t-ball" });
  test("the manual mixer accepts an immediate source switch", () => ok(performance.now() - switchAt < 1000));

  window.godwinmixPanels = window.godwinmixPanels || [];
  const { default: ScenesPanel } = await import("../panels/scenes/panel.js");
  const panel = new ScenesPanel();
  panel.setClient(client);
  // A real size, because a marquee over a zero height grid selects nothing.
  panel.style.cssText = "display:block;width:900px;height:320px";
  document.body.appendChild(panel);
  panel.connectedCallback();
  // Everything below is the tile grammar: the marquee, the tile names, F2. The
  // panel opens on the tab strip now, so ask it for the grid.
  panel.setView("tiles");
  await waitFor(() => panel.scenes.supported !== undefined && panel.scenes.summaries !== null, 5000, "scene.list");

  if (!panel.scenes.supported) {
    line("ok", "skipped the live suite: this core has no scene server");
    panel.remove();
    return;
  }

  const addCheck = await panel.scenes.add("Add source check");
  await panel.addSources(addCheck.id, [sources[0]]);
  const addedView = await client.call("scene.get", { scene: addCheck.id });
  test("dropping a source into an existing scene uses the content schema", () => {
    eq(addedView.records.filter((record) => record.kind === "item").length, 1);
  });
  const { openSceneSources } = await import("../panels/sources/chooser.js");
  const choose = openSceneSources(client, panel.scenes, panel.scenes.summary(addCheck.id));
  const nextSource = client.state.sources.find(source => source.id === sources[1]);
  const { nameOf } = await import("../panels/sources/local.js");
  const addButton = choose.el.querySelector(`[aria-label="Add ${nameOf(nextSource)}"]`);
  addButton.click();
  await waitFor(() => panel.scenes.mirror.items(addCheck.id).length === 2, 3000, "the chooser to add an existing source");
  test("the scene chooser reuses an existing source through scene.item.add", () => {
    eq(panel.scenes.mirror.items(addCheck.id).length, 2);
    eq(panel.scenes.summary(addCheck.id).sources.slice().sort(), sources.slice().sort());
    ok(choose.el.querySelector(`[aria-label="Already in scene: ${nameOf(nextSource)}"]`).disabled);
  });
  choose.close();
  const { default: SourcesPanel } = await import("../panels/sources/panel.js");
  const { settings: sourceSettings, setSetting: setSourceSetting } = await import("../shell/settings.js");
  const confirmBefore = sourceSettings().confirmRemove;
  setSourceSetting("confirmRemove", false);
  try {
    await SourcesPanel.prototype.remove.call({ scopedTo: () => panel.scenes.summary(addCheck.id), sceneClient: () => panel.scenes }, [sources[1]]);
  } finally { setSourceSetting("confirmRemove", confirmBefore); }
  const retainedSource = await client.call("source.get", { id: sources[1] });
  test("removing a scene source preserves the reusable mixer source", () => {
    eq(panel.scenes.mirror.items(addCheck.id).length, 1);
    eq(panel.scenes.summary(addCheck.id).sources, [sources[0]]);
    eq(retainedSource.id, sources[1]);
  });
  await panel.scenes.remove(addCheck.id);

  const before = panel.scenes.scenes().length;

  // --- two tiles dragged onto empty space make a two box scene --------------

  window.dispatchEvent(
    new CustomEvent("gmx:tiles-dropped", {
      detail: { ids: sources, target: "scenes:empty", from: "sources", copy: false },
    })
  );
  await waitFor(() => panel.scenes.scenes().length > before, 8000, "the new scene");
  // The list changes first and the panel repaints a tick later, on the kit's
  // own settle timer, so wait for the tiles as well as for the summaries.
  await waitFor(() => panel.tiles.size === panel.scenes.scenes().length, 8000, "the tiles to catch up");
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
  for (const id of made) await client.call("source.remove", { id }).catch(() => {});
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
  .then(() => studioTests(test, eq, ok))
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
  .then(addSourcePickerSuite)
  .catch((e) => {
    failed += 1;
    line("fail", "the add source picker suite threw: " + e.message);
    console.error(e);
  })
  .then(scopedSourcesSuite)
  .then(() => sourceChooserTests(test, eq, ok))
  .catch((e) => {
    failed += 1;
    line("fail", "the scoped sources suite threw: " + e.message);
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
  .then(sceneTabsSuite)
  .catch((e) => {
    failed += 1;
    line("fail", "the scene tab suite threw: " + e.message);
    console.error(e);
  })
  .then(liveSuite)
  .catch((e) => {
    failed += 1;
    line("fail", "the live suite threw: " + e.message);
    console.error(e);
  })
  .then(summarise);
