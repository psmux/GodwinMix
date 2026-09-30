// Renditions on the page, against renditions-stub.js: the Format step, the
// HLS ladder and its playback card, the plan's line on each row and tile,
// Resources, a governor refusal with its advice, and calibration. No core
// needed, and nothing here needs the backend to exist yet.

import { renditionStub, programmePlan } from "./renditions-stub.js";

const wait = (ms = 40) => new Promise((r) => setTimeout(r, ms));
const dialogs = () => [...document.querySelectorAll(".dialog")];
const top = () => dialogs().at(-1);
const button = (root, text) => [...root.querySelectorAll("button")].find((b) => b.textContent.trim() === text);
const closeAll = () => { for (const d of dialogs()) d.closest(".scrim")?.remove(); };
const cardIds = (root) => [...root.querySelectorAll(".rnd-card")].map((c) => c.dataset.id);
const picked = (root) => root.querySelector('.rnd-card[aria-checked="true"]')?.dataset.id;

export async function renditionTests(test, eq, ok) {
  const model = await import("../panels/renditions/model.js");
  const words = await import("../panels/renditions/words.js");
  const custom = await import("../panels/renditions/custom.js");
  const ladder = await import("../panels/renditions/ladder.js");
  const hls = await import("../panels/renditions/hls-add.js");
  const { PRESETS, refusal } = await import("./renditions-stub.js");

  test("a preset is described by its size, rate and bitrate, and badged by its cost", () => {
    eq(model.describe(PRESETS[0].request), { size: "1920×1080", fps: "30 fps", bitrate: "6 Mb/s" });
    eq(model.describe(PRESETS[5].request), { size: "Sound only", bitrate: "128 kb/s" });
    eq(model.costClass({ id: "copy" }), "free");
    eq(model.costClass(PRESETS[1], 6000), "light", "720p30 on a machine with room");
    eq(model.costClass(PRESETS[0], 6000), "heavy", "1080p30 on the CPU");
    eq(model.costClass({ ...PRESETS[0], cost: { cpu_millicores: 60, device_millis: 300 } }), "light", "on a GPU");
  });

  test("the platform suggests its preset, and copy wins when the source already matches", () => {
    eq(model.initialChoice(PRESETS, "youtube", null), "youtube-1080p30");
    eq(model.initialChoice(PRESETS, "twitch", null), "twitch-720p30", "1080p60 is not available here");
    eq(model.initialChoice(PRESETS, "custom", null), "copy");
    const same = { video: { codec: "h264", width: 1920, height: 1080, fps: { num: 30, den: 1 }, bitrate_kbps: 5500 } };
    eq(model.initialChoice(PRESETS, "youtube", same), "copy");
    eq(model.initialChoice(PRESETS, "youtube", { video: { ...same.video, bitrate_kbps: 2000 } }), "youtube-1080p30", "too far off in bitrate");
    eq(model.initialChoice(PRESETS, "youtube", same, { preset: "youtube-720p30" }), "youtube-720p30", "an edit opens on what is set");
  });

  test("the plan's line says copied, which encoder, what it shares and why", () => {
    const plan = programmePlan();
    eq(words.planLine(plan, "relay"), "Copied, no re-encoding");
    eq(words.planLine(plan, "youtube"), "Encoded on the GPU (h264-videotoolbox), shared with 2 others");
    eq(words.planLine(plan, "twitch"), "Encoded on the CPU (x264) because the GPU is full");
    eq(words.planLine(plan, "nobody"), "");
    eq(words.planLine({ nodes: [{ kind: "encode", serves: ["a"], encoder: "x264" }] }, "a"), "Encoded on the CPU (x264)", "an encoder named by id alone");
    eq(words.planShort(plan, "youtube"), "GPU h264-videotoolbox, shared");
  });

  test("a refusal says what it needs and what is left, in words", () => {
    const w = words.refusalWords(refusal("youtube").data);
    eq(w.need, "This needs about 1.8 cores of the CPU, one GPU encoder session and 6 Mb/s of upload.");
    eq(w.room, "Without dropping what is on air, there is room for 60% of one core of the CPU, no GPU sessions and 30 Mb/s of upload.");
    ok(words.isRefusal(refusal("x")), "a governor refusal");
    ok(!words.isRefusal({ data: { action: {} } }), "not any other");
  });

  test("the Custom form's answers become a RenditionRequest and back", () => {
    const r = custom.requestFrom({ codec: "h265", size: "1280x720", fps: 60, bitrate_kbps: 2500, keyframe_s: 2, sound: "opus-128" }, "yt");
    eq(r, { id: "yt", container: "flv", video: { codec: "h265", width: 1280, height: 720, fps: { num: 60, den: 1 }, bitrate_kbps: 2500, keyframe_ms: 2000 }, audio: { codec: "opus", bitrate_kbps: 128 } });
    eq(custom.valuesFrom(r), { codec: "h265", size: "1280x720", fps: 60, bitrate_kbps: 2500, keyframe_s: 2, sound: "opus-128" });
    eq(custom.requestFrom({ sound: "none" }, "a").no_audio, true);
    eq(custom.customSchema(new Set(["h265"])).properties.codec.oneOf.map((o) => o.const), ["h264", "h265"], "only codecs this machine has");
  });

  test("a ladder's rungs are 16:9, and a custom ladder must make sense", () => {
    eq(ladder.rung(480).video.width, 854);
    eq(ladder.ladderError([]), "Add at least one rung.");
    eq(ladder.ladderError([ladder.rung(720), ladder.rung(720)]), "Two rungs are the same size. Change or remove one.");
    eq(hls.hlsParams("viewers", { preset: "abr-ladder-4" }, false).params, { id: "viewers", type: "hls/output", rendition: { preset: "abr-ladder-4" }, params: { segment_ms: 2000, window: 30 } });
    eq(hls.hlsParams("Big Room", { preset: "x" }).error, "Give it a short name of small letters, numbers and dashes.");
    eq(hls.hlsParams("v", { preset: "x" }, true).params.params.part_ms, 200);
  });

  await formatStepTests(test, eq, ok);
  await addAndRefuseTests(test, eq, ok);
  await hlsTests(test, eq, ok);
  await outputsPanelTests(test, eq, ok);
  await channelTileTests(test, eq, ok);
}

