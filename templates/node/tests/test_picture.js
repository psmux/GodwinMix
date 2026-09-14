#!/usr/bin/env node
/* The failing test. It is meant to fail until you replace it.
 *
 * `./check` runs it last. It fails on a fresh template on purpose, because a
 * template that passes out of the box teaches nothing and a green tick on an
 * unfinished plugin is a lie.
 *
 * Replace the body with a check of the picture your plugin actually draws. Two
 * that are worth writing:
 *
 *   * the frame is the right size for the canvas (the one below, keep it)
 *   * one pixel you can predict has the value you expect (the one below, change it)
 *
 * Delete DELIBERATELY_FAILING when you have.
 */
"use strict";

const assert = require("node:assert");
const main = require("../main.js");

const DELIBERATELY_FAILING = true;

const CANVAS = { width: 160, height: 90, fps: 30 };

const tests = {
  "the frame is the right size": () => {
    const frame = main.draw(CANVAS, { bars: 8 }, 0);
    const expected = (CANVAS.width * CANVAS.height * 3) / 2;
    assert.strictEqual(frame.length, expected,
      "draw() returned " + frame.length + " bytes, an I420 frame at "
      + CANVAS.width + "x" + CANVAS.height + " is " + expected);
  },

  "the first pixel is what you meant": () => {
    const frame = main.draw(CANVAS, { bars: 8 }, 0);
    assert.strictEqual(frame[0], 235,
      "the top left luma is " + frame[0] + ", not the white bar's 235");
  },

  // vint is the one place JavaScript's 32 bit bitwise operators would bite, so
  // the sizes a real canvas produces are checked here. Keep this one.
  "vint holds a 1080p frame": () => {
    assert.deepStrictEqual([...main.vint(127)], [0x80 | 127]);
    assert.deepStrictEqual([...main.vint(1)], [0x81]);
    assert.deepStrictEqual([...main.vint(3110404)], [0x20 | 0x2f, 0x72, 0x44]);
  },
};

function run() {
  const failures = [];
  for (const [name, test] of Object.entries(tests)) {
    try {
      test();
    } catch (err) {
      failures.push(name + ": " + (err.message || err));
    }
  }
  if (DELIBERATELY_FAILING) {
    failures.push("test_picture.js has not been written yet. Replace the tests "
      + "with checks of your own picture, then set DELIBERATELY_FAILING = false.");
  }
  if (failures.length > 0) {
    console.error("FAIL: tests/test_picture.js");
    for (const f of failures) console.error("  " + f);
    return 1;
  }
  console.error("ok: the picture is what you meant");
  return 0;
}

process.exitCode = run();
