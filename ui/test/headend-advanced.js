// Headend mode's settings on the page, against the stubs: the Custom form's
// Advanced group, an HLS output's settings when it is added and changed,
// a governor refusal kept in a direct show's detail, and the Flussonic
// import with its report first. No station needed.

import { wallStub } from "./wall-stub.js";
import { RpcError } from "../client/errors.js";
import { wait, until, sent, button, closeDialogs, dialogs } from "./wall-help.js";
import { refusal } from "./renditions-stub.js";

/** The schema kit's field with this label, and its input or select. */
const field = (root, label) => [...root.querySelectorAll(".field")].find((f) => f.querySelector(".lbl") && f.querySelector(".lbl").firstChild.textContent === label);
const put = (root, label, value) => {
  const input = field(root, label).querySelector("select, input");
  if (input.type === "checkbox") input.checked = !!value;
  else input.value = String(value);
  input.dispatchEvent(new Event(input.tagName === "SELECT" || input.type === "checkbox" ? "change" : "input", { bubbles: true }));
};
const advanced = (root) => [...root.querySelectorAll("details")].find((d) => d.querySelector("summary").textContent === "Advanced");

/** A Flussonic transcoder line as the import makes it. */
const IMPORTED = {
  video: { codec: "h264", bitrate_kbps: 1500, width: 1280, height: 720, fit: "pad", pad_color: "#000000", deinterlace: "off", keyframe_frames: 50, preset: "p1" },
  audio: { codec: "aac", bitrate_kbps: 128, tracks: "all" },
};

export async function headendAdvancedTests(test, eq, ok) {
  await customTests(test, eq, ok);
  await hlsTests(test, eq, ok);
  await detailTests(test, eq, ok);
  await importTests(test, eq, ok);
}

async function customTests(test, eq, ok) {
  const custom = await import("../panels/renditions/custom.js");
  const adv = await import("../panels/renditions/custom-advanced.js");
  test("the Custom form's Advanced answers become the want's newer fields", () => {
    const r = custom.requestFrom({ codec: "h264", size: "1280x720", fps: 0, bitrate_kbps: 1500, keyframe_s: 2, sound: "aac-128", fit: "crop", deinterlace: "auto", keyframe_frames: 50, preset: "p1", bframes: 0, tracks: "all", audio_kbps: 96 }, "flussonic");
    eq(r.video, { codec: "h264", width: 1280, height: 720, bitrate_kbps: 1500, keyframe_ms: 2000, fit: "crop", deinterlace: "auto", keyframe_frames: 50, preset: "p1", bframes: 0 });
    eq(r.audio, { codec: "aac", bitrate_kbps: 96, tracks: "all" });
  });
  test("left alone, the Advanced group sends nothing the form did not send before", () => {
    const r = custom.requestFrom({ codec: "h264", size: "1280x720", fps: 30, bitrate_kbps: 3000, keyframe_s: 2, sound: "aac-160", fit: "pad", pad_color: "#000000", deinterlace: "off", tracks: "first" }, "x");
    eq(Object.keys(r.video).sort(), ["bitrate_kbps", "codec", "fps", "height", "keyframe_ms", "width"]);
    eq(r.audio, { codec: "aac", bitrate_kbps: 160 });
  });
  test("an imported want opens on what it asks for, and saves the same", () => {
    const v = custom.valuesFrom(IMPORTED);
    eq([v.fps, v.keyframe_frames, v.preset, v.tracks, v.sound], [0, 50, "p1", "all", "aac-128"]);
    const back = custom.requestFrom(v, "flussonic");
    eq([back.video.fps, back.video.keyframe_frames, back.video.preset, back.audio.tracks], [undefined, 50, "p1", "all"]);
    ok(adv.usesAdvanced(IMPORTED), "so the group opens");
    eq(custom.valuesFrom({ audio: { codec: "aac", bitrate_kbps: 192 } }).audio_kbps, 192, "a rate the Sound choice lacks is kept in Advanced");
  });

  const { formatStep } = await import("../panels/renditions/format-step.js");
  const stub = wallStub({ n: 3 });
  const step = formatStep(stub, { current: IMPORTED });
  document.body.appendChild(step.node);
  await step.ready;
  await until(() => step.node.querySelector(".rnd-customform"));
  test("an edit of an imported want opens Custom with Advanced open and a cost badge", () => {
    const box = advanced(step.node);
    ok(box && box.open, "Advanced is open");
    ok(field(box, "Keyframe every, in frames").querySelector("input").value === "50");
    ok(step.node.querySelector('.rnd-card[data-id="custom"] .rnd-badge'), "the Custom card is badged");
  });
  put(step.node, "Deinterlace", "auto");
  test("a change in the Advanced group is in the rendition the step sends", () => eq(step.value().video.deinterlace, "auto"));
  step.node.remove();
}

