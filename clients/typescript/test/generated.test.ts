// The drift test: src/generated/protocol.ts against protocol.json.
//
// A method added to the core changes protocol.json, and this fails until
// someone runs `python3 clients/gen/generate.py` and commits what changed. That
// is the whole mechanism by which a core method reaches this library.

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, it } from "node:test";

import { API_COMPATIBLE, API_LEVEL, EVENT_NAMES, EXT_KEYS, METHODS } from "../src/index.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const inRepo = existsSync(ROOT + "protocol.json");

describe("generated from protocol.json", { skip: inRepo ? false : "installed without the repository" }, () => {
  it("is what the generator would write today", () => {
    try {
      execFileSync("python3", [ROOT + "clients/gen/generate.py", "--check", "--lang", "ts"], {
        cwd: ROOT,
        stdio: "pipe",
      });
    } catch (e) {
      const out = e as { stdout?: Buffer; stderr?: Buffer };
      assert.fail(
        "src/generated/protocol.ts is stale. Run `python3 clients/gen/generate.py` and commit the result.\n" +
          String(out.stdout || "") +
          String(out.stderr || ""),
      );
    }
  });

  it("carries every method, event and ext key the contract names", () => {
    const doc = JSON.parse(readFileSync(ROOT + "protocol.json", "utf8"));
    assert.equal(API_LEVEL, doc.api_level);
    assert.equal(API_COMPATIBLE, doc.api_compatible);

    const names = new Set(METHODS.map((m) => m.name as string));
    for (const method of doc.methods) assert.ok(names.has(method.name), `${method.name} is missing`);
    assert.equal(names.size, doc.methods.length);

    const events = new Set<string>(EVENT_NAMES as readonly string[]);
    for (const event of doc.events) assert.ok(events.has(event.pattern), `${event.name} is missing`);

    for (const ext of doc.ext) {
      assert.ok(EXT_KEYS[ext.key], `ext.${ext.key} is missing`);
      assert.equal(EXT_KEYS[ext.key]!.implemented, ext.implemented);
    }
  });

  it("knows which methods change the mixer, so a surface can guard them", () => {
    const take = METHODS.find((m) => m.name === "program.take")!;
    assert.equal(take.mutating, true);
    assert.equal(take.scope, "operate");
    assert.deepEqual(take.rest, { method: "POST", path: "/api/v1/program/take" });

    const list = METHODS.find((m) => m.name === "source.list")!;
    assert.equal(list.mutating, false);
    assert.equal(list.scope, "read");
  });
});
