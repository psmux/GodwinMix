// Mixer Settings against a stub schema and a stub client: the form the schema
// kit draws from it, which keys a Save sends, and where a refusal lands.

import { layoutFromSchema, valuesFrom, changedKeys, refusalField, statusFrom, pendingFrom, APPLIES } from "../shell/mixer-form.js";
import { openMixerSettings } from "../shell/mixer-settings.js";
import { SchemaInspector } from "../kits/schema/index.js";
import { RpcError } from "../client/errors.js";

const SCHEMA = {
  type: "object",
  properties: {
    "safety.min_hold_ms": { type: "integer", title: "Minimum hold", description: "Shortest time on air.", default: 0, minimum: 0, maximum: 60000, "x-gmx-unit": "ms", "x-gmx-group": "safety", "x-gmx-applies": "live" },
    "canvas.width": { type: "integer", title: "Canvas width", default: 1920, minimum: 16, maximum: 7680, "x-gmx-unit": "px", "x-gmx-group": "canvas", "x-gmx-applies": "restart" },
    "program.video_bitrate_kbps": { type: "integer", title: "Video bitrate", default: 6000, minimum: 100, maximum: 100000, "x-gmx-unit": "kbit/s", "x-gmx-group": "program", "x-gmx-applies": "restart" },
    "hardware.encode": { type: "string", title: "Hardware encode", enum: ["auto", "off"], default: "auto", "x-gmx-group": "hardware" },
    "control.token": { type: "string", format: "secret", title: "Control token", "x-gmx-group": "control" },
    "zeta.thing": { type: "boolean", title: "Something new", default: false },
  },
};

const GOT = {
  path: "/srv/show/godwinmix.toml",
  needs_restart: ["canvas.width"],
  keys: [
    { key: "safety.min_hold_ms", value: 0, source: "default", applies: "live", secret: false },
    { key: "canvas.width", value: 1280, source: "file", applies: "restart", secret: false, pending: true },
    { key: "program.video_bitrate_kbps", value: 6000, source: "default", applies: "restart", secret: false },
    { key: "hardware.encode", value: "auto", source: "default", applies: "restart", secret: false },
    { key: "control.token", value: null, source: "file", applies: "restart", secret: true, set: true },
    { key: "zeta.thing", value: false, source: "default", applies: "restart", secret: false },
  ],
};

const tick = () => new Promise((r) => setTimeout(r, 0));

export async function mixerSettingsTests(test, eq, ok) {
  test("the mixer form groups keys by section, known sections first in their order", () => {
    const ui = layoutFromSchema(SCHEMA);
    eq(ui.elements.map((g) => g.label).join(","), "Picture,Programme stream,Hardware,This mixer,Safety,Zeta");
    eq(ui.elements[0].elements[0].scope, "canvas.width");
  });

  test("the schema kit draws the mixer form with titles, units, ranges and a secret box", () => {
    const inspector = new SchemaInspector({ schema: SCHEMA, ui: layoutFromSchema(SCHEMA), value: valuesFrom(GOT) });
    const text = inspector.el.textContent;
    ok(text.includes("Canvas width") && text.includes("kbit/s") && text.includes("Shortest time on air."), text);
    const width = [...inspector.el.querySelectorAll("input[type=number]")].find((i) => i.value === "1280");
    ok(width && width.max === "7680", "the width box carries the file's value and the maximum");
    eq(inspector.el.querySelectorAll("input[type=password]").length, 1, "the token is a password box");
    eq(inspector.el.querySelector("input[type=password]").value, "", "a secret is never filled in");
  });

  test("Save sends only the keys that moved, never an untouched secret", () => {
    const before = valuesFrom(GOT);
    eq(JSON.stringify(changedKeys(before, Object.assign({}, before, { "canvas.width": 1920 }))), '{"canvas.width":1920}');
    eq(Object.keys(changedKeys(before, before)).length, 0);
  });

  test("a refusal belongs to the field its data names, or to none", () => {
    const keys = Object.keys(SCHEMA.properties);
    eq(refusalField(new RpcError(-32602, "too big", { key: "canvas.width" }), keys), "canvas.width");
    eq(refusalField(new RpcError(-32602, "would not load", { keys: ["nope", "safety.min_hold_ms"] }), keys), "safety.min_hold_ms");
    eq(refusalField(new RpcError(-32001, "no file", { path: "/x" }), keys), null);
  });

  test("each key's answer reads as when it takes effect", () => {
    const s = statusFrom({ applied: ["safety.min_hold_ms"], next_source: ["browser.args"], needs_restart: ["canvas.width"], changed: [] });
    eq(s["safety.min_hold_ms"], APPLIES.live);
    eq(s["browser.args"], APPLIES.next_source);
    eq(s["canvas.width"], APPLIES.restart);
    eq(pendingFrom(GOT)["canvas.width"], APPLIES.restart);
  });

  const calls = [];
  const client = {
    call: async (method, params) => {
      calls.push([method, params]);
      if (method === "config.schema") return SCHEMA;
      if (method === "config.get") return GOT;
      if (method === "config.set" && params.values["program.video_bitrate_kbps"] > 100000) {
        throw new RpcError(-32602, "program.video_bitrate_kbps is 200000, above 100000. Use 100 to 100000.", { key: "program.video_bitrate_kbps", minimum: 100, maximum: 100000 });
      }
      if (method === "config.set") return { dry_run: false, path: GOT.path, changed: [], unchanged: [], applied: ["safety.min_hold_ms"], next_source: [], needs_restart: ["canvas.width"] };
      throw new Error("unexpected " + method);
    },
  };
  const m = await openMixerSettings(client);
  const field = (title) => [...m.el.querySelectorAll(".field")].find((f) => f.querySelector(".lbl")?.textContent.startsWith(title));
  const type = (title, value) => {
    const input = field(title).querySelector("input");
    input.value = String(value);
    input.dispatchEvent(new Event("input"));
  };
  const save = () => [...m.el.querySelectorAll("footer button")].find((b) => b.textContent === "Save").click();
  try {
    test("a key waiting for a restart says so when the dialog opens", () => {
      ok(field("Canvas width").textContent.includes(APPLIES.restart), field("Canvas width").textContent);
      ok(m.el.textContent.includes("One change is saved and waits for the mixer to restart."));
    });
    type("Video bitrate", 200000);
    save();
    await tick();
    await tick();
    test("the refusal is shown under the field it names and nothing else is marked", () => {
      const err = field("Video bitrate").querySelector(".field-error");
      ok(!err.hidden && err.textContent.includes("above 100000"), err.textContent);
      eq([...m.el.querySelectorAll(".field-error")].filter((e) => !e.hidden).length, 1);
    });
    type("Video bitrate", 6000);
    type("Minimum hold", 1500);
    save();
    await tick();
    await tick();
    test("a Save after a fix sends only the changed key and says it is in force", () => {
      const sent = calls.filter(([method]) => method === "config.set").pop()[1].values;
      eq(JSON.stringify(sent), '{"safety.min_hold_ms":1500}');
      ok(field("Minimum hold").textContent.includes(APPLIES.live));
      ok(field("Video bitrate").querySelector(".field-error").hidden, "the old refusal is cleared");
    });
  } finally {
    m.close();
  }
}
