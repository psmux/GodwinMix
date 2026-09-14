#!/usr/bin/env node
/* Replay a recorded transcript against the plugin, with no core running.
 *
 * Bytes in, bytes out. It spawns main.js, writes the `core` lines to its stdin
 * in order, and checks that each `plugin` line turns up on stderr, as a subset.
 * Lines the transcript does not mention (log notifications, media reports) are
 * skipped rather than failed, so adding a log line does not break the test.
 *
 * `gmx plugin test --offline` will do this and more once gmx is installed. This
 * script is here so the template's check runs on a bare machine today.
 *
 *     node tests/replay.js tests/transcript.jsonl
 */
"use strict";

const fs = require("node:fs");
const path = require("node:path");
const readline = require("node:readline");
const { spawn } = require("node:child_process");

const TIMEOUT_MS = 10000;
const EXIT_MS = 8000;

/** Does `actual` contain everything `expected` asks for? "*" matches any. */
function matches(expected, actual) {
  if (expected === "*") return true;
  if (Array.isArray(expected)) {
    return Array.isArray(actual) && expected.length === actual.length
      && expected.every((e, i) => matches(e, actual[i]));
  }
  if (expected !== null && typeof expected === "object") {
    if (actual === null || typeof actual !== "object" || Array.isArray(actual)) {
      return false;
    }
    return Object.keys(expected).every(
      (key) => key in actual && matches(expected[key], actual[key]));
  }
  return expected === actual;
}

function readSteps(file) {
  const steps = [];
  const raw = fs.readFileSync(file, "utf8").split("\n");
  raw.forEach((text, index) => {
    const line = text.trim();
    if (!line || line.startsWith("#") || line.startsWith("//")) return;
    const step = JSON.parse(line);
    const keys = Object.keys(step);
    if (keys.length !== 1 || (keys[0] !== "core" && keys[0] !== "plugin")) {
      throw new Error("line " + (index + 1)
        + ": a step has one key, 'core' or 'plugin'.");
    }
    steps.push([index + 1, step]);
  });
  return steps;
}

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

async function main() {
  const here = path.dirname(path.dirname(path.resolve(__filename)));
  const transcript = process.argv[2] || path.join(here, "tests/transcript.jsonl");
  const steps = readSteps(transcript);

  const plugin = spawn(process.execPath, [path.join(here, "main.js")], {
    cwd: here,
    stdio: ["pipe", "ignore", "pipe"],
    env: Object.assign({}, process.env, {
      GMX_PLUGIN: "{{name}}", GMX_PROVIDE: "source", GMX_INSTANCE: "test",
      GMX_API_LEVEL: "1", GMX_PLUGIN_ROOT: here,
    }),
  });

  // stderr is read as it arrives so a plugin that says nothing cannot wedge the
  // writer, and so the timeout is real.
  const lines = [];
  let ended = false;
  let exitCode = null;
  readline.createInterface({ input: plugin.stderr })
    .on("line", (line) => lines.push(line.trim()))
    .on("close", () => { ended = true; });
  plugin.on("exit", (code) => { exitCode = code; });
  plugin.stdin.on("error", () => { /* a closed pipe is reported by the step */ });

  let readTo = 0;
  const failures = [];
  for (const [number, step] of steps) {
    if ("core" in step) {
      if (!plugin.stdin.writable) {
        failures.push("line " + number + ": the plugin closed stdin early");
        break;
      }
      plugin.stdin.write(JSON.stringify(step.core) + "\n");
      continue;
    }
    const expected = step.plugin;
    const deadline = Date.now() + TIMEOUT_MS;
    let found = false;
    while (!found && Date.now() < deadline) {
      while (readTo < lines.length) {
        const raw = lines[readTo++];
        if (!raw) continue;
        let actual;
        try {
          actual = JSON.parse(raw);
        } catch (err) {
          continue;           // a non JSON line is a log line, not an error
        }
        if (matches(expected, actual)) {
          found = true;
          break;
        }
      }
      if (found) break;
      if (ended && readTo >= lines.length) break;
      await sleep(20);
    }
    if (!found) {
      failures.push("line " + number + ": never saw a line matching "
        + JSON.stringify(expected));
      break;
    }
  }

  plugin.stdin.end();
  const gone = Date.now() + EXIT_MS;
  while (exitCode === null && Date.now() < gone) await sleep(20);
  if (exitCode === null) {
    plugin.kill("SIGKILL");
    failures.push("the plugin did not exit within 8 seconds of shutdown");
  }

  if (failures.length > 0) {
    console.error("FAIL: offline transcript");
    for (const f of failures) console.error("  " + f);
    console.error("  what the plugin actually said:");
    for (const raw of lines.slice(0, 40)) console.error("    " + raw.slice(0, 200));
    return 1;
  }
  console.error("ok: transcript replayed, " + steps.length
    + " steps, plugin exited " + exitCode);
  return 0;
}

main().then((code) => { process.exitCode = code; });
