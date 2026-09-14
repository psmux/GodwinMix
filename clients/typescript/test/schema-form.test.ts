// The schema reader: a plugin's settings, without the UI knowing the plugin.

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { applyConditions, describeForm, missing, readForm } from "../src/index.ts";

/** The shape a plugin's `plugin.describe` answers with. */
const SCHEMA = {
  type: "object",
  title: "RTMP output",
  required: ["url"],
  properties: {
    url: { type: "string", format: "uri", title: "Destination", examples: ["rtmp://live/app"] },
    key: { type: "string", format: "secret", title: "Stream key" },
    codec: { type: "string", enum: ["h264", "hevc"], default: "h264" },
    bitrate: {
      type: "integer",
      minimum: 500,
      maximum: 20000,
      default: 4500,
      "x-gmx-unit": "kbit/s",
      "x-gmx-group": "Advanced",
    },
    keyframes: { type: "number", multipleOf: 0.5, "x-gmx-group": "Advanced" },
    hardware: { type: "boolean", default: false },
    hosts: { type: "array", items: { type: "string" } },
    extra: { type: "object" },
  },
  allOf: [
    {
      if: { properties: { codec: { const: "hevc" } } },
      then: { properties: { keyframes: {} } },
    },
  ],
};

describe("describeForm", () => {
  it("gives every field a control kind, a group and a unit", () => {
    const form = describeForm(SCHEMA, { url: "rtmp://live/app" });
    const byName = Object.fromEntries(form.fields.map((f) => [f.name, f]));

    assert.equal(byName.url!.kind, "url");
    assert.equal(byName.url!.required, true);
    assert.equal(byName.url!.placeholder, "rtmp://live/app");
    assert.equal(byName.key!.kind, "secret");
    assert.equal(byName.codec!.kind, "choice");
    assert.deepEqual(byName.codec!.choices?.map((c) => c.value), ["h264", "hevc"]);
    assert.equal(byName.bitrate!.kind, "integer");
    assert.equal(byName.bitrate!.unit, "kbit/s");
    assert.equal(byName.bitrate!.group, "Advanced");
    assert.equal(byName.bitrate!.min, 500);
    assert.equal(byName.hardware!.kind, "boolean");
    assert.equal(byName.hosts!.kind, "lines");
    assert.equal(byName.extra!.kind, "json");
    assert.deepEqual(form.groups, ["", "Advanced"]);
  });

  it("takes the default when the settings have no value", () => {
    const form = describeForm(SCHEMA, {});
    const bitrate = form.fields.find((f) => f.name === "bitrate")!;
    assert.equal(bitrate.value, 4500);
  });

  it("hides what an if/then says does not apply, and shows it when it does", () => {
    const form = describeForm(SCHEMA, { codec: "h264" });
    const keyframes = () => form.fields.find((f) => f.name === "keyframes")!;
    assert.equal(keyframes().visible, false);

    applyConditions(form, { codec: "hevc" });
    assert.equal(keyframes().visible, true);
  });
});

describe("readForm", () => {
  it("coerces what a text control hands back", () => {
    const form = describeForm(SCHEMA, {});
    const out = readForm(form, {
      url: "rtmp://live/app",
      bitrate: "6000",
      keyframes: "2.5",
      hardware: true,
      hosts: "a.example\nb.example\n",
      extra: '{"x": 1}',
      codec: "hevc",
    });
    assert.equal(out.bitrate, 6000);
    assert.deepEqual(out.hosts, ["a.example", "b.example"]);
    assert.deepEqual(out.extra, { x: 1 });
    assert.equal(out.hardware, true);
  });

  it("leaves out a secret the operator did not retype, rather than blanking it", () => {
    const form = describeForm(SCHEMA, { url: "rtmp://live/app", key: "already-set" });
    const untouched = readForm(form, { url: "rtmp://live/app", key: "••••••••" });
    assert.equal("key" in untouched, false);

    const typed = readForm(form, { url: "rtmp://live/app", key: "new-key" }, new Set(["key"]));
    assert.equal(typed.key, "new-key");
  });

  it("leaves out a field an if/then is hiding", () => {
    const form = describeForm(SCHEMA, { codec: "h264" });
    const out = readForm(form, { url: "rtmp://x", codec: "h264", keyframes: "2" });
    assert.equal("keyframes" in out, false);
  });

  it("names the required fields that are still empty", () => {
    const form = describeForm(SCHEMA, {});
    assert.deepEqual(missing(form, {}), ["url"]);
    assert.deepEqual(missing(form, { url: "rtmp://x" }), []);
  });
});
