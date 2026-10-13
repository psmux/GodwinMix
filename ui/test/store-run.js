// The Plugins page suites on their own, against the stub, with no core:
// /test/store.html. The same two suites run inside the full harness, run.js.
// Writes "pass" or "fail" into #out's data-result for dev/browser-run.mjs.

import { pluginGalleryTests } from "./plugins.js";
import { storeTests } from "./store.js";

const out = document.getElementById("out");
let passed = 0;
let failed = 0;

function line(good, text) {
  const row = document.createElement("div");
  row.textContent = `${good ? "ok  " : "FAIL"}  ${text}`;
  out.append(row);
  console[good ? "log" : "error"](row.textContent);
}

function test(name, fn) {
  try {
    fn();
    passed += 1;
    line(true, name);
  } catch (e) {
    failed += 1;
    line(false, `${name}: ${e.message}`);
  }
}

function eq(a, b, what) {
  const x = JSON.stringify(a);
  const y = JSON.stringify(b);
  if (x !== y) throw new Error(`${what || "value"}: expected ${y}, got ${x}`);
}

function ok(v, what) {
  if (!v) throw new Error(what || "expected something truthy");
}

async function main() {
  for (const suite of [pluginGalleryTests, storeTests]) {
    try {
      await suite(test, eq, ok);
    } catch (e) {
      failed += 1;
      line(false, `the suite threw: ${e.message}`);
    }
  }
  line(!failed, `${passed} passed, ${failed} failed`);
  out.dataset.result = failed ? "fail" : "pass";
}

main();
