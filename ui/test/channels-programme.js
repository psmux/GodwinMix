// A platform added to a channel nothing feeds. The report this is for:
// Custom RTMP added to Live, as in Livebox, and the destination waiting for
// ever, because a channel then took its picture only from an encoder. A
// channel is now fed by this mixer's programme, an encoder or a pull, and
// an encoder's channel with none on it offers the programme in one press.

import { ChannelStub, liveStream } from "./channels-stub.js";

const wait = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const dialogs = () => [...document.querySelectorAll(".dialog")];
const top = () => dialogs().at(-1);
const button = (root, text) => [...root.querySelectorAll("button")].find((b) => b.textContent.trim() === text);
const closeAll = () => { for (const d of dialogs()) d.parentElement.remove(); };

export async function panelOver(stub) {
  const { mount } = await import("../panels/channels/panel.js");
  const host = document.createElement("div");
  host.client = stub;
  document.body.appendChild(host);
  mount(host);
  await wait();
  return host;
}

function paste(input, text) {
  input.value = text;
  input.dispatchEvent(new Event("input"));
}

export async function programmeTests(test, eq, ok) {
  // The add form, on a channel an encoder is meant to feed and none does.
  const stub = new ChannelStub();
  await stub.call("channel.add", { name: "Live", app: "live" });
  const host = await panelOver(stub);
  const card = () => host.querySelector(".chn-card");
  [...card().querySelectorAll(".chn-qtile")].find((b) => b.textContent.includes("YouTube")).click();
  await wait();
  test("adding a platform to an encoder's channel with none on it offers the programme first", () => {
    const note = top().querySelector(".chn-addnote");
    ok(note && note.textContent.includes("No encoder is sending to Live yet"), note && note.textContent);
    ok(note.textContent.includes("feed the channel from this mixer's programme"), note.textContent);
    ok(button(top(), "Feed it from the programme and start"), "no way to feed it from here");
  });
  button(top(), "Feed it from the programme and start").click();
  await wait(60);
  test("with no key pasted it asks for the key and changes nothing", () => {
    ok(!stub.calls.some((c) => c.method === "channel.set"), stub.calls.map((c) => c.method).join(", "));
    ok(dialogs().length, "the form stayed");
  });
  paste(top().querySelector("input[type=password]"), "  abcd-efgh ");
  button(top(), "Feed it from the programme and start").click();
  await wait(60);
  test("one press feeds the channel from the programme and adds the platform to it", () => {
    const methods = stub.calls.map((c) => c.method);
    eq(stub.calls.find((c) => c.method === "channel.set").params, { id: "live", feed: { kind: "programme" } });
    eq(stub.calls.at(-1).method, "channel.destination.add", methods.join(", "));
    ok(!methods.includes("output.add"), "nothing under Outputs");
    ok(!dialogs().length, "the form closed");
    ok(!document.body.innerHTML.includes("abcd-efgh"), "the key is not on the page");
  });
  test("a channel fed by the programme says so and never says nothing is sending", () => {
    eq(card().querySelector(".chn-pill").textContent, "Offline");
    ok(card().querySelector(".chn-status").textContent.includes("waiting for this mixer's programme"), card().querySelector(".chn-status").textContent);
    ok(card().querySelector("input[value=programme]").checked, "Fed by shows the programme");
    const note = card().querySelector(".chn-idle");
    ok(!note || note.hidden, "the encoder note is up on a programme channel");
    ok(!card().textContent.includes("Waiting for an encoder"), card().textContent);
  });
  closeAll();

  // A saved tile on an encoder's channel: the note under the strip, one press.
  await stub.call("channel.add", { name: "Hall", app: "hall" });
  await stub.call("channel.destination.add", { id: "hall", platform: "facebook", server: "rtmps://live-api-s.facebook.com:443/rtmp", key: "fb-key", enabled: true });
  await wait();
  const hall = () => [...host.querySelectorAll(".chn-card")].find((c) => c.textContent.includes("Hall"));
  const note = () => hall().querySelector(".chn-idle");
  test("an encoder's channel with a platform waiting says so under the tiles", () => {
    ok(note() && !note().hidden, "no note");
    ok(note().textContent.includes("No encoder is sending to Hall yet"), note().textContent);
    ok(button(note(), "Feed it from this mixer's programme"), note().textContent);
    ok(hall().querySelector(".chn-status").textContent.includes("waiting for an encoder"), hall().querySelector(".chn-status").textContent);
  });
  button(note(), "Feed it from this mixer's programme").click();
  await wait(60);
  test("and the press feeds it from the programme, with the note gone", () => {
    eq(stub.calls.at(-1), { method: "channel.set", params: { id: "hall", feed: { kind: "programme" } } });
    ok(!note() || note().hidden, "the note stayed");
    ok([...hall().querySelectorAll(".chn-mark")].some((m) => m.dataset.mark === "push"), "the push mark");
  });

  // Live, the line says what arrives, as Livebox does.
  stub.channels.get("hall").streams = [liveStream("programme", { video: { codec: "h264", width: 1920, height: 1080, fps: 30, kbps: 3000 }, audio: { codec: "aac", channels: 2, sample_rate: 48000, kbps: 128 } })];
  stub.changed("hall");
  await wait();
  test("a live channel shows LIVE with its picture and sound", () => {
    eq(hall().querySelector(".chn-pill").textContent, "LIVE");
    eq(hall().querySelector(".chn-statetext").textContent, "Coming in: Video 1920x1080 3.0 Mb/s | Audio AAC 128 kb/s");
  });
  host.view.stop();
  host.remove();
  closeAll();
}
