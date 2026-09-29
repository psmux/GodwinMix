// The Channels panel against the stub, one state per address, for looking at
// and for screenshots: /test/channels.html?scene=cards&theme=light
//
// Scenes: empty, cards, create, reveal, connect, settings, pick, addform,
// editdest, install. Nothing here talks to a core.

import { ChannelStub, liveStream } from "./channels-stub.js";

const q = new URLSearchParams(location.search);
const scene = q.get("scene") || "cards";
const theme = q.get("theme") || "dark";
document.getElementById("gmx-theme").href = `../themes/${theme}.css`;
document.documentElement.dataset.theme = theme;

const stub = new ChannelStub();
if (scene === "install") stub.call = async () => { throw Object.assign(new Error("no such method"), { code: -32601, data: {} }); };

async function seed() {
  if (scene === "empty" || scene === "install") return;
  const a = await stub.call("channel.add", { name: "Sunday service" });
  await stub.call("channel.key.add", { id: a.channel.id, label: "Camera op" });
  const c = stub.channels.get("sunday-service");
  c.streams = [
    liveStream("main", { source: "sunday-service-main", key: "key-2" }),
    liveStream("main_720p", { source: "sunday-service-main_720p", key: "key-2", video: { codec: "h264", width: 1280, height: 720, fps: 29.97, kbps: 2500 } }),
  ];
  const dests = [
    { platform: "youtube", server: "rtmp://a.rtmp.youtube.com/live2", key: "x" },
    { platform: "facebook", server: "rtmps://live-api-s.facebook.com:443/rtmp", key: "x" },
    { platform: "twitch", server: "rtmp://live.twitch.tv/app", key: "x" },
    { platform: "kick", server: "rtmps://fa723fc1b171.global-contribute.live-video.net:443/app", key: "x", enabled: false },
    { platform: "custom", server: "rtmp://relay.church.example/live", label: "Overflow room" },
  ];
  for (const d of dests) await stub.call("channel.destination.add", { id: c.id, ...d });
  const [yt, fb, tw, , custom] = c.destinations;
  Object.assign(yt, { state: "live", kbps: 4660 });
  Object.assign(fb, { state: "connecting" });
  Object.assign(tw, { state: "failed", error: "Twitch refused the stream key. Paste a fresh one from the Creator Dashboard." });
  Object.assign(custom, { state: "reconnecting", reconnects: 3, error: "Nothing answered at relay.church.example." });
  await stub.call("channel.add", { name: "Youth night" });
  await stub.call("channel.add", { name: "Studio B" });
  await stub.call("channel.set", { id: "studio-b", enabled: false });
}

async function main() {
  await seed();
  const { mount } = await import("../panels/channels/panel.js");
  const host = document.createElement("div");
  host.client = stub;
  document.getElementById("stage").appendChild(host);
  mount(host);
  // Forty updates' worth of bitrate, so the sparklines have a shape.
  const c = stub.channels.get("sunday-service");
  for (let i = 0; c && i < 40; i++) {
    for (const s of c.streams) s.video.kbps = Math.round(s.video.kbps * (0.9 + Math.random() * 0.2));
    stub.changed("sunday-service");
  }
  await new Promise((r) => setTimeout(r, 50));
  const m = await import("../panels/channels/panel.js");
  const view = host.view;
  const pick = (id) => view.model.byId.get(id);
  if (scene === "create") {
    await m.addChannel(stub);
    const input = document.querySelector(".chn-bigin");
    input.value = "Easter Sunday 2027";
    input.dispatchEvent(new Event("input"));
  } else if (scene === "reveal") {
    const { showKey } = await import("../panels/channels/reveal.js");
    showKey(stub, view.model, pick("sunday-service"), { id: "key-9", label: "Camera 2", secret: "k9Xq7Lm2Pz9Rv4Tw" });
  } else if (scene === "connect") {
    (await import("../panels/channels/reveal.js")).showConnect(stub, view.model, pick("youth-night"));
  } else if (scene === "settings") {
    (await import("../panels/channels/edit.js")).editChannel(view, pick("sunday-service"));
  } else if (scene === "pick") {
    (await import("../panels/channels/destination-form.js")).addDestination(view, pick("youth-night"));
  } else if (scene === "addform") {
    const { platform } = await import("../client/destinations.js");
    (await import("../panels/channels/destination-form.js")).addDestination(view, pick("sunday-service"), platform("youtube"));
  } else if (scene === "editdest") {
    const ch = pick("sunday-service");
    (await import("../panels/channels/destination-form.js")).editDestination(view, ch, ch.destinations[2]);
  }
  document.body.dataset.ready = "1";
}

main().catch((e) => {
  console.error(e);
  document.body.dataset.ready = "error";
  document.body.append(String(e && e.stack ? e.stack : e));
});
