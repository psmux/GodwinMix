// Renditions against the stub, one state per address, for looking at and
// for screenshots: /test/renditions.html?scene=format&theme=dark
//
// Scenes: format, custom, refused, hls, outputs, resources, channel,
// chanform. Nothing here talks to a core.

import { renditionStub } from "./renditions-stub.js";

const q = new URLSearchParams(location.search);
const scene = q.get("scene") || "format";
const theme = q.get("theme") || "dark";
document.getElementById("gmx-theme").href = `../themes/${theme}.css`;
document.documentElement.dataset.theme = theme;

const wait = (ms = 80) => new Promise((r) => setTimeout(r, ms));
const stage = document.getElementById("stage");
const stub = renditionStub();
const live = (id, extra = {}) => ({ id, uri_host: extra.host || "rtmp://a.rtmp.youtube.com/…", has_key: true, state: "live", reconnects: 0, queue_secs: 0.4, ...extra });
const OUTPUTS = [
  live("youtube"),
  live("facebook", { host: "rtmps://live-api-s.facebook.com:443/…" }),
  live("twitch", { host: "rtmp://live.twitch.tv/…" }),
  live("relay", { host: "srt://10.0.0.9:9000" }),
  live("viewers", { type: "hls/output", host: "hls" }),
];

async function youtubeForm() {
  const { addDestination } = await import("../panels/outputs/destination.js");
  await addDestination(stub);
  await wait();
  [...document.querySelectorAll(".kindtile")].find((t) => t.textContent.startsWith("YouTube")).click();
  await wait(250);
  const key = document.querySelector(".dialog input[type=password]");
  key.value = "xxxx-xxxx-xxxx-xxxx";
  key.dispatchEvent(new Event("input"));
}

async function outputs(view) {
  await import("../panels/outputs/panel.js");
  stub.setOutputs(OUTPUTS);
  const panel = document.createElement("gmx-outputs");
  panel.setClient(stub);
  stage.appendChild(panel);
  if (view) await panel.show(view);
}

async function main() {
  if (scene === "format") await youtubeForm();
  else if (scene === "custom") {
    await youtubeForm();
    document.querySelector('.rnd-card[data-id="custom"]').click();
  } else if (scene === "refused") {
    await youtubeForm();
    stub.full = true;
    [...document.querySelectorAll(".dialog button")].find((b) => b.textContent === "Start sending").click();
  } else if (scene === "hls") {
    (await import("../panels/renditions/hls-add.js")).addHls(stub);
  } else if (scene === "outputs") await outputs();
  else if (scene === "resources") await outputs("resources");
  else if (scene === "channel" || scene === "chanform") await channel();
  await wait(300);
  document.body.dataset.ready = "1";
}

async function channel() {
  const a = await stub.call("channel.add", { name: "Sunday service" });
  const id = a.channel.id;
  for (const d of [
    { platform: "youtube", server: "rtmp://a.rtmp.youtube.com/live2", key: "k" },
    { platform: "facebook", server: "rtmps://live-api-s.facebook.com:443/rtmp", key: "k", rendition: { preset: "facebook-720p30" } },
    { platform: "twitch", server: "rtmp://live.twitch.tv/app", key: "k", rendition: { preset: "twitch-720p30" } },
  ]) await stub.call("channel.destination.add", { id, ...d });
  stub.plans[`channel:${id}`] = { nodes: [
    { kind: "copy", serves: ["youtube"] },
    { kind: "encode", serves: ["facebook"], encoder: { id: "h264-videotoolbox", hardware: true } },
    { kind: "encode", serves: ["twitch"], encoder: "x264", reason: { code: "gpu_full", text: "The GPU is full" } },
  ] };
  const host = document.createElement("div");
  host.client = stub;
  stage.appendChild(host);
  const m = await import("../panels/channels/panel.js");
  m.mount(host);
  if (scene === "chanform") {
    await wait();
    const { platform } = await import("../client/destinations.js");
    (await import("../panels/channels/destination-form.js")).addDestination(host.view, host.view.model.byId.get(id), platform("facebook"));
  }
}

main();
