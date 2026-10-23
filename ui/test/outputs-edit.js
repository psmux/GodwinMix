// Two things testers of 0.3.17 could not find, against the stubs: the
// Advanced settings of a custom rung in HLS for viewers, and Edit on an
// output that is already saved, for an HLS output and an address on the
// Outputs panel, and for an output of a show without compositing in its
// detail. No core needed.

import { renditionStub } from "./renditions-stub.js";
import { wallStub } from "./wall-stub.js";
import { wait, until, sent, button, closeDialogs, dialogs } from "./wall-help.js";

const top = () => dialogs().at(-1);
const field = (root, label) => [...root.querySelectorAll(".field")].find((f) => f.querySelector(".lbl") && f.querySelector(".lbl").firstChild.textContent === label);
const put = (root, label, value) => {
  const input = field(root, label).querySelector("select, input");
  if (input.type === "checkbox") input.checked = !!value;
  else input.value = String(value);
  input.dispatchEvent(new Event(input.tagName === "SELECT" || input.type === "checkbox" ? "change" : "input", { bubbles: true }));
};
const rungBoxes = (root) => [...root.querySelectorAll(".rnd-rungbox")];
const rungAdv = (box) => box.querySelector(".rnd-rungmore details");
const typeInto = (input, value) => {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
};

export async function outputsEditTests(test, eq, ok) {
  await ladderTests(test, eq, ok);
  await hlsEditTests(test, eq, ok);
  await addressEditTests(test, eq, ok);
  await showOutputTests(test, eq, ok);
}

async function ladderTests(test, eq, ok) {
  const adv = await import("../panels/renditions/rung-advanced.js");
  const ladder = await import("../panels/renditions/ladder.js");
  test("a rung keeps its Advanced settings when its size or bitrate changes", () => {
    const was = ladder.rung(720);
    Object.assign(was.video, { fit: "crop", preset: "p4", bframes: 0, keyframe_frames: 50 });
    const now = adv.keepAdvanced(ladder.rung(540, 2000), was);
    eq([now.video.height, now.video.bitrate_kbps, now.video.fit, now.video.preset, now.video.bframes, now.video.keyframe_frames], [540, 2000, "crop", "p4", 0, undefined]);
    ok(!adv.rungUsesAdvanced(ladder.rung(360)), "a plain rung opens closed");
  });
  test("a saved ladder comes back as rungs with its sound, and the sound goes on every rung", () => {
    const saved = [{ id: "720p", video: { codec: "h264", height: 720, bitrate_kbps: 2500, fit: "stretch" }, audio: { codec: "aac", bitrate_kbps: 96 } }, { id: "360p", video: { height: 360 } }];
    const r = ladder.rungsFrom(saved);
    eq([r[0].video.width, r[0].video.fit, r[1].video.bitrate_kbps], [1280, "stretch", 800]);
    eq(ladder.soundOf(saved), 96);
    eq(ladder.withSound(r, 96).map((x) => x.audio.bitrate_kbps), [96, 96]);
    eq(ladder.withSound(r, 5)[0].audio.bitrate_kbps, 128, "a rate out of range is the usual one");
  });

  const { addHls } = await import("../panels/renditions/hls-add.js");
  const stub = renditionStub();
  await addHls(stub);
  const m = top();
  m.querySelector('[data-id="custom"]').click();
  await until(() => rungBoxes(m).length && rungBoxes(m).every(rungAdv));
  test("every custom rung has an Advanced group of its own, closed", () => {
    const boxes = rungBoxes(m);
    eq(boxes.length, 3);
    eq(boxes.map((b) => rungAdv(b).querySelector("summary").textContent), ["Advanced, 720p", "Advanced, 480p", "Advanced, 360p"]);
    ok(boxes.every((b) => !rungAdv(b).open));
    const labels = [...rungAdv(boxes[0]).querySelectorAll(".field .lbl")].map((l) => l.firstChild.textContent);
    eq(labels, ["Another shape", "Bar colour", "Deinterlace", "Encoder preset", "B frames"]);
  });
  const first = rungAdv(rungBoxes(m)[0]);
  first.open = true;
  put(first, "Another shape", "crop");
  put(first, "Encoder preset", "p4");
  put(first, "B frames", 2);
  put(rungAdv(rungBoxes(m)[2]), "Deinterlace", "auto");
  // A new bitrate on the top rung makes it again; what Advanced said stays.
  typeInto(rungBoxes(m)[0].querySelector('input[aria-label="Bitrate in kb/s"]'), "2500");
  await until(() => rungBoxes(m).every(rungAdv));
  typeInto(m.querySelector('input[aria-label="Sound bitrate in kb/s"]'), "96");
  button(m, "Start serving").click();
  await until(() => stub.rcalls.some((c) => c.method === "output.add"));
  test("Start serving sends each rung's Advanced settings, and the one sound on every rung", () => {
    const call = stub.rcalls.filter((c) => c.method === "output.add").at(-1);
    const rungs = call.params.rendition.ladder;
    eq(rungs.map((r) => r.video.height), [720, 480, 360]);
    eq(rungs[0].video, { codec: "h264", width: 1280, height: 720, bitrate_kbps: 2500, keyframe_ms: 2000, fit: "crop", preset: "p4", bframes: 2 });
    eq(rungs[1].video, { codec: "h264", width: 854, height: 480, bitrate_kbps: 1400, keyframe_ms: 2000 });
    eq(rungs[2].video.deinterlace, "auto");
    ok(rungs.every((r) => r.audio.bitrate_kbps === 96 && r.audio.codec === "aac"));
  });
  closeDialogs();
}

