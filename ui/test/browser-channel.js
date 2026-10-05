// The browser channel against a stub of the channel.* methods, and the rows
// the add source picker shows for this browser. No core needed.

import { repairFor, streamNameFor, ensureBrowserChannel, sourceIdFor } from "../panels/sources/browser-channel.js";
import { browserEntries, sameMachine } from "../panels/sources/browser-entry.js";

export async function browserChannelTests(test, eq, ok) {
  test("a channel somebody changed is put back to take this browser, and one that does is left alone", () => {
    eq(repairFor({ id: "browser", enabled: true, auto_source: true, key_mode: "query", protocols: ["whip"] }), null);
    eq(repairFor({ id: "browser", enabled: false, auto_source: false, key_mode: "stream", protocols: ["rtmp"] }),
      { id: "browser", enabled: true, auto_source: true, key_mode: "query", protocols: ["rtmp", "whip"] });
  });

  test("the stream is named after the browser and the system it runs on", () => {
    const mac = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36";
    eq(streamNameFor(mac, "macOS"), "chrome-macos");
    eq(streamNameFor("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:130.0) Gecko/20100101 Firefox/130.0", "Win32"), "firefox-windows");
    eq(streamNameFor("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Version/18.0 Mobile/15E148 Safari/604.1", "iPhone"), "safari-ios");
    eq(streamNameFor("Mozilla/5.0 (Windows NT 10.0) Chrome/140.0 Safari/537.36 Edg/140.0", "Windows"), "edge-windows");
    eq(sourceIdFor({ app: "browser" }, "chrome-macos"), "browser-chrome-macos");
  });

  const calls = [];
  const stub = (answers) => ({
    call: async (method, params) => {
      calls.push([method, params]);
      const a = answers[method];
      if (a instanceof Error) throw a;
      return typeof a === "function" ? a(params) : a;
    },
  });
  const missing = Object.assign(new Error("no channel 'browser'"), { code: -32004 });
  const made = await ensureBrowserChannel(stub({
    "channel.get": missing,
    "channel.add": { channel: { id: "browser", app: "browser" }, key: { id: "key-1", secret: "s3cret" } },
  }));
  test("the first time, the browser channel is made through channel.add with WHIP and auto source on", () => {
    eq(calls.map((c) => c[0]), ["channel.get", "channel.add"]);
    eq(calls[1][1], { name: "Browser", app: "browser", auto_source: true, protocols: ["whip"] });
    eq(made.key, "s3cret");
  });
  calls.length = 0;
  const reused = await ensureBrowserChannel(stub({
    "channel.get": { id: "browser", app: "browser", enabled: true, auto_source: true, protocols: ["rtmp"], keys: [{ id: "key-1" }] },
    "channel.set": (p) => ({ id: "browser", app: "browser", enabled: true, auto_source: true, protocols: p.protocols, keys: [{ id: "key-1" }] }),
    "channel.key.reveal": { secret: "again" },
  }));
  test("after that it is reused, WHIP switched back on, and the key read back with channel.key.reveal", () => {
    eq(calls.map((c) => c[0]), ["channel.get", "channel.set", "channel.key.reveal"]);
    eq(calls[2][1], { id: "browser", key: "key-1" });
    eq(reused.key, "again");
  });

  test("a key the secret store has lost is replaced by a new one rather than failing", async () => {
    const calls = [];
    const client = {
      call: async (m, p) => {
        calls.push(m);
        if (m === "channel.get") return { id: "browser", app: "browser", enabled: true, auto_source: true, key_mode: "query", protocols: ["whip"], keys: [{ id: "key-1" }] };
        if (m === "channel.key.reveal") throw Object.assign(new Error("not in the secret store"), { code: -32010, data: { reason: "unsealed" } });
        if (m === "channel.key.add") return { key: { secret: "new" } };
        throw new Error("unexpected " + m);
      },
    };
    const got = await ensureBrowserChannel(client);
    eq(got.key, "new");
    eq(calls, ["channel.get", "channel.key.reveal", "channel.key.add"]);
  });

  test("the add source picker offers this browser under Cameras and under Microphones only, and a phone under Cameras", () => {
    const none = { cameras: [], mics: [] };
    eq(browserEntries({}, "cameras", {}, none).map((e) => e.name), ["This browser's camera", "A phone's camera"]);
    eq(browserEntries({}, "cameras", {}, none)[1].label, "Show code");
    eq(browserEntries({}, "audio", {}, none).map((e) => e.name), ["This browser's microphone"]);
    eq(browserEntries({}, "screens", {}, none), []);
  });

  test("this browser's rows add, as a camera on the mixer does, rather than open something", () => {
    eq(browserEntries({}, "cameras", { onAdded() {} })[0].label, "Add");
    eq(browserEntries({}, "audio", { onExisting() {} })[0].label, "Add");
  });

  test("once allowed a camera, the picker lists this browser's devices by name, as it does the mixer's", () => {
    const devices = { cameras: [{ id: "c1", label: "FaceTime HD" }, { id: "c2", label: "Logitech C920" }], mics: [{ id: "m1", label: "MacBook Pro Microphone" }] };
    eq(browserEntries({}, "cameras", {}, devices).map((e) => e.name), ["FaceTime HD (this browser)", "Logitech C920 (this browser)", "A phone's camera"]);
    eq(browserEntries({}, "audio", {}, devices).map((e) => e.name), ["MacBook Pro Microphone (this browser)"]);
  });

  test("on the mixer's own computer, a browser row warns that the device may be listed twice", () => {
    ok(sameMachine("localhost") && sameMachine("127.0.0.1"), "localhost is the same machine");
    ok(!sameMachine("192.168.1.20"), "another address is not");
  });
}