async function hlsTests(test, eq, ok) {
  const h = await import("../panels/renditions/hls-settings.js");
  test("HLS params: the one click alone is what the add always sent", () => {
    eq(h.paramsFrom({}, false), { segment_ms: 2000, window: 30 });
    eq(h.paramsFrom({}, true), { segment_ms: 1000, window: 30, part_ms: 200 });
    eq(h.paramsFrom({ segments: "both", access: "open", segment_ms: 3400, window: 60 }, false), { segment_ms: 3400, window: 60, segments: "both", access: "open" });
    eq(h.paramsFrom({ access: "keyed", open_public: true }, false).open_public, undefined, "open to all only when open");
    ok(!("segments" in h.hlsSchema({ direct: false }).properties), "a show that composites makes no MPEG-TS");
    ok(h.usesAdvanced({ segments: "ts" }) && !h.usesAdvanced({ segment_ms: 2000, window: 30 }));
  });

  const { addHls } = await import("../panels/renditions/hls-add.js");
  const stub = wallStub({ n: 3 });
  await addHls(stub);
  const m = dialogs().at(-1);
  test("the add dialog keeps low latency as one click, and the rest under Advanced, closed", () => {
    const box = advanced(m);
    ok(box && !box.open, "closed");
    ok(field(box, "Who may play it") && field(box, "Segment length") && !field(box, "Segments"));
    ok(box.textContent.includes("gateway that checks its own signature"), "open says what it is for");
  });
  put(m, "Who may play it", "open");
  put(m, "Segment length", "4000");
  button(m, "Start serving").click();
  await until(() => stub.rcalls.some((c) => c.method === "output.add"));
  test("Advanced answers go out with the add", () => eq(stub.rcalls.filter((c) => c.method === "output.add").at(-1).params.params, { segment_ms: 4000, window: 30, access: "open" }));
  closeDialogs();
}

async function detailTests(test, eq, ok) {
  const stub = wallStub({ n: 3 });
  const show = stub.shows[0];
  show.outputs.push({ id: "viewers", platform: "hls", uri_host: "hls://viewers", enabled: true, rendition: null, state: "live", params: { segment_ms: 3400, part_ms: 0, window: 30, segments: "both", access: "open" } });
  const { showDetail } = await import("../panels/wall/detail.js");
  const d = showDetail(stub, show);
  const box = d.dialog.el;
  await wait(80);
  test("an HLS output says its segments and access in its row", () => ok(/CMAF and TS, open on the network/.test(box.textContent), box.textContent));
  button(box, "HLS settings").click();
  await until(() => box.querySelector(".wl-dhls"));
  test("its settings open on what it has, Advanced open since it uses them", () => {
    const adv = advanced(box.querySelector(".wl-dhls"));
    ok(adv && adv.open);
    eq(field(adv, "Segments").querySelector("select").value, "both");
  });
  put(box.querySelector(".wl-dhls"), "Segments", "ts");
  button(box, "Save settings").click();
  await until(() => sent(stub, "show.output.set").length);
  test("Save sends every param with show.output.set, which replaces them all", () => eq(sent(stub, "show.output.set")[0].params, { id: show.id, output: "viewers", params: { segment_ms: 3400, window: 30, segments: "ts", access: "open" } }));

  // A conversion the governor refuses: the refusal stays in the form.
  const inner = stub.call;
  stub.call = async (method, params = {}) => {
    if (method === "show.output.set" && params.rendition && params.rendition.preset === "youtube-1080p30") {
      stub.calls.push({ method, params });
      throw refusal("udp-out");
    }
    return inner(method, params);
  };
  button(box, "Format").click();
  await until(() => box.querySelector('.wl-dbox .rnd-card[data-id="youtube-1080p30"]'));
  box.querySelector('.rnd-card[data-id="youtube-1080p30"]').click();
  button(box, "Save format").click();
  await until(() => box.querySelector(".rnd-refusal"));
  test("a refused format keeps the form open with what would fit", () => {
    ok(box.querySelector(".wl-dbox"), "the form is still there");
    ok(/720p30 fits/.test(box.querySelector(".rnd-refusal").textContent));
  });
  d.close();
  closeDialogs();
}