const HLS = {
  id: "viewers", type: "hls/output", state: "live", uri_host: "", has_key: true, reconnects: 0, queue_secs: 0,
  rendition: { ladder: [{ id: "720p", container: "hls", video: { codec: "h264", width: 1280, height: 720, bitrate_kbps: 3000, keyframe_ms: 1000, preset: "p4" }, audio: { codec: "aac", bitrate_kbps: 160 } }, { id: "360p", container: "hls", video: { codec: "h264", width: 640, height: 360, bitrate_kbps: 800, keyframe_ms: 1000 }, audio: { codec: "aac", bitrate_kbps: 160 } }] },
  low_latency: true, segment_ms: 1000, part_ms: 200, window: 30, access: "keyed", open_public: false,
  playback: { master_url_path: "/hls/viewers/master.m3u8?key=abcdefghijklmnopqrstuvwx" },
};

async function hlsEditTests(test, eq, ok) {
  const hls = await import("../panels/renditions/hls-add.js");
  test("an edit says every HLS setting, so one turned back to its default is not kept", () => {
    const p = hls.hlsEditParams("viewers", { preset: "abr-ladder-3" }, { segment_ms: 2000, window: 30 }).params;
    eq(p, { id: "viewers", rendition: { preset: "abr-ladder-3" }, segment_ms: 2000, window: 30, part_ms: 0, access: "keyed", open_public: false });
    eq(hls.savedParams(HLS), { window: 30, access: "keyed", open_public: false }, "the low latency click covers 1000 and 200");
  });

  await import("../panels/outputs/panel.js");
  const stub = renditionStub();
  stub.setOutputs([{ ...HLS }]);
  const panel = document.createElement("gmx-outputs");
  panel.setClient(stub);
  document.body.appendChild(panel);
  await wait(120);
  const row = panel.querySelector(".output-row");
  test("an HLS output's row has Edit", () => ok(button(row, "Edit"), row.textContent));
  button(row, "Edit").click();
  await until(() => top() && top().querySelector(".rnd-rungbox .rnd-rungmore details"));
  const m = top();
  test("Edit opens HLS for viewers on what the output has", () => {
    eq(m.querySelector("header h2").textContent, "Edit viewers");
    eq(m.querySelector('.rnd-card[aria-checked="true"]').dataset.id, "custom");
    eq(rungBoxes(m).map((b) => b.querySelector("select").value), ["720", "360"]);
    ok(m.querySelector(".rnd-check input").checked, "low latency, as it is");
    ok(m.querySelector('input[autocomplete="off"]').disabled, "the name is part of the link and stays");
    eq(m.querySelector('input[aria-label="Sound bitrate in kb/s"]').value, "160");
  });
  await until(() => rungAdv(rungBoxes(m)[0]).open);
  test("a rung that uses Advanced opens on it", () => {
    const first = rungAdv(rungBoxes(m)[0]);
    ok(first.open);
    eq(field(first, "Encoder preset").querySelector("input").value, "p4");
  });
  m.querySelector(".rnd-check input").checked = false;
  button(m, "Save").click();
  await until(() => stub.rcalls.some((c) => c.method === "output.set"));
  test("Save sends output.set with the ladder as it stands and low latency off", () => {
    const p = stub.rcalls.filter((c) => c.method === "output.set").at(-1).params;
    eq([p.id, p.segment_ms, p.part_ms, p.window, p.access, p.open_public], ["viewers", 2000, 0, 30, "keyed", false]);
    eq(p.rendition.ladder.map((r) => [r.video.height, r.video.preset, r.audio.bitrate_kbps]), [[720, "p4", 160], [360, undefined, 160]]);
  });
  panel.remove();
  closeDialogs();
}

