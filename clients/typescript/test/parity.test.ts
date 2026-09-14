// This package and the browser client in ui/client, over the same session.
//
// The reference web UI has its own copy of the protocol code, written first and
// in plain JavaScript against browser globals it can rely on. This package is
// the same design, typed, with the browser-only parts (SheetPainter, the legacy
// transport, localStorage, XHR uploads) left out and the parts a non browser
// surface needs (MJPEG, WHEP, a form description rather than a form) added.
//
// Two copies of anything drift. These tests are the guard: the error table, the
// store's behaviour over a scripted session, and the frame header are checked
// against ui/client's own files, so a change to either side that moves them
// apart fails here. They skip themselves when the repository is not around,
// which is the case when this package is installed from npm.
//
// The designer kits are guarded the same way, in kits.test.ts rather than here:
// they are driven from ui/kits/fixtures.json, and the case runners that replay
// that file are what the TypeScript and browser kits are compared over, so the
// cross check sits beside them instead of being written out a second time.

import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, it } from "node:test";

import { CODES, parseFrame, Store } from "../src/index.ts";

const UI = fileURLToPath(new URL("../../../ui/client/", import.meta.url));
const present = existsSync(UI + "store.js");

describe("parity with ui/client", { skip: present ? false : "no ui/client in this tree" }, () => {
  it("has the same error codes", async () => {
    const browser = await import(UI + "errors.js");
    assert.deepEqual(browser.CODES, CODES);
  });

  it("reads a refusal the same way", async () => {
    const browser = await import(UI + "errors.js");
    const message = "cam1 is connecting. Wait for it to go live, then take it.";
    const theirs = new browser.RpcError(CODES.WRONG_STATE, message, {});
    const { RpcError } = await import("../src/errors.ts");
    const ours = new RpcError(CODES.WRONG_STATE, message, {});
    assert.equal(ours.title, theirs.title);
    assert.equal(ours.nextStep, theirs.nextStep);
    assert.equal(ours.retryable, theirs.retryable);
  });

  it("ends a scripted session in the same state", async () => {
    const browser = await import(UI + "store.js");
    const theirs = new browser.Store();
    const ours = new Store();

    const status = {
      program: "cam1",
      sources: [
        { id: "cam1", name: "One", uri: "rtmp://a", state: "live", has_video: true, has_audio: true },
        { id: "cam2", name: "Two", uri: "rtmp://b", state: "connecting", has_video: true, has_audio: true },
      ],
      outputs: [{ id: "twitch", uri_host: "live", state: "live", reconnects: 0, queue_secs: 0 }],
      running_time_ms: 1500,
      uptime_secs: 9,
    };

    for (const store of [theirs, ours]) {
      store.snapshot(status, 42);
      store.patchSource("cam2", { state: "live" });
      store.patchOutput("twitch", { state: "reconnecting", reconnects: 1 });
      store.patch({ program: "cam2" });
      store.setMeters([-12, -12], { cam2: [-18] });
      store.addAlert({ severity: "warning", message: "queue is filling" });
      store.patch({ seq: 44 });
      store.flush();
    }

    assert.equal(ours.state.program, theirs.state.program);
    assert.equal(ours.state.seq, theirs.state.seq);
    assert.deepEqual(ours.state.sources, theirs.state.sources);
    assert.deepEqual(ours.state.outputs, theirs.state.outputs);
    assert.deepEqual(ours.state.meters, theirs.state.meters);
    assert.equal(ours.tallyOf("cam2"), theirs.tallyOf("cam2"));
    assert.equal(ours.tallyOf("cam1"), theirs.tallyOf("cam1"));
    assert.equal(ours.state.alerts.length, theirs.state.alerts.length);
  });

  it("reads the same numbers out of a frame header", async () => {
    const browser = await import(UI + "frames.js");
    const bytes = new Uint8Array(20);
    const view = new DataView(bytes.buffer);
    view.setUint32(0, 9, true);
    view.setUint32(4, 3, true);
    view.setBigUint64(8, 1234n, true);
    bytes.set([0xff, 0xd8, 0xff, 0xd9], 16);

    const theirs = browser.parseFrame(bytes);
    const ours = parseFrame(bytes);
    assert.equal(ours!.seq, theirs.seq);
    assert.equal(ours!.layout, theirs.layout);
    // The one deliberate difference. The core writes milliseconds at offset 8
    // (api::rpc::frame_header), ui/client now calls the field runningTimeMs too and
    // holds it as a BigInt. This package names it runningTimeMs and hands over
    // a Number, because that is what the bytes are. Same value either way.
    assert.equal(BigInt(ours!.runningTimeMs), theirs.runningTimeMs);
    assert.deepEqual([...ours!.jpeg], [...theirs.jpeg]);
  });

  it("asks the core for the same mosaic width", async () => {
    const browser = await import(UI + "frames.js");
    const { sheetWidthFor } = await import("../src/frames.ts");
    for (const [css, cols, dpr] of [
      [160, 3, 1],
      [220, 4, 2],
      [64, 1, 1],
      [900, 3, 2],
    ] as Array<[number, number, number]>) {
      assert.equal(sheetWidthFor(css, cols, dpr), browser.sheetWidthFor(css, cols, dpr));
    }
  });
});