async function formatStepTests(test, eq, ok) {
  const { formatStep } = await import("../panels/renditions/format-step.js");
  const stub = renditionStub();
  const step = formatStep(stub, { platform: "youtube", platformTitle: "YouTube", id: () => "youtube" });
  document.body.appendChild(step.node);
  await step.ready;
  test("copy is the first card and free; the platform's preset is picked and says so", () => {
    eq(cardIds(step.node).slice(0, 3), ["copy", "youtube-1080p30", "youtube-720p30"]);
    eq(step.node.querySelector(".rnd-card .rnd-badge").textContent, "Free");
    eq(picked(step.node), "youtube-1080p30");
    ok(step.node.querySelector('[data-id="youtube-1080p30"]').textContent.includes("Suggested for YouTube"));
    ok(!cardIds(step.node).includes("abr-ladder-4"), "ladders belong to HLS");
    eq(cardIds(step.node).at(-1), "custom");
    eq(step.value(), { preset: "youtube-1080p30" });
  });
  test("a preset this machine cannot make is dimmed, with the reason", () => {
    const off = step.node.querySelector('[data-id="twitch-1080p60"]');
    ok(off.disabled && off.classList.contains("off"));
    ok(off.textContent.includes("Needs a GPU encoder"), off.textContent);
  });
  step.node.querySelector('[data-id="custom"]').click();
  await wait(80);
  test("Custom opens the schema form under the cards, and its answer is a request", () => {
    ok(!step.node.querySelector(".rnd-customwrap").hidden);
    ok(step.node.querySelector(".rnd-customform select"), "a form built by the schema kit");
    const v = step.value();
    eq([v.id, v.video.width, v.video.height, v.audio.codec], ["youtube", 1280, 720, "aac"]);
  });
  step.node.querySelector('[data-id="copy"]').click();
  test("copy sends no rendition at all", () => eq(step.value(), undefined));
  step.node.remove();

  const matching = renditionStub({ programmeKbps: 6000 });
  const { programmeShape } = await import("../panels/renditions/format-step.js");
  const same = formatStep(matching, { platform: "youtube", platformTitle: "YouTube", shape: programmeShape(matching) });
  await same.ready;
  test("when the programme already is YouTube 1080p, copy stays first and picked", () => {
    eq(picked(same.node), "copy");
    ok(same.node.querySelector('[data-id="copy"]').textContent.includes("Already matches YouTube 1080p"));
  });
  const old = formatStep(renditionStub({ presets: false }), { platform: "youtube" });
  await old.ready;
  test("a core without renditions shows no step and sends what it always did", () => {
    ok(old.node.hidden);
    eq(old.value(), undefined);
  });
  const edit = formatStep(stub, { platform: "youtube", current: { preset: "youtube-720p30" } });
  await edit.ready;
  test("an edit opens on the format in force and sends nothing unless it changes", () => {
    eq(picked(edit.node), "youtube-720p30");
    eq(edit.value(), undefined);
    edit.node.querySelector('[data-id="copy"]').click();
    eq(edit.value(), null, "back to copy clears it");
  });
}

