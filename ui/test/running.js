// What is running: the model a pure function of the status and the channel
// list, the words it asks with, the panel's Stop and Stop all, the banner a
// page opens with, and Stop streaming on an Outputs row. Against a stand in
// client, so nothing is sent anywhere.

import {
  runningThings, confirmWording, stopAllLabel, stopAllWording, summary, longestLive, platformName,
} from "../panels/header/running-model.js";
import { ratesFrom, openRunning } from "../panels/header/running.js";
import { bannerText, showBanner } from "../panels/header/banner.js";
import { destinationsPill } from "../panels/header/destinations.js";
import { fmtDuration } from "../shell/dom.js";

const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));
async function until(predicate, what, ms = 3000) {
  const started = Date.now();
  while (!predicate()) {
    if (Date.now() - started > ms) throw new Error(`timed out waiting for ${what}`);
    await tick(20);
  }
}

const YOUTUBE = { id: "youtube", uri_host: "rtmp://a.rtmp.youtube.com/…", has_key: true, state: "live", reconnects: 0, queue_secs: 0.2, live_secs: 6983, bytes_out: 1000 };
const RECORDING = { id: "recording-1", type: "record/output", state: "live", recording_secs: 65, has_key: true };
const STOPPED = { id: "facebook", uri_host: "rtmps://live-api-s.facebook.com:443/…", has_key: true, state: "stopped" };
const NO_KEY = { id: "twitch", uri_host: "rtmp://live.twitch.tv/…", has_key: false, state: "reconnecting" };
const CHANNEL = {
  id: "church", name: "Church", enabled: true,
  streams: [{ name: "main", state: "live", since_ms: 1000000, from: "192.168.1.20" }],
  destinations: [
    { id: "youtube", platform: "youtube", label: "YouTube", uri_host: "rtmp://a.rtmp.youtube.com", has_key: true, enabled: true, state: "live", since_ms: 61000, kbps: 4500 },
    { id: "kick", platform: "custom", label: "Kick", enabled: false, state: "off", since_ms: 0, kbps: 0 },
    { id: "watch", platform: "hls", label: "watch", enabled: true, state: "waiting", since_ms: 0, kbps: 0 },
  ],
};

function standIn(outputs) {
  const client = {
    sent: [],
    state: { connected: true, outputs },
    call: async (method, params) => {
      client.sent.push([method, params]);
      if (method === "output.stop") client.state = { ...client.state, outputs: client.state.outputs.map((o) => (o.id === params.id ? { ...o, state: "stopped" } : o)) };
      if (method === "output.remove") client.state = { ...client.state, outputs: client.state.outputs.filter((o) => o.id !== params.id) };
      if (method === "channel.list") return { channels: [] };
      return {};
    },
    on: () => () => {},
    onRender: () => () => {},
  };
  return client;
}

const dialogs = () => [...document.querySelectorAll(".dialog")];
const titled = (t) => dialogs().find((d) => d.getAttribute("aria-label") === t);
const press = (root, text) => [...root.querySelectorAll("button")].find((b) => b.textContent === text).click();

