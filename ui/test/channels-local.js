// Record and Watch link on a channel's strip: the words the picker uses,
// what Start sends, the tiles while recording and serving, and the watch
// link's card with its Copy, QR code and embed code.

import { ChannelStub } from "./channels-stub.js";

const wait = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const dialogs = () => [...document.querySelectorAll(".dialog")];
const top = () => dialogs().at(-1);
const button = (root, text) => [...root.querySelectorAll("button")].find((b) => b.textContent.trim() === text);
const closeAll = () => { for (const d of dialogs()) d.parentElement.remove(); };

/** Ask for something and wait until it is true, or give up after a second. */
async function until(test, limit = 1000) {
  const end = Date.now() + limit;
  while (!test() && Date.now() < end) await wait(20);
  return test();
}

export async function localDestinationTests(test, eq, ok) {
  const model = await import("../panels/channels/model.js");
  const { localLine, localParams, localPlan } = await import("../panels/channels/local.js");
  const when = new Date(2026, 9, 9, 10, 30, 0);
  const sunday = { id: "sunday-service", name: "Sunday service", streams: [] };

  test("the Record tile names the file it will make, and says it is copied", () => {
    eq(localLine("file", sunday, when), "Record. Saves sunday-service-main-20261009-103000.ts, copied, not converted.");
    const live = { ...sunday, streams: [{ name: "cam2", state: "live" }] };
    eq(model.recordName(live, when), "sunday-service-cam2-20261009-103000.ts", "the stream that is live");
  });

  test("Start sends the platform and, only when given, the folder and the stream", () => {
    eq(localParams("file", sunday, { folder: " D:/Recordings ", stream: "*" }), { id: "sunday-service", platform: "file", enabled: true, server: "D:/Recordings" });
    eq(localParams("hls", sunday, { folder: "ignored", stream: "cam2" }), { id: "sunday-service", platform: "hls", enabled: true, stream: "cam2" });
  });

  test("a recording's tile says its size and length, and a link's its viewers", () => {
    const rec = { platform: "file", enabled: true, state: "live", file: { name: "a.ts", path: "C:/v/a.ts", bytes: 12_345_678, duration_ms: 245_000, open: true } };
    eq(model.tileState(rec), "Recording, 12.3 MB, 4:05");
    eq(localPlan(rec).line, "a.ts");
    eq(localPlan({ ...rec, state: "waiting", file: { ...rec.file, open: false } }).line, "a.ts, 12.3 MB, 4:05", "the last file, once the encoder stops");
    eq(model.tileState({ platform: "hls", enabled: true, state: "live", playback: { viewers: 3 } }), "Live, 3 watching");
    eq(model.tileState({ platform: "hls", enabled: true, state: "live" }), "Live");
    eq(model.fmtBytes(950), "950 B");
    eq(model.fmtBytes(2_500_000_000), "2.5 GB");
  });

  test("the embed code plays the link with the browser's own player or hls.js", () => {
    const code = model.embedCode("http://10.0.0.5:8080/hls/channel/sunday/watch-link/index.m3u8?key=k");
    ok(code.includes('"http://10.0.0.5:8080/hls/channel/sunday/watch-link/index.m3u8?key=k"'), code);
    ok(code.includes("cdn.jsdelivr.net/npm/hls.js@1") && code.includes("<video"), code);
  });

  const stub = new ChannelStub();
  const { mount } = await import("../panels/channels/panel.js");
  const host = document.createElement("div");
  host.client = stub;
  document.body.appendChild(host);
  mount(host);
  await wait();
  await stub.call("channel.add", { name: "Sunday service" });
  host.view.accept(await stub.call("channel.get", { id: "sunday-service" }));
  await wait();
  const card = () => host.querySelector(".chn-card");

  test("an empty strip offers Record and Watch link beside the platforms", () => {
    const quick = [...card().querySelectorAll(".chn-qtile")].map((b) => b.textContent.trim());
    ok(quick.includes("Record") && quick.includes("Watch link"), quick.join(", "));
  });

  [...card().querySelectorAll(".chn-qtile")].find((b) => b.textContent.trim() === "Record").click();
  await wait();
  test("the Record form says what it saves and asks for nothing it does not need", () => {
    ok(top().textContent.includes("Saves sunday-service-main-"), top().textContent);
    ok(!top().querySelector("input[type=password]"), "no key box");
  });
  button(top(), "Start recording").click();
  await wait(60);
  closeAll();
  test("Start recording adds a file destination", () => {
    const sent = stub.calls.filter((c) => c.method === "channel.destination.add").at(-1);
    eq(sent.params, { id: "sunday-service", platform: "file", enabled: true });
  });

  // The listener reports, the station serves the link.
  const c = stub.channel("sunday-service");
  await stub.call("channel.destination.add", { id: "sunday-service", platform: "hls", label: "Watch link" });
  const [rec, link] = c.destinations;
  Object.assign(rec, { label: "Record", state: "live", file: { name: "sunday-service-main-20261009-103000.ts", path: "C:/Videos/GodwinMix/sunday-service-main-20261009-103000.ts", bytes: 3_400_000, duration_ms: 65_000, open: true } });
  Object.assign(link, { state: "live", playback: { master_url_path: "/hls/channel/sunday-service/watch-link/index.m3u8?key=abcdefghijkmnpqrstuvwxyz", dash_url_path: "", viewers: 2 } });
  host.view.accept(await stub.call("channel.get", { id: "sunday-service" }));
  await until(() => card().querySelector(".chn-watch textarea"));

  test("the Record tile shows the file, its size and how long it has run", () => {
    const tile = [...card().querySelectorAll(".chn-tile")].find((t) => t.textContent.includes("Record"));
    eq(tile.querySelector(".chn-tstate").textContent, "Recording, 3.4 MB, 1:05");
    eq(tile.querySelector(".chn-tplan").textContent, "sunday-service-main-20261009-103000.ts");
    eq(tile.dataset.state, "live");
  });

  test("the watch link has its link with Copy, a QR code and the embed code", () => {
    const watch = card().querySelector(".chn-watch");
    ok(watch.querySelector(".rnd-url").textContent.includes("/hls/channel/sunday-service/watch-link/index.m3u8?key="), watch.querySelector(".rnd-url").textContent);
    ok(button(watch, "Copy"), "Copy");
    ok(watch.querySelector(".rnd-qr svg path"), "a QR code");
    ok(watch.querySelector("textarea").value.includes("hls.js"), "the embed code");
    const tile = [...card().querySelectorAll(".chn-tile")].find((t) => t.textContent.includes("Watch link"));
    eq(tile.querySelector(".chn-tstate").textContent, "Live, 2 watching");
  });

  await stub.call("channel.destination.set", { id: "sunday-service", destination: link.id, enabled: false });
  host.view.accept(await stub.call("channel.get", { id: "sunday-service" }));
  await until(() => card().querySelector(".chn-watches").hidden);
  test("switched off, the link's card goes", () => ok(card().querySelector(".chn-watches").hidden, "hidden"));

  host.view.stop();
  host.remove();
}