async function addAndRefuseTests(test, eq, ok) {
  const { addDestination } = await import("../panels/outputs/destination.js");
  const stub = renditionStub();
  await addDestination(stub);
  await wait(60);
  const tiles = () => [...top().querySelectorAll(".kindtile")];
  test("Add destination offers HLS for viewers on a core that serves it", () => ok(tiles().some((t) => t.textContent.includes("HLS for viewers"))));
  tiles().find((t) => t.textContent.startsWith("YouTube")).click();
  await wait(150);
  const form = top();
  test("the YouTube form has the Format step, on YouTube 1080p", () => {
    ok(form.querySelector(".rnd-step"), "a Format step");
    eq(picked(form), "youtube-1080p30");
  });
  const key = form.querySelector("input[type=password]");
  key.value = "abcd-efgh";
  key.dispatchEvent(new Event("input"));
  stub.full = true;
  button(form, "Start sending").click();
  await wait(120);
  test("a refused add stays in the form with the need, the room and the advice", () => {
    eq(stub.rcalls.filter((c) => c.method === "output.add").at(-1).params.rendition, { preset: "youtube-1080p30" });
    const box = form.querySelector(".rnd-refusal");
    ok(box, "the refusal is in the form");
    ok(box.textContent.includes("This needs about 1.8 cores"), box.textContent);
    ok(box.textContent.includes("there is room for 60% of one core"), box.textContent);
    eq([...box.querySelectorAll(".rnd-try")].map((b) => b.firstChild.textContent), ["720p30 fits", "Same as the source"]);
    ok(dialogs().includes(form), "the form is still open");
  });
  form.querySelector(".rnd-try").click();
  await wait(120);
  test("pressing advice retries with that request and closes the form", () => {
    const last = stub.rcalls.filter((c) => c.method === "output.add").at(-1).params;
    eq([last.id, last.rendition.video.height, last.uri], ["youtube", 720, "rtmp://a.rtmp.youtube.com/live2/abcd-efgh"]);
    ok(!dialogs().includes(form), "closed");
  });
  closeAll();
}

async function hlsTests(test, eq, ok) {
  const { addHls } = await import("../panels/renditions/hls-add.js");
  const stub = renditionStub();
  await addHls(stub);
  const m = top();
  test("the ladder picker offers the ladders drawn to scale, and Custom", () => {
    eq(cardIds(m), ["abr-ladder-4", "abr-ladder-3", "custom"]);
    eq(m.querySelectorAll('[data-id="abr-ladder-4"] rect').length, 4);
    eq(picked(m), "abr-ladder-4");
  });
  m.querySelector('[data-id="custom"]').click();
  button(m, "Add a rung").click();
  test("custom rungs are added and removed", () => {
    eq(m.querySelectorAll(".rnd-rung").length, 4);
    m.querySelector(".rnd-rung .btn.icon").click();
    eq(m.querySelectorAll(".rnd-rung").length, 3);
  });
  m.querySelector('[data-id="abr-ladder-4"]').click();
  button(m, "Start serving").click();
  await wait(60);
  test("Start serving adds an hls/output with the ladder", () => {
    eq(stub.rcalls.at(-1), { method: "output.add", params: { id: "viewers", type: "hls/output", rendition: { preset: "abr-ladder-4" }, params: { segment_ms: 2000, window: 30 } } });
  });
  closeAll();

  const { hlsCard, playsHls } = await import("../panels/renditions/hls-card.js");
  const card = hlsCard(stub, { id: "viewers", type: "hls/output", state: "connecting" });
  card.update({ id: "viewers", state: "connecting" });
  await wait(30);
  test("the playback card has the link, a QR code and Copy; Watch waits for live", () => {
    eq(card.node.querySelector(".rnd-url").textContent, "http://192.168.1.20:8080/hls/viewers/master.m3u8");
    ok(card.node.querySelector(".rnd-qr svg path"), "a QR code");
    ok(button(card.node, "Copy"));
    ok(button(card.node, "Watch here").disabled, "nothing to watch yet");
  });
  card.update({ id: "viewers", state: "live" });
  button(card.node, "Watch here").click();
  test("once live, Watch here opens a player or says where to open it", () => {
    const player = card.node.querySelector(".rnd-player");
    if (playsHls()) ok(player.querySelector("video"), "a native player");
    else ok(player.querySelector("a[href$='master.m3u8']").textContent === "Open in a player", player.textContent);
  });
}

