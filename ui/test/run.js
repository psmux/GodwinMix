// The test runner: forty lines, no dependencies, no toolchain.
//
// Everything testable without a mixer is tested here: the selection rules, the
// meter scale, the fader curve, the frame header, the store, the schema form,
// the ext union, the palette ranking and the legacy error unwrapping. Open the
// page, read the console, or read the list.

import { Selection, overlaps, rectFrom } from "../shell/selection.js";
import { dbToPos, FLOOR } from "../shell/meter.js";
import { posToGain, gainToPos, gainLabel, UNITY } from "../shell/fader.js";
import { parseFrame, sheetWidthFor, HEADER_BYTES } from "../client/frames.js";
import { Store } from "../client/store.js";
import { SchemaForm } from "../client/schema-form.js";
import { Client } from "../client/index.js";
import { RpcError, CODES } from "../client/errors.js";
import { rank } from "../shell/palette.js";
import { chordOf, DEFAULT_MAP } from "../shell/keymap.js";
import { kindOfUri } from "../client/kinds.js";
import { tagFor } from "../shell/registry.js";
import * as layout from "../shell/layout.js";

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
  row.appendChild(document.createTextNode(text));
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
  ok(frame.runningTimeNs === 1234567890n);
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
  const form = new SchemaForm(
    {
      type: "object",
      properties: { kind: { type: "string", enum: ["file", "page"], default: "file" } },
      allOf: [
        {
          if: { properties: { kind: { const: "page" } } },
          then: { properties: { superimpose: { type: "string", enum: ["off", "auto"], default: "off" } } },
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
  eq(chordOf({ key: "k", ctrlKey: true, metaKey: false, altKey: false, shiftKey: false }), "Ctrl+K");
  eq(chordOf({ key: "F2", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false }), "F2");
  eq(chordOf({ key: "1", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false }), "1");
});

test("the map is commands, never indices into a source list", () => {
  for (const value of Object.values(DEFAULT_MAP)) ok(typeof value === "string" && value.includes("."), `${value} is a command id`);
  eq(DEFAULT_MAP["1"], "tray.take-slot");
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

// ---------------------------------------------------------------- summary

const summary = `${passed} passed, ${failed} failed`;
line(failed ? "fail" : "ok", summary);
document.title = (failed ? "FAIL " : "ok ") + summary;
if (out) out.dataset.result = failed ? "fail" : "pass";
console.log(failed ? `FAILED: ${summary}` : `ALL PASSED: ${summary}`);