async function addressEditTests(test, eq, ok) {
  const d = await import("../panels/outputs/destination.js");
  test("an output that is only an address edits as one, with no key", () => {
    const p = d.platformFor({ id: "lan", uri_host: "udp://239.10.0.1:5001" });
    eq([p.id, p.key, p.provides], ["udp", false, "udp/output"]);
    eq(d.platformFor({ id: "yt", uri_host: "rtmp://a.rtmp.youtube.com/live2/…" }).id, "youtube");
    eq(d.platformFor({ id: "s", uri_host: "srt://10.0.0.9:9000" }).id, "srt");
    eq(d.schemeError(p, "udp://239.10.0.2:5001"), null);
    eq(d.schemeError(p, "rtmp://x/live"), "This destination sends over udp://. Change what the address starts with.");
    eq(d.paramsFor(p, { id: "lan" }, { server: "udp://239.10.0.2:5001" }).params, { id: "lan", uri: "udp://239.10.0.2:5001" });
  });
  const stub = renditionStub();
  await d.editDestination(stub, { id: "lan", uri_host: "udp://239.10.0.1:5001", has_key: true, state: "live" });
  await until(() => top() && top().querySelector(".rnd-step"));
  const m = top();
  test("its form asks for the address and not a stream key", () => {
    ok(field(m, "Address"), m.textContent);
    ok(!field(m, "Stream key"));
  });
  typeInto(field(m, "Address").querySelector("input"), "udp://239.10.0.2:5001");
  button(m, "Save").click();
  await until(() => stub.rcalls.some((c) => c.method === "output.set"));
  test("Save sends the new address with output.set", () => {
    const p = stub.rcalls.filter((c) => c.method === "output.set").at(-1).params;
    eq([p.id, p.uri], ["lan", "udp://239.10.0.2:5001"]);
  });
  closeDialogs();
}

async function showOutputTests(test, eq, ok) {
  const d = await import("../panels/outputs/destination.js");
  const yt = d.platformFor({ platform: "youtube" });
  test("a show output's edit names only what moved, the key apart from the address", () => {
    eq(d.showParamsFor(yt, { id: "youtube" }, "CH101", { server: yt.server, key: " abcd-1234 " }).params, { id: "CH101", output: "youtube", key: "abcd-1234" });
    const udp = d.platformFor({ platform: "udp", uri_host: "udp://239.10.0.1:5001" });
    eq(d.showParamsFor(udp, { id: "udp" }, "CH101", { server: "udp://239.10.0.3:5001" }).params, { id: "CH101", output: "udp", uri: "udp://239.10.0.3:5001" });
    eq(d.showParamsFor(udp, { id: "udp" }, "CH101", { server: "srt://x:1" }).field, "server");
  });

  const stub = wallStub({ n: 3 });
  const show = stub.shows[0];
  show.id = "ch101";
  show.name = "CH101";
  show.outputs = [{ id: "udp", platform: "udp", label: "udp", uri_host: "udp://239.10.0.1:5001", has_key: true, enabled: true, rendition: null, state: "live" }];
  const { showDetail } = await import("../panels/wall/detail.js");
  const det = showDetail(stub, show);
  const box = det.dialog.el;
  await wait(80);
  test("each output of a direct show has Edit beside Format", () => ok(button(box, "Edit") && button(box, "Format"), box.textContent));
  button(box, "Edit").click();
  await until(() => top() !== det.dialog.el && top().querySelector(".rnd-step"));
  const m = top();
  test("Edit opens the destination form on the output, with its format", () => {
    ok(field(m, "Address"));
    ok(!field(m, "When it drops"), "a show's output has no reconnect policy of its own");
    eq(m.querySelector('.rnd-card[aria-checked="true"]').dataset.id, "copy");
  });
  typeInto(field(m, "Address").querySelector("input"), "udp://239.10.0.4:5001");
  button(m, "Save").click();
  await until(() => sent(stub, "show.output.set").length);
  test("Save sends show.output.set for that show and output", () => eq(sent(stub, "show.output.set").at(-1).params, { id: "ch101", output: "udp", uri: "udp://239.10.0.4:5001" }));
  det.close();
  closeDialogs();
}
