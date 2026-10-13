// What a destination sends, kept apart from what comes into its channel.
// The report this is for: a channel fed by the programme at 1920x1080 and
// 6.3 Mb/s, with a Custom RTMP destination set to 720p at 3000 kb/s, and the
// card showing only the channel's 1080p, which testers read as what the
// platform got.

import { ChannelStub, liveStream } from "./channels-stub.js";
import { panelOver } from "./channels-programme.js";

const wait = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const dialogs = () => [...document.querySelectorAll(".dialog")];
const closeAll = () => { for (const d of dialogs()) d.parentElement.remove(); };

/** The plan the core gives a Custom 720p30 3000 kb/s destination fed 1080p. */
export const PLAN_720 = {
  mode: "transcode", stream: "programme", reason: "encoded because the source is 1920x1080 and this output wants 1280x720",
  encoder: "h264-mediafoundation", nodes: [],
  video: { codec: "h264", width: 1280, height: 720, fps: { num: 30, den: 1 }, bitrate_kbps: 3000, keyframe_ms: 2000 },
  audio: { codec: "aac", channels: 2, sample_rate: 48000, bitrate_kbps: 128 },
};

export async function sendsTests(test, eq, ok) {
  const { sends, planFps } = await import("../panels/channels/sends.js");

  test("a destination's words are its own size, rate, codecs and measured bitrate", () => {
    const d = { state: "live", kbps: 3140, plan: PLAN_720 };
    eq(sends(d).short, "1280×720 30 fps H.264");
    eq(sends(d).line, "Sends: 1280×720 30 fps H.264, video at 3.0 Mb/s; AAC 128 kb/s. Measured going out: 3.1 Mb/s");
    eq(planFps({ num: 30000, den: 1001 }), "29.97 fps");
  });

  test("a copy says it sends what comes in, and no plan says nothing", () => {
    const copy = { mode: "copy", video: { codec: "h264", width: 1920, height: 1080, fps: { num: 30, den: 1 }, bitrate_kbps: 0 } };
    eq(sends({ state: "waiting", plan: copy }).line, "Sends what comes in, copied: 1920×1080 30 fps H.264");
    eq(sends({ state: "live", kbps: 6000 }), { short: "", line: "" });
  });

  const stub = new ChannelStub();
  await stub.call("channel.add", { name: "Programme", app: "programme" });
  await stub.call("channel.destination.add", { id: "programme", platform: "custom", label: "Custom RTMP", server: "rtmp://relay.example/live/k", enabled: true });
  const ch = stub.channels.get("programme");
  ch.streams = [liveStream("programme", { video: { codec: "h264", width: 1920, height: 1080, fps: 30.42, kbps: 6200 }, audio: { codec: "aac", channels: 2, sample_rate: 48000, kbps: 128 } })];
  Object.assign(ch.destinations[0], { state: "live", kbps: 3140, since_ms: 1000, plan: PLAN_720 });
  const host = await panelOver(stub);
  const card = () => [...host.querySelectorAll(".chn-card")].find((c) => c.textContent.includes("Programme"));

  test("the channel's line says it is what comes in", () => {
    eq(card().querySelector(".chn-statetext").textContent, "Coming in: Video 1920x1080 6.2 Mb/s | Audio AAC 128 kb/s");
    ok(card().querySelector(".chn-stream .chn-in").textContent === "Coming in", card().querySelector(".chn-stream").textContent);
  });

  test("the destination's tile says the 720p it sends and the rate it measured", () => {
    const tile = card().querySelector(".chn-tile:not(.chn-addtile)");
    eq(tile.querySelector(".chn-tsends").textContent, "1280×720 30 fps H.264");
    ok(tile.querySelector(".chn-tstate").textContent.startsWith("Live, 3.1 Mb/s"), tile.querySelector(".chn-tstate").textContent);
    ok(tile.querySelector(".chn-tsends").title.includes("video at 3.0 Mb/s"), tile.querySelector(".chn-tsends").title);
  });

  card().querySelector(".chn-tile:not(.chn-addtile) .chn-tmain").click();
  await wait();
  test("and its dialog says the same under Live", () => {
    const said = dialogs().at(-1).querySelector(".chn-dsends");
    ok(said, "no sends line in the dialog");
    eq(said.textContent, "Sends: 1280×720 30 fps H.264, video at 3.0 Mb/s; AAC 128 kb/s. Measured going out: 3.1 Mb/s");
  });
  closeAll();
  host.view.stop();
  host.remove();
}
