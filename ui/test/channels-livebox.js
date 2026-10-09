// Channels moved from Livebox: the paste box's reading of what is pasted,
// the dialog making each line a channel.add against the stub, the line that
// is already a channel offered its password as a key, and Add Channel's own
// Address and Password boxes. No core.

import { ChannelStub } from "./channels-stub.js";

const wait = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const top = () => [...document.querySelectorAll(".dialog")].at(-1);
const button = (root, text) => [...root.querySelectorAll("button")].find((b) => b.textContent.trim() === text);
const closeAll = () => { for (const d of document.querySelectorAll(".dialog")) d.parentElement.remove(); };

export async function liveboxTests(test, eq, ok) {
  const { parseLivebox } = await import("../panels/channels/livebox-parse.js");
  const { importLivebox } = await import("../panels/channels/livebox.js");
  const { addParams } = await import("../panels/channels/create.js");
  const { keyParams } = await import("../panels/channels/keys.js");

  test("one whole Livebox address a line becomes a channel, its stream and its password", () => {
    eq(parseLivebox("rtmp://192.168.1.10:1935/Church/main?psk=Sunday-2024"), [{ line: 1, app: "Church", stream: "main", secret: "Sunday-2024" }]);
    const two = parseLivebox("  rtmp://h:1935/Church/main?psk=a1b2c3\n\nrtmp://h/Youth%20Hall/cam2?psk=my+hall+pw  ");
    eq(two, [
      { line: 1, app: "Church", stream: "main", secret: "a1b2c3" },
      { line: 3, app: "Youth Hall", stream: "cam2", secret: "my hall pw" },
    ]);
  });

  test("the dashboard's Stream URL and Stream key pasted together are one channel", () => {
    const pasted = "STREAM URL\nrtmp://10.0.0.2:1935/Church/\nSTREAM KEY\nmain?psk=Sunday-2024";
    eq(parseLivebox(pasted), [{ line: 2, app: "Church", stream: "main", secret: "Sunday-2024" }]);
    eq(parseLivebox("Stream URL: rtmp://h:1935/Choir\nStream key: rehearsal?psk=choir-pw"), [{ line: 1, app: "Choir", stream: "rehearsal", secret: "choir-pw" }]);
    eq(parseLivebox("rtmp://h:1935/Church/\nkeynote?psk=abcdef")[0].stream, "keynote", "a stream that starts with 'key' is not a label");
  });

  test("a line that cannot be read says which line and what to fix", () => {
    const [nokey] = parseLivebox("rtmp://h:1935/Church/main");
    eq(nokey.line, 1);
    ok(/Line 1: there is no \?psk= password/.test(nokey.error), nokey.error);
    const [stray] = parseLivebox("hello there");
    ok(stray.error.startsWith("Line 1: this is not an rtmp:// address"), stray.error);
    const [alone] = parseLivebox("rtmp://h:1935/Church/");
    ok(alone.error.includes("no stream key after it"), alone.error);
    eq(parseLivebox(""), []);
  });

  test("Add Channel sends a typed address as typed and a password only when given", () => {
    eq(addParams("Church", "", ""), { name: "Church", app: "church" });
    eq(addParams("Church", " Church ", " Sunday-2024 "), { name: "Church", app: "Church", secret: "Sunday-2024" });
    eq(keyParams("church", "", ""), { id: "church" });
    eq(keyParams("church", "Livebox", "Sunday-2024"), { id: "church", label: "Livebox", secret: "Sunday-2024" });
  });

  // The dialog, as a person would use it.
  const stub = new ChannelStub();
  await stub.call("channel.add", { name: "Church", app: "Church", secret: "old-password" });
  importLivebox(stub);
  const box = top().querySelector("textarea.chn-paste");
  box.value = "rtmp://h:1935/Choir/main?psk=choir-pw\nrtmp://h:1935/church/main?psk=Sunday-2024\nrtmp://h:1935/Bad/main\nrtmp://h:1935/Tiny/main?psk=abc";
  button(top(), "Bring them in").click();
  await wait(80);
  const rows = [...top().querySelectorAll(".chn-results li")];
  const adds = stub.calls.filter((c) => c.method === "channel.add").slice(1);
  test("each line is one channel.add with its address and password, and says what came of it", () => {
    eq(adds.map((c) => c.params), [
      { name: "Choir", app: "Choir", secret: "choir-pw" },
      { name: "church", app: "church", secret: "Sunday-2024" },
      { name: "Tiny", app: "Tiny", secret: "abc" },
    ]);
    eq(rows.length, 4);
    ok(rows[0].textContent.startsWith("Line 1: made Choir"), rows[0].textContent);
    ok(rows[1].textContent.includes("already the channel church"), rows[1].textContent);
    ok(rows[2].classList.contains("bad") && rows[2].textContent.startsWith("Line 3:"), rows[2].textContent);
    ok(rows[3].classList.contains("bad") && rows[3].textContent.includes("too short"), rows[3].textContent);
    ok(!top().textContent.includes("choir-pw") && !top().textContent.includes("Sunday-2024"), "no password is shown back");
    eq(stub.channels.get("choir").keys[0].imported, true);
  });

  button(rows[1], "Add this password to church").click();
  await wait(60);
  const keyAdd = stub.calls.find((c) => c.method === "channel.key.add");
  test("a channel already here is offered the password as a key of its own", () => {
    eq(keyAdd.params, { id: "church", label: "Livebox", secret: "Sunday-2024" });
    eq(stub.channels.get("church").keys.length, 2);
    ok(top().querySelector(".chn-results li:nth-child(2)").textContent.includes("added the password to church"));
  });
  closeAll();

  const { bring } = await import("../panels/channels/livebox.js");
  const hall = await bring(stub, { line: 1, app: "Youth Hall", stream: "cam", secret: "youth-pass-1" });
  test("a channel name with a space is said the way an encoder has to send it", () => {
    ok(hall.textContent.includes("…/Youth%20Hall/cam"), hall.textContent);
  });

  const { addChannel } = await import("../panels/channels/create.js");
  addChannel(stub);
  const typed = top().querySelector("input[placeholder='Church']");
  typed.value = "Choir Loft";
  typed.dispatchEvent(new Event("input", { bubbles: true }));
  test("Add Channel's preview writes a space in the address as %20, as the card after it does", () => {
    ok(top().querySelector(".chn-preview code").textContent.endsWith("/Choir%20Loft"), top().querySelector(".chn-preview code").textContent);
  });
  closeAll();
}
