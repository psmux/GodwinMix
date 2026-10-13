// What feeds a channel: the words a card and a row say about it, Fed by on
// a card, the programme picked first on Add Channel, and the output the
// programme feeds a channel through, named for it under Outputs.

import { ChannelStub, liveStream } from "./channels-stub.js";
import { panelOver } from "./channels-programme.js";

const wait = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const dialogs = () => [...document.querySelectorAll(".dialog")];
const top = () => dialogs().at(-1);
const button = (root, text) => [...root.querySelectorAll("button")].find((b) => b.textContent.trim() === text);
const closeAll = () => { for (const d of dialogs()) d.parentElement.remove(); };

function channel(feed, over = {}) {
  return { id: "live", name: "Live", enabled: true, streams: [], destinations: [], feed, ...over };
}

export async function feedTests(test, eq, ok) {
  const f = await import("../panels/channels/feed.js");

  test("a channel from an older core is an encoder's, and says what it waits for", () => {
    eq(f.feedKind({}), "encoder");
    eq(f.statusOf(channel(undefined)), { state: "offline", label: "Offline", text: "waiting for an encoder" });
    eq(f.statusOf(channel({ kind: "programme" }, { enabled: false })).label, "Off");
  });

  test("a programme or a pull says why it has no picture, in words", () => {
    eq(f.offlineReason(channel({ kind: "programme", error: "show main is starting" })), "waiting for this mixer's programme: show main is starting");
    eq(f.offlineReason(channel({ kind: "pull", uri_host: "rtmp://cam.example:1935", state: "reconnecting", retry_in_ms: 3600, error: "connection refused" })),
      "pulling rtmp://cam.example:1935, trying again in 4 s: connection refused");
    eq(f.offlineReason(channel({ kind: "pull", uri_host: "srt://cam:9000", state: "connecting" })), "pulling srt://cam:9000, connecting");
  });

  test("live is the picture and the sound, as Livebox says it", () => {
    const s = liveStream("main", { video: { codec: "h264", width: 1280, height: 720, fps: 30, kbps: 2500 }, audio: { codec: "aac", channels: 2, sample_rate: 48000, kbps: 96 } });
    eq(f.statusOf(channel({ kind: "pull" }, { streams: [s] })), { state: "live", label: "LIVE", text: "Video 1280x720 2.5 Mb/s | Audio AAC 96 kb/s" });
    eq(f.specLine({ video: { codec: "h264", width: 0, kbps: 0 } }), "Video H.264");
  });

  test("the marks say pull, push, record and convert", () => {
    const dests = [
      { platform: "youtube", enabled: true, rendition: { preset: "youtube-720p30" } },
      { platform: "file", enabled: true },
      { platform: "twitch", enabled: false },
    ];
    eq(f.badgesOf(channel({ kind: "pull", uri_host: "rtmp://a" }, { destinations: dests })).map((b) => b.id), ["pull", "push", "record", "convert"]);
    eq(f.badgesOf(channel({ kind: "encoder" })), []);
  });

  test("a pull needs a whole address; the other two need nothing", () => {
    eq(f.feedParams("live", "programme"), { params: { id: "live", feed: { kind: "programme" } } });
    ok(f.feedParams("live", "pull", "cam.example/live").error.includes("rtmp://"));
    eq(f.feedParams("live", "pull", " srt://cam:9000?streamid=x ").params.feed, { kind: "pull", url: "srt://cam:9000?streamid=x" });
  });

  const { rowOf } = await import("../panels/channels/rows-model.js");
  test("a row says Offline and why, with its marks", () => {
    const r = rowOf(channel({ kind: "pull", uri_host: "rtmp://a", state: "connecting" }));
    eq(r.status, "Offline: pulling rtmp://a, connecting");
    eq(r.marks.map((m) => m.id), ["pull"]);
  });

  const { feedsChannel } = await import("../panels/outputs/panel.js");
  test("Outputs names the output that feeds a channel, and says where it is changed", () => {
    ok(feedsChannel("to-channel-live").includes("Feeds the channel live"), feedsChannel("to-channel-live"));
    eq(feedsChannel("youtube"), "");
  });

  // Fed by on a card: a pull, with its address never shown again.
  const stub = new ChannelStub();
  await stub.call("channel.add", { name: "Relay", app: "relay" });
  const host = await panelOver(stub);
  const card = () => host.querySelector(".chn-card");
  const pick = (kind) => {
    const r = card().querySelector(`.chn-feed input[value=${kind}]`);
    r.checked = true;
    r.dispatchEvent(new Event("change"));
  };
  test("Fed by has the three choices, the encoder picked on an older channel", () => {
    const words = [...card().querySelectorAll(".chn-feedopt")].map((o) => o.textContent);
    eq(words, ["This mixer's programme", "An encoder (OBS, a phone, a camera)", "Pull from a URL"]);
    ok(card().querySelector(".chn-feed input[value=encoder]").checked);
  });
  pick("pull");
  await wait();
  const url = () => card().querySelector(".chn-feedurl");
  test("choosing Pull opens the box and sends nothing yet", () => {
    ok(!url().closest(".chn-feedurlrow").hidden, "no box");
    ok(!stub.calls.some((c) => c.method === "channel.set"));
  });
  url().value = "rtmp://cdn.example:1935/live/sekret-key";
  button(card(), "Pull").click();
  await wait(60);
  test("Pull sends the address once, and the card shows only its host", () => {
    eq(stub.calls.at(-1), { method: "channel.set", params: { id: "relay", feed: { kind: "pull", url: "rtmp://cdn.example:1935/live/sekret-key" } } });
    ok(card().querySelector(".chn-status").textContent.includes("pulling rtmp://cdn.example:1935"), card().querySelector(".chn-status").textContent);
    ok(!document.body.innerHTML.includes("sekret"), "the address is not on the page");
    ok([...card().querySelectorAll(".chn-mark")].some((m) => m.dataset.mark === "pull"), "the pull mark");
  });
  pick("programme");
  await wait(60);
  test("choosing the programme sends it at once", () => {
    eq(stub.calls.at(-1), { method: "channel.set", params: { id: "relay", feed: { kind: "programme" } } });
    ok(url().closest(".chn-feedurlrow").hidden, "the box stayed open");
  });
  host.view.stop();
  host.remove();

  // Add Channel: the programme picked first, and no key to copy for it.
  const { addChannel } = await import("../panels/channels/create.js");
  const fresh = new ChannelStub();
  await addChannel(fresh);
  test("Add Channel picks this mixer's programme first, the other two one press away", () => {
    ok(top().querySelector("input[name=chn-new-feed][value=programme]").checked);
    eq(top().querySelectorAll("input[name=chn-new-feed]").length, 3);
  });
  const name = top().querySelector(".chn-bigin");
  name.value = "Live";
  name.dispatchEvent(new Event("input"));
  button(top(), "Create channel").click();
  await wait(60);
  test("Create sends the programme as its feed and shows no key", () => {
    eq(fresh.calls.find((c) => c.method === "channel.add").params, { name: "Live", app: "live", feed: { kind: "programme" } });
    ok(!dialogs().length, "a key was shown for a channel no encoder needs");
  });
  closeAll();
}
