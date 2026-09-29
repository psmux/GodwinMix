// The Channels panel against a stub that answers as the real channel.*
// methods do: create, the key shown once, revoke, a destination added and
// switched, an event updating a card, and the numbers read while live. No
// core needed.

import { ChannelStub, liveStream } from "./channels-stub.js";

const wait = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const dialogs = () => [...document.querySelectorAll(".dialog")];
const top = () => dialogs().at(-1);
const button = (root, text) => [...root.querySelectorAll("button")].find((b) => b.textContent.trim() === text);
const closeAll = () => { for (const d of dialogs()) d.parentElement.remove(); };

/** "hi" at level M, as Project Nayuki's reference encoder draws it with mask 2. */
const HI = ["111111100111101111111", "100000100110101000001", "101110101101101011101", "101110101100101011101", "101110101001101011101", "100000101100101000001", "111111101010101111111", "000000001011100000000", "101111100000101111100", "011101010010100100001", "001100110101010011110", "111010000100000110100", "111010100001010010101", "000000001001111001001", "111111100010101100010", "100000101111111001001", "101110101000100100100", "101110101110100100100", "101110101001010011100", "100000100110000110100", "111111101011010011110"];

export async function channelTests(test, eq, ok) {
  const model = await import("../panels/channels/model.js");
  const { qrMatrix } = await import("../panels/channels/qr.js");
  const { platformOfHost, platform } = await import("../client/destinations.js");
  const { addParams } = await import("../panels/channels/destination-form.js");
  const { settingsParams } = await import("../panels/channels/edit.js");
  const { mount, addChannel } = await import("../panels/channels/panel.js");

  test("a channel's name becomes the slug its address uses", () => {
    eq(model.slugify("Sunday Service!"), "sunday-service");
    eq(model.slugify("  Café  Night 2 "), "cafe-night-2");
    eq(model.slugify("***"), "");
  });

  test("the QR code matches the reference encoder, and a long address still fits", () => {
    eq(qrMatrix("hi").map((r) => r.map((b) => (b ? 1 : 0)).join("")), HI);
    const long = "rtmp://192.168.100.200:1935/a-long-channel-name/main?psk=" + "k".repeat(60);
    ok(qrMatrix(long).length >= 37, "a key on the end needs a bigger version");
    eq(qrMatrix("x".repeat(400)), null, "too long says so rather than drawing garbage");
  });

  test("OBS gets the server and the key box as it joins them", () => {
    const c = { app: "sunday", key_mode: "query", publish: { server: "rtmp://a:1935/sunday" } };
    eq(model.obsFields(c, "SECRET", "rtmp://10.0.0.5:1935"), { server: "rtmp://10.0.0.5:1935/sunday", key: "main?psk=SECRET", url: "rtmp://10.0.0.5:1935/sunday/main?psk=SECRET" });
    eq(model.obsFields({ ...c, key_mode: "stream" }, "SECRET", "").key, "SECRET");
  });

  test("Kick and Twitch share an ingest domain and are still told apart", () => {
    eq(platformOfHost("rtmps://fa723fc1b171.global-contribute.live-video.net/…").id, "kick");
    eq(platformOfHost("rtmp://ingest.global-contribute.live-video.net/…").id, "twitch");
    eq(platformOfHost("rtmps://va.pscp.tv:443/…").id, "x");
  });

  test("a platform that hands out an address per stream asks for it", () => {
    const ch = { id: "sun" };
    eq(addParams(platform("instagram"), ch, { key: "k" }).field, "server");
    eq(addParams(platform("youtube"), ch, {}).field, "key");
    eq(addParams(platform("youtube"), ch, { key: " abc \n" }).params, { id: "sun", platform: "youtube", server: "rtmp://a.rtmp.youtube.com/live2", enabled: true, key: "abc" });
    eq(addParams(platform("custom"), ch, { server: "rtmp://relay/live" }).params.key, undefined, "a custom server may have no key");
  });

  test("saving settings sends only what changed", () => {
    const c = { id: "sun", name: "Sun", enabled: true, auto_source: true, key_mode: "query" };
    eq(settingsParams(c, { name: "Sun", enabled: true, auto_source: false, key_mode: "query" }), { id: "sun", auto_source: false });
    eq(settingsParams(c, { name: " ", enabled: true, auto_source: true, key_mode: "stream" }), { id: "sun", key_mode: "stream" });
  });

  const stub = new ChannelStub();
  const host = document.createElement("div");
  host.client = stub;
  document.body.appendChild(host);
  mount(host);
  await wait();
  const view = host.view;

  test("the panel asks for channel events while it is on screen, and gives them back", () => {
    eq(stub.patterns.get("channel.*"), 1);
    ok(host.querySelector(".chn-empty"), "no channels is a picture, not a list");
  });

  // Create, as a person would: type a name, press Create.
  await addChannel(stub);
  const input = document.querySelector(".chn-bigin");
  input.value = "Sunday Service";
  input.dispatchEvent(new Event("input"));
  button(top(), "Create channel").click();
  await wait(60);
  const created = stub.calls.find((c) => c.method === "channel.add");
  const secret = stub.secrets.get("key-1");
  test("Create sends the name and the slug it showed, and the card appears", () => {
    eq(created.params, { name: "Sunday Service", app: "sunday-service" });
    ok(host.querySelector(".chn-card"), "a card");
    eq(host.querySelector(".chn-addr").textContent, "rtmp://10.0.0.5:1935/sunday-service");
  });
  test("the new key is shown large, as OBS asks for it, with a QR code", () => {
    const shown = top().querySelector(".chn-secret");
    eq(shown.textContent, `main?psk=${secret}`);
    ok(top().textContent.includes("shown once"), "it says it is shown once");
    ok(top().querySelector(".chn-qrpic svg path"), "a QR code");
    ok(button(top(), "Make another key"), "and the way to another key");
  });
  closeAll();

  test("once the card is closed the key is nowhere on the page", () => {
    ok(!document.body.innerHTML.includes(secret), "the secret is gone");
  });

  // Revoke asks once, then goes.
  const { editChannel } = await import("../panels/channels/edit.js");
  await stub.call("channel.key.add", { id: "sunday-service", label: "Camera 2" });
  view.accept(await stub.call("channel.get", { id: "sunday-service" }));
  editChannel(view, view.model.byId.get("sunday-service"));
  test("the settings list the keys by label and last four characters only", () => {
    const text = top().textContent;
    ok(text.includes("Camera 2"), "the label");
    ok(!text.includes(stub.secrets.get("key-2")), "never the key");
    ok(text.includes("…" + stub.secrets.get("key-2").slice(-4)), "the hint");
  });
  const rows = [...top().querySelectorAll(".chn-key")];
  button(rows[1], "Revoke").click();
  await wait();
  test("Revoke asks once, naming the key", () => ok(top().textContent.includes("Revoke Camera 2?"), top().textContent));
  button(top(), "Revoke").click();
  await wait(60);
  test("and on yes the key is removed and the list redrawn", () => {
    eq(stub.calls.at(-1), { method: "channel.key.remove", params: { id: "sunday-service", key: "key-2" } });
    eq(top().querySelectorAll(".chn-key").length, 1);
  });
  closeAll();

  // A destination: tile, paste key, done.
  const card = () => host.querySelector(".chn-card");
  [...card().querySelectorAll(".chn-qtile")].find((b) => b.textContent.includes("YouTube")).click();
  await wait();
  const key = top().querySelector("input[type=password]");
  key.value = "  abcd-efgh ";
  key.dispatchEvent(new Event("input"));
  button(top(), "Start sending").click();
  await wait(60);
  test("adding YouTube sends the platform, its ingest and the trimmed key", () => {
    eq(stub.calls.at(-1), { method: "channel.destination.add", params: { id: "sunday-service", platform: "youtube", server: "rtmp://a.rtmp.youtube.com/live2", enabled: true, key: "abcd-efgh" } });
    ok(!dialogs().length, "the form closed");
    const tile = card().querySelector(".chn-tile[data-state]");
    eq(tile.dataset.state, "waiting");
    ok(!document.body.innerHTML.includes("abcd-efgh"), "the key is not on the page");
  });

  const box = card().querySelector(".chn-tile .chn-switch input");
  box.checked = false;
  box.dispatchEvent(new Event("change"));
  await wait(60);
  test("the switch on a tile turns it off without opening anything", () => {
    eq(stub.calls.at(-1), { method: "channel.destination.set", params: { id: "sunday-service", destination: "youtube", enabled: false } });
    eq(card().querySelector(".chn-tile[data-state]").dataset.state, "off");
    ok(!dialogs().length);
  });

  // The core says a publisher arrived; the card follows.
  const tileBefore = card().querySelector(".chn-tile[data-state]");
  const c = stub.channels.get("sunday-service");
  c.streams = [liveStream("main", { source: "sunday-service-main", key: "key-1" })];
  c.destinations[0].enabled = true;
  c.destinations[0].state = "failed";
  c.destinations[0].error = "YouTube refused the key.";
  stub.changed("sunday-service");
  await wait();
  test("event/channel.changed updates the card in place", () => {
    ok(card().classList.contains("live"), "the card is live");
    eq(card().querySelector(".chn-pill").textContent, "Live");
    eq(card().querySelector(".chn-sname").textContent, "main");
    ok(card().querySelector(".chn-stream").textContent.includes("1920×1080"), "the resolution");
    eq(card().querySelector(".chn-feeds").textContent, "sunday-service-main");
    const tile = card().querySelector(".chn-tile[data-state]");
    ok(tile === tileBefore, "the tile is written into, not rebuilt");
    eq(tile.dataset.state, "failed");
    eq(tile.querySelector(".chn-terr").textContent, "YouTube refused the key.");
  });

  // No event carries bit rates, so while something is live the panel reads
  // them every other tick of its clock.
  c.streams[0].video.kbps = 2600;
  c.destinations[0].state = "reconnecting";
  c.destinations[0].error = "nothing answered at rtmp://10.0.0.9:1935";
  const before = stub.calls.filter((x) => x.method === "channel.list").length;
  view.tick();
  view.tick();
  await wait(60);
  test("while live the numbers are read again every two seconds", () => {
    ok(stub.calls.filter((x) => x.method === "channel.list").length > before, "channel.list was read again");
    ok(card().querySelector(".chn-rate").textContent.includes("2.8 Mb/s"), card().querySelector(".chn-rate").textContent);
  });
  test("a tile trying again says why", () => {
    eq(card().querySelector(".chn-tile .chn-terr").textContent, "nothing answered at rtmp://10.0.0.9:1935");
  });

  stub.emit("event", { name: "channel.removed", params: { id: "sunday-service" } });
  await wait();
  test("event/channel.removed takes the card away", () => ok(!card(), "no card"));

  view.stop();
  test("hidden, the panel lets go of its events", () => eq(stub.patterns.size, 0));
  host.remove();
}
