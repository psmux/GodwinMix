// A new mixer's Channels panel: it opens on the default channel, `live`, with
// its Connect section unfolded, and says in plain text which port is open.
// With more than one channel, or one already live, it opens folded as before.

import { ChannelStub, liveStream } from "./channels-stub.js";

const wait = (ms = 30) => new Promise((r) => setTimeout(r, ms));

async function panelOver(stub) {
  const { mount } = await import("../panels/channels/panel.js");
  const host = document.createElement("div");
  host.client = stub;
  document.body.appendChild(host);
  mount(host);
  await wait();
  return host;
}

export async function defaultChannelTests(test, eq, ok) {
  const stub = new ChannelStub();
  await stub.call("channel.add", { name: "Live", app: "live" });
  const host = await panelOver(stub);
  const box = host.querySelector(".chn-connectbox");
  test("a mixer with only its default channel opens on that channel's Connect", () => {
    ok(box && !box.hidden, "Connect is open");
    eq(host.querySelector(".chn-connect").getAttribute("aria-expanded"), "true");
    const codes = [...box.querySelectorAll(".chn-obsval code")].map((c) => c.textContent);
    eq(codes[0], "rtmp://10.0.0.5:1935/live", "the server, ready to copy");
    ok(box.querySelectorAll(".chn-copybtn, button").length >= 3, "with its Copy buttons");
  });
  test("the ports line says plainly what is open, and nothing in it is a warning", () => {
    const line = host.querySelector(".chn-ports");
    eq(line.textContent.trim(), "Open ports: RTMP 1935 for live");
    eq(line.querySelector(".bad").textContent, "");
    ok(!/when a channel|until/.test(line.textContent), "no sentence about ports waiting for a channel");
  });
  host.view.stop();
  host.remove();

  const two = new ChannelStub();
  await two.call("channel.add", { name: "Live", app: "live" });
  await two.call("channel.add", { name: "Sunday", app: "sunday" });
  const other = await panelOver(two);
  test("with two channels the panel opens with every Connect folded", () => {
    ok([...other.querySelectorAll(".chn-connectbox")].every((b) => b.hidden));
  });
  other.view.stop();
  other.remove();

  const busy = new ChannelStub();
  await busy.call("channel.add", { name: "Live", app: "live" });
  busy.channels.get("live").streams = [liveStream("main")];
  const onAir = await panelOver(busy);
  test("and one already live opens on its streams, not on Connect", () => {
    ok(onAir.querySelector(".chn-connectbox").hidden);
  });
  onAir.view.stop();
  onAir.remove();

  // A mixer with no channels, then the Livebox box makes one: the listener
  // opens the RTMP port a moment later, and only channel.list says so.
  const empty = new ChannelStub();
  const fresh = await panelOver(empty);
  const made = await empty.call("channel.add", { name: "Church", app: "Church" });
  fresh.view.accept(made.channel);
  const before = fresh.querySelector(".chn-ports").textContent.trim();
  fresh.view.portsLater(5);
  await wait(60);
  test("the ports line reads the list again after a channel is made, rather than saying no port is open", () => {
    eq(before, "No port is open for encoders.", "what the first, empty list said");
    eq(fresh.querySelector(".chn-ports").textContent.trim(), "Open ports: RTMP 1935 for church");
  });
  fresh.style.width = "620px";
  await wait(150);
  test("in a narrow panel the Cards and Rows switch keeps its width and the lede gives way", () => {
    const seg = fresh.querySelector(".chn-top > .chn-viewas");
    eq(getComputedStyle(seg).flexShrink, "0");
    const rows = [...seg.querySelectorAll("button")].at(-1);
    ok(rows.scrollWidth <= rows.clientWidth + 1, `Rows is drawn whole (${rows.scrollWidth} of ${rows.clientWidth})`);
  });
  fresh.view.stop();
  fresh.remove();
}