const REPORT = (dry) => ({
  source: "pasted text", dry_run: dry, converted: 1, copied: 1, disabled: 1,
  streams: [
    { name: "CH117", title: "News", disabled: false, copy: false, input: { uri: "udp://@239.10.0.1:2001", program: 7, params: { interface: "eth1" } }, outputs: [{ id: "flussonic", uri: "hls://flussonic", params: { segments: "both", access: "open", segment_ms: 3400 }, rendition: IMPORTED }] },
    { name: "RELAY1", disabled: true, copy: true, input: { uri: "rtmp://127.0.0.1:1935/live/relay1" }, outputs: [{ id: "flussonic", uri: "hls://flussonic", params: { segments: "both", access: "open", segment_ms: 3400 } }] },
  ],
  notes: [{ scope: "global", line: 2, kind: "ignored", what: "directive 'cluster_key'" }],
  shows: { added: dry ? ["ch117"] : ["ch117"], refused: [{ index: 1, name: "RELAY1", why: "There is already a show called RELAY1. Pick another name.", data: {} }], plan: { fits: true }, dry_run: dry },
});

async function importTests(test, eq, ok) {
  const words = await import("../panels/headend/import-report.js");
  test("the report says each stream's rendition and HLS output in words", () => {
    eq(words.renditionWords(IMPORTED), "H.264 · 1280×720 · 1.5 Mb/s · keyframe every 50 frames · pad #000000 · preset p1 · AAC 128 kb/s, every track");
    eq(words.hlsWords({ segments: "both", access: "open", segment_ms: 3400 }), "MPEG-TS and CMAF, open on the network, 3.4 s segments");
    eq(words.renditionWords(null), "Copy, as it comes");
  });
  const { importFlussonic, importParams } = await import("../panels/headend/import.js");
  test("the import takes pasted text or a path, and says which is missing", () => {
    eq(importParams("text", "stream A { input udp://239.10.0.1:1; }", "", true), { text: "stream A { input udp://239.10.0.1:1; }", dry_run: true });
    eq(importParams("path", "", " /etc/flussonic/flussonic.conf ", false), { path: "/etc/flussonic/flussonic.conf", dry_run: false });
    ok(importParams("path", "x", "", true).error);
  });
  const stub = wallStub({ n: 2 });
  const inner = stub.call;
  stub.call = async (method, params = {}) => {
    if (method === "headend.status") return { following: true, file: "/etc/flussonic/flussonic.conf", streams: 4, shows: 4, listeners: ["0.0.0.0:8090"], note: "" };
    if (method !== "headend.import") return inner(method, params);
    stub.calls.push({ method, params });
    if (!params.text && !params.path) throw new RpcError(-32602, "send one of the two", {});
    return REPORT(params.dry_run !== false);
  };
  await importFlussonic(stub);
  const m = dialogs().at(-1);
  test("a station that follows a config says so before anything is imported", () => ok(/follows \/etc\/flussonic\/flussonic.conf/.test(m.textContent)));
  m.querySelector("textarea").value = "http 8090;\nstream CH117 { input udp://239.10.0.1:2001; }";
  button(m, "Read it").click();
  await until(() => m.querySelector(".he-imp-table"));
  test("Read it is a dry run, and the report comes first: streams, refusals, lines not carried", () => {
    eq(sent(stub, "headend.import")[0].params.dry_run, true);
    eq(m.querySelectorAll(".he-imp-table tbody tr").length, 2);
    ok(m.querySelector(".he-imp-refused") && /already a show called RELAY1/.test(m.textContent));
    ok(/Lines not carried \(1\)/.test(m.querySelector(".he-imp-notes summary").textContent));
    eq(button(m, "Make 1 show") ? "yes" : "no", "yes");
  });
  button(m, "Make 1 show").click();
  await until(() => sent(stub, "headend.import").length > 1);
  test("Make sends the same config with dry_run false", () => eq(sent(stub, "headend.import")[1].params, { text: "http 8090;\nstream CH117 { input udp://239.10.0.1:2001; }", dry_run: false }));
  closeDialogs();
}
