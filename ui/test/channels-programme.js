// A platform added to a channel nothing is sending to. The report this is
// for: YouTube added to Live, the key pasted, Start sending pressed, and
// YouTube Studio saying "No data" with nothing on this page to say why. A
// channel passes on an encoder's stream; the programme goes out under
// Outputs. The page now says so, and offers the programme instead.

import { ChannelStub, liveStream } from "./channels-stub.js";

const wait = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const dialogs = () => [...document.querySelectorAll(".dialog")];
const top = () => dialogs().at(-1);
const button = (root, text) => [...root.querySelectorAll("button")].find((b) => b.textContent.trim() === text);
const closeAll = () => { for (const d of dialogs()) d.parentElement.remove(); };

async function panelOver(stub) {
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
  const { programmeParams, freeOutputId } = await import("../panels/channels/to-programme.js");
  const { platform } = await import("../client/destinations.js");

  test("the programme's address is the platform's server and the key, under a free id", () => {
    const client = { state: { outputs: [{ id: "youtube" }] } };
    eq(freeOutputId(client, "youtube"), "youtube-2");
    eq(programmeParams(client, platform("youtube"), "", " abcd \n").params, { id: "youtube-2", uri: "rtmp://a.rtmp.youtube.com/live2/abcd", policy: "cdn" });
    eq(programmeParams(client, platform("youtube"), "", "").error, "Paste the stream key.");
    ok(programmeParams(client, platform("custom"), "127.0.0.1:1935/live", "k").error.includes("rtmp://"), "a server with no scheme is turned back here");
  });

  // The add form, on a channel with no encoder: the note, and one press.
  const stub = new ChannelStub();
  await stub.call("channel.add", { name: "Live", app: "live" });
  const host = await panelOver(stub);
  const card = () => host.querySelector(".chn-card");
  [...card().querySelectorAll(".chn-qtile")].find((b) => b.textContent.includes("YouTube")).click();
  await wait();
  test("adding a platform to a channel with no encoder says what a channel is for", () => {
    const note = top().querySelector(".chn-addnote");
    ok(note && note.textContent.includes("Nothing is sending to Live yet"), note && note.textContent);
    ok(note.textContent.includes("send the programme"), note.textContent);
    ok(button(top(), "Send the programme instead"), "no way to send the programme from here");
  });
  paste(top().querySelector("input[type=password]"), "  abcd-efgh ");
  button(top(), "Send the programme instead").click();
  await wait(60);
  test("Send the programme instead adds an output with the same server and key, and nothing to the channel", () => {
    const calls = stub.calls.map((c) => c.method);
    ok(!calls.includes("channel.destination.add"), calls.join(", "));
    eq(stub.calls.at(-1), { method: "output.add", params: { id: "youtube", uri: "rtmp://a.rtmp.youtube.com/live2/abcd-efgh", policy: "cdn" } });
    ok(!dialogs().length, "the form closed");
    ok(!document.body.innerHTML.includes("abcd-efgh"), "the key is not on the page");
  });
  closeAll();

  // A tile already saved on the channel: the note under the strip, and the
  // key once more, because the channel never hands one back.
  await stub.call("channel.destination.add", { id: "live", platform: "facebook", server: "rtmps://live-api-s.facebook.com:443/rtmp", key: "fb-key", enabled: true });
  await wait();
  const note = () => card().querySelector(".chn-idle");
  test("a channel with a platform waiting and no encoder says so under the tiles", () => {
    ok(note() && !note().hidden, "no note");
    ok(note().textContent.includes("A channel passes on what an encoder"), note().textContent);
    ok(button(note(), "Send the programme to Facebook instead"), note().textContent);
  });
  button(note(), "Send the programme to Facebook instead").click();
  await wait();
  test("moving a saved platform asks for the key again and says why", () => {
    ok(top().textContent.includes("cannot hand this one back"), top().textContent);
    ok(top().querySelector("input[type=password]"), "no key box");
  });
  paste(top().querySelector("input[type=password]"), "fb-key");
  button(top(), "Send the programme").click();
  await wait(60);
  test("and then adds the output and takes the platform off the channel", () => {
    const [add, remove] = stub.calls.slice(-2);
    eq(add, { method: "output.add", params: { id: "facebook", uri: "rtmps://live-api-s.facebook.com:443/rtmp/fb-key", policy: "cdn" } });
    eq(remove, { method: "channel.destination.remove", params: { id: "live", destination: "facebook" } });
    ok(!note() || note().hidden, "the note stayed with nothing left waiting");
  });
  closeAll();

  // Live, the note has nothing to say: the platform is getting the stream.
  await stub.call("channel.destination.add", { id: "live", platform: "twitch", server: "rtmp://live.twitch.tv/app", key: "tw", enabled: true });
  stub.channels.get("live").streams = [liveStream("main")];
  stub.changed("live");
  await wait();
  test("a channel with an encoder on it shows no note", () => ok(!note() || note().hidden, "the note is up over a live channel"));
  host.view.stop();
  host.remove();
}