export async function runningTests(test, eq, ok) {
  document.querySelectorAll(".still-running").forEach((b) => b.remove());

  test("the model lists what streams, records and receives, and leaves out what is stopped", () => {
    const items = runningThings({ outputs: [YOUTUBE, RECORDING, STOPPED, NO_KEY] }, [CHANNEL], { youtube: 4400 }, 1000000 + 90000);
    eq(items.map((i) => i.key), ["output:youtube", "output:recording-1", "channel:church#youtube", "ingest:church"]);
    const [yt, rec, sent, ingest] = items;
    eq(yt.title, "YouTube");
    eq(yt.since_secs, 6983);
    eq(yt.kbps, 4400);
    eq(yt.stop, { method: "output.stop", params: { id: "youtube" } });
    eq(rec.stop, { method: "output.remove", params: { id: "recording-1" } });
    eq(sent.where, "channel Church");
    eq(sent.since_secs, 61);
    eq(sent.stop, { method: "channel.destination.set", params: { id: "church", destination: "youtube", enabled: false } });
    eq(ingest.since_secs, 90);
    eq(ingest.stop, { method: "channel.set", params: { id: "church", enabled: false } });
    eq(runningThings({ outputs: [] }, []), []);
  });

  test("a platform is named from its address, and anything else by its own name", () => {
    eq(platformName("rtmp://a.rtmp.youtube.com/…", "yt"), "YouTube");
    eq(platformName("rtmps://live-api-s.facebook.com:443/…", "fb"), "Facebook");
    eq(platformName("rtmp://127.0.0.1:1935/…", "church-relay"), "church-relay");
  });

  test("a live stream is stopped only after the question, in plain words", () => {
    const [yt, rec] = runningThings({ outputs: [YOUTUBE, RECORDING] });
    eq(confirmWording(yt), { title: "Stop streaming to YouTube?", body: "Viewers see the stream end.", yes: "Stop streaming" });
    eq(confirmWording(rec).title, "Stop recording?");
    const dialling = runningThings({ outputs: [{ ...YOUTUBE, state: "connecting", live_secs: undefined }] })[0];
    eq(confirmWording(dialling), null, "nothing live, nothing to ask");
  });

  test("Stop all says what it stops, and keeps the keys", () => {
    const items = runningThings({ outputs: [YOUTUBE, RECORDING] }, [CHANNEL]);
    eq(stopAllLabel(items), "Stop all streaming and recording");
    eq(stopAllLabel(runningThings({ outputs: [YOUTUBE] })), "Stop all streaming");
    const w = stopAllWording(items);
    ok(w.body.startsWith("Viewers of YouTube see the stream end."), w.body);
    ok(w.body.includes("The recording is finished"), w.body);
    ok(w.body.includes("keeps its stream key"), w.body);
  });

  test("the banner reads like a sentence and the clock is the longest live thing", () => {
    const items = runningThings({ outputs: [YOUTUBE, RECORDING] });
    eq(bannerText(items), "Still streaming from before: YouTube for 1:56:23, recording for 1:05");
    eq(summary(items, fmtDuration), "YouTube for 1:56:23, recording for 1:05");
    eq(bannerText(runningThings({ outputs: [RECORDING] })), "Still recording from before: recording for 1:05");
    eq(longestLive(items), 6983);
  });

  test("a stopped destination is not connecting, failing or live in the header", () => {
    const pill = destinationsPill([STOPPED]);
    eq(pill.text, "Streaming stopped");
    eq(pill.kind, "none");
    eq(destinationsPill([STOPPED, YOUTUBE]).text, "1 destination live");
  });

  test("bit rates come from how far bytes_out moved", () => {
    const seen = new Map();
    eq(ratesFrom([YOUTUBE], seen, 1000), {});
    eq(ratesFrom([{ ...YOUTUBE, bytes_out: 1000 + 562500 }], seen, 2000), { youtube: 4500 });
  });

  // The panel: Keep it running sends nothing, Stop sends output.stop.
  const client = standIn([YOUTUBE, RECORDING]);
  const panel = openRunning(client);
  await until(() => panel.el.querySelectorAll(".running-row").length === 2, "two rows");
  test("What is running lists each thing with a Stop and a Stop all", () => {
    const rows = [...panel.el.querySelectorAll(".running-row")];
    ok(rows[0].textContent.includes("YouTube") && rows[0].textContent.includes("for 1:56:23"), rows[0].textContent);
    ok(panel.el.textContent.includes("Stop all streaming and recording"));
  });
  panel.el.querySelector('[aria-label="Stop YouTube"]').click();
  await until(() => titled("Stop streaming to YouTube?"), "the question");
  press(titled("Stop streaming to YouTube?"), "Keep it running");
  await tick();
  test("Keep it running stops nothing", () => eq(client.sent.filter(([m]) => m === "output.stop"), []));
  panel.el.querySelector('[aria-label="Stop YouTube"]').click();
  await until(() => titled("Stop streaming to YouTube?"), "the question again");
  press(titled("Stop streaming to YouTube?"), "Stop streaming");
  await until(() => client.sent.some(([m]) => m === "output.stop"), "output.stop");
  await until(() => panel.el.querySelectorAll(".running-row").length === 1, "one row left");
  test("Stop sends output.stop, which keeps the key, and the row goes", () => {
    eq(client.sent.find(([m]) => m === "output.stop"), ["output.stop", { id: "youtube" }]);
    eq(client.sent.filter(([m]) => m === "output.remove"), [], "the destination is not removed");
  });
  panel.close();

  // The banner's Stop all asks once and stops everything outgoing.
  const left = standIn([YOUTUBE, RECORDING]);
  const bar = showBanner(left, runningThings(left.state));
  test("the banner is at the top of the page and says what is still running", () => {
    ok(bar.isConnected && bar.textContent.includes("Still streaming from before: YouTube for 1:56:23"), bar.textContent);
  });
  press(bar, "Stop all");
  await until(() => titled("Stop all streaming and recording?"), "the Stop all question");
  press(titled("Stop all streaming and recording?"), "Stop all streaming and recording");
  await until(() => !bar.isConnected, "the banner to go");
  test("Stop all on the banner stops the stream and the recording", () => {
    eq(left.sent.map(([m]) => m).filter((m) => m !== "channel.list"), ["output.stop", "output.remove"]);
  });

  // Stop streaming and Start streaming on an Outputs row.
  window.godwinmixPanels ||= [];
  await import("../panels/outputs/panel.js");
  const rows = standIn([STOPPED]);
  const outputs = document.createElement("gmx-outputs");
  outputs.setClient(rows);
  document.body.append(outputs);
  test("a stopped destination says so and offers Start streaming, not Reconnect", () => {
    ok(outputs.textContent.includes("Stopped, stream key kept"), outputs.textContent);
    const buttons = [...outputs.querySelectorAll("button")].map((b) => b.textContent);
    ok(buttons.includes("Start streaming") && !buttons.includes("Reconnect"), buttons.join(","));
  });
  press(outputs, "Start streaming");
  await until(() => rows.sent.length, "output.start");
  test("Start streaming sends output.start", () => eq(rows.sent[0], ["output.start", { id: "facebook" }]));
  outputs.remove();
}