async function outputsPanelTests(test, eq, ok) {
  await import("../panels/outputs/panel.js");
  const stub = renditionStub();
  const live = (id, extra = {}) => ({ id, uri_host: "rtmp://x/…", has_key: true, state: "live", reconnects: 0, queue_secs: 0, ...extra });
  stub.setOutputs([live("youtube"), live("twitch"), live("relay"), live("viewers", { type: "hls/output" })]);
  const panel = document.createElement("gmx-outputs");
  panel.setClient(stub);
  document.body.appendChild(panel);
  await wait(120);
  const planOf = (id) => [...panel.querySelectorAll(".output-row")].find((r) => r.querySelector("strong").textContent === id).querySelector(".output-plan").textContent;
  test("each output row says what the plan did for it", () => {
    eq(planOf("relay"), "Copied, no re-encoding");
    eq(planOf("twitch"), "Encoded on the CPU (x264) because the GPU is full");
    eq(stub.patterns.get("rendition.*"), 1, "plan events asked for while rows are on screen");
    ok(panel.querySelector(".rnd-hls .rnd-url"), "the HLS row carries its link");
  });
  stub.event("rendition.plan", { scope: "programme", plan: { nodes: [{ kind: "copy", serves: ["twitch"] }] } });
  await wait(30);
  test("event/rendition.plan rewrites the lines in place", () => eq(planOf("twitch"), "Copied, no re-encoding"));

  await panel.show("resources");
  await wait(80);
  const res = panel.querySelector(".rnd-resources");
  test("Resources draws the CPU, each GPU with its sessions, upload and what was shed", () => {
    ok(panel.querySelector(".output-row").closest("[hidden]"), "the rows are put away");
    const cards = [...res.querySelectorAll(".rnd-rescard")].map((c) => c.querySelector(".rnd-kicker").textContent);
    eq(cards, ["CPU", "GPU (apple-m1)", "Upload", "Shed"]);
    ok(res.querySelectorAll(".rnd-rescard")[0].querySelectorAll(".rnd-seg").length >= 3, "used, room and kept");
    ok(res.textContent.includes("x264 for twitch") || res.textContent.includes("Copying"), "who uses the CPU");
    ok(res.textContent.includes("3 of 4 sessions"));
    ok(res.textContent.includes("Multiview thumbnails"));
    eq(stub.patterns.get("governor.*"), 1);
  });
  test("measuring again is refused while on air, with the reason", () => {
    const b = button(res, "Measure this machine again");
    ok(b.disabled);
    ok(!res.querySelector(".rnd-why").hidden);
  });
  stub.event("governor.shed", { what: "The 360p rung of viewers", why: "The CPU is throttling." });
  stub.setOutputs([]);
  await wait(30);
  test("a shed event is listed, and off air the button works", () => {
    ok(res.querySelector(".rnd-shed").textContent.includes("The 360p rung of viewers"));
    const b = button(res, "Measure this machine again");
    ok(!b.disabled);
    b.click();
    eq(stub.rcalls.at(-1).method, "governor.calibrate");
  });
  panel.setWorkspaceActive(false);
  test("off screen, the panel asks for nothing", () => {
    eq(stub.patterns.get("governor.*"), undefined);
    eq(stub.patterns.get("rendition.*"), undefined);
  });
  panel.remove();
}

async function channelTileTests(test, eq, ok) {
  const { mount } = await import("../panels/channels/panel.js");
  const stub = renditionStub();
  const a = await stub.call("channel.add", { name: "Sunday" });
  await stub.call("channel.destination.add", { id: a.channel.id, platform: "youtube", server: "rtmp://a.rtmp.youtube.com/live2", key: "k", rendition: { preset: "youtube-720p30" } });
  stub.plans["channel:sunday"] = { nodes: [{ kind: "encode", serves: ["youtube"], encoder: "x264", reason: { code: "no_gpu", text: "This machine has no GPU encoder" } }] };
  const host = document.createElement("div");
  host.client = stub;
  document.body.appendChild(host);
  mount(host);
  await wait(120);
  test("a channel's destination tile says what the plan did, whole on hover", () => {
    const line = host.querySelector(".chn-tplan");
    eq(line.textContent, "CPU x264");
    eq(line.title, "Encoded on the CPU (x264) because this machine has no GPU encoder");
    eq(stub.patterns.get("rendition.*"), 1);
  });
  host.view.stop();
  test("and stops asking when the panel goes", () => eq(stub.patterns.get("rendition.*"), undefined));
  host.remove();
}
