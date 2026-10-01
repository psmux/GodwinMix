// This browser's camera: the publisher's parts that need no camera, and the
// browser channel against a stub of channel.*. Nothing here opens a device
// or a socket; a real publish needs a camera and a core with the ingest
// plugin, which this page does not have.

import { orderCodecs, WhipError } from "../join/whip.js";
import { Session, backoff, isFinal } from "../join/session.js";
import { audioConstraints, videoConstraints, remembered, remember, secureProblem, mediaErrorText } from "../join/devices.js";
import { levelOf } from "../join/meter.js";
import { summarise, statsText } from "../join/stats.js";
import { parseLink } from "../join/page.js";
import { fillSelect, paintState, buildForm } from "../join/form.js";
import { browserChannelTests } from "./browser-channel.js";

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

/** A peer connection that changes state when told to. */
function fakePc() {
  const pc = new EventTarget();
  pc.connectionState = "new";
  pc.closed = false;
  pc.close = () => { pc.closed = true; };
  pc.go = (s) => { pc.connectionState = s; pc.dispatchEvent(new Event("connectionstatechange")); };
  return pc;
}

/** connect() answers from a list: a pc, or an error to throw. */
function fakeWhip(answers) {
  const made = [];
  const ended = [];
  const connect = async () => {
    const next = answers.shift();
    if (next instanceof Error) throw next;
    const pc = fakePc();
    const replaced = [];
    const sender = { replaceTrack: async (t) => replaced.push(t) };
    const live = { pc, location: `/whip/browser/x/s${made.length + 1}`, senders: { video: sender, audio: sender }, replaced };
    made.push(live);
    return live;
  };
  return { made, ended, connect, end: (loc) => ended.push(loc) };
}

export async function browserDeviceTests(test, eq, ok) {
  test("H.264 goes first, packetization mode 1 and baseline ahead of the other H.264", () => {
    const codecs = [
      { mimeType: "video/VP8" },
      { mimeType: "video/H264", sdpFmtpLine: "packetization-mode=0;profile-level-id=42e01f" },
      { mimeType: "video/rtx" },
      { mimeType: "video/H264", sdpFmtpLine: "packetization-mode=1;profile-level-id=640c1f" },
      { mimeType: "video/H264", sdpFmtpLine: "packetization-mode=1;profile-level-id=42e01f" },
    ];
    const order = orderCodecs(codecs).map((c) => `${c.mimeType} ${c.sdpFmtpLine || ""}`.trim());
    eq(order[0], "video/H264 packetization-mode=1;profile-level-id=42e01f");
    eq(order[1], "video/H264 packetization-mode=1;profile-level-id=640c1f");
    eq(order.slice(3), ["video/VP8", "video/rtx"], "the rest keep the browser's order");
  });

  test("reconnecting waits 1, 2, 4, 8 seconds and then 15 each time", () => {
    eq([0, 1, 2, 3, 4, 9].map(backoff), [1000, 2000, 4000, 8000, 15000, 15000]);
  });

  test("a wrong key or no such channel stops; a name in use or a mixer restarting tries again", () => {
    ok(isFinal(new WhipError(403, "no")), "403");
    ok(isFinal(new WhipError(404, "no")), "404");
    ok(isFinal(new WhipError(0, "no H.264")), "a browser with no H.264");
    ok(!isFinal(new WhipError(409, "in use")), "409");
    ok(!isFinal(new WhipError(503, "plugin")), "503");
    ok(!isFinal(new TypeError("Failed to fetch")), "the network");
  });

  // One session through its states: connecting, live, lost, reconnecting, live, stopped.
  const states = [];
  const whip = fakeWhip([null, null]);
  const s = new Session({ url: "/whip/browser/x", key: "k", connect: whip.connect, end: whip.end, onChange: (x) => states.push(x) });
  s.start();
  await wait(0);
  whip.made[0].pc.go("connected");
  test("a session says connecting and then live", () => eq(states.map((x) => x.state), ["connecting", "live"]));
  await s.setTrack("video", "new-camera");
  test("a new camera replaces the track on the live sender, with no new offer", () => {
    eq(whip.made[0].replaced, ["new-camera"]);
    eq(whip.made.length, 1);
  });
  whip.made[0].pc.go("failed");
  test("a lost connection is closed, its session deleted, and tried again in a second", () => {
    const last = states.at(-1);
    eq([last.state, last.retryIn], ["reconnecting", 1000]);
    ok(whip.made[0].pc.closed, "the old connection is closed");
    eq(whip.ended, ["/whip/browser/x/s1"]);
    ok(/lost/.test(last.error), last.error);
  });
  await wait(1100);
  whip.made[1] && whip.made[1].pc.go("connected");
  test("it publishes again with the camera it has now, and is live", () => {
    eq(whip.made.length, 2);
    eq(states.at(-1).state, "live");
    eq(states.at(-1).error, "", "the error goes when it is live again");
  });
  s.stop();
  test("Stop deletes the session and says stopped", () => {
    eq(whip.ended, ["/whip/browser/x/s1", "/whip/browser/x/s2"]);
    eq(states.at(-1).state, "stopped");
    ok(!s.active);
  });

  const refused = [];
  const bad = fakeWhip([new WhipError(403, "the key is not one of the channel's keys")]);
  new Session({ url: "/w", key: "x", connect: bad.connect, end: bad.end, onChange: (x) => refused.push(x) }).start();
  await wait(0);
  test("a refused key stops at once, with the mixer's own sentence", () => {
    eq(refused.at(-1).state, "stopped");
    eq(refused.at(-1).error, "the key is not one of the channel's keys");
  });

  test("echo cancellation, noise suppression and gain follow the one switch", () => {
    eq(audioConstraints("", false, true), { echoCancellation: true, noiseSuppression: true, autoGainControl: true });
    eq(audioConstraints("m1", true, false), { echoCancellation: false, noiseSuppression: false, autoGainControl: false, deviceId: { exact: "m1" } });
  });

  test("the camera is asked for 720p at 30, and a remembered one is only a hint", () => {
    const c = videoConstraints("c1", false);
    eq([c.width.ideal, c.height.ideal, c.frameRate.ideal], [1280, 720, 30]);
    eq(c.deviceId, { ideal: "c1" });
    eq(videoConstraints("c1", true).deviceId, { exact: "c1" });
  });

  test("the device choice is remembered, and storage that throws is no error", () => {
    const box = new Map();
    const store = { getItem: (k) => box.get(k) || null, setItem: (k, v) => box.set(k, v) };
    remember("camera", "c2", store);
    remember("mic", "m3", store);
    eq(remembered(store), { camera: "c2", mic: "m3" });
    const broken = { getItem: () => { throw new Error("blocked"); }, setItem: () => { throw new Error("blocked"); } };
    remember("camera", "c2", broken);
    eq(remembered(broken), {});
  });

  test("a page that is not a secure context says so and says what to do", () => {
    const insecure = { navigator: {}, isSecureContext: false, location: { origin: "http://192.168.1.20:8080" } };
    const text = secureProblem(insecure);
    ok(text.includes("http://192.168.1.20:8080"), text);
    ok(text.includes("localhost") && text.includes("https"), text);
    eq(secureProblem({ navigator: { mediaDevices: { getUserMedia() {} } } }), "");
  });

  test("a camera refused or in use is said plainly", () => {
    ok(/not allowed/.test(mediaErrorText({ name: "NotAllowedError" }, "camera")));
    ok(/in use/.test(mediaErrorText({ name: "NotReadableError" }, "camera")));
  });

  test("the meter reads silence as empty and full scale as full", () => {
    eq(levelOf(new Float32Array(256)), 0);
    ok(levelOf(new Float32Array(256).fill(1)) > 0.99);
  });

  test("the stats line adds video and audio bytes and reads the round trip", () => {
    const report = (bytes) => new Map([
      ["v", { type: "outbound-rtp", kind: "video", bytesSent: bytes, frameWidth: 1280, frameHeight: 720, framesPerSecond: 30 }],
      ["a", { type: "outbound-rtp", kind: "audio", bytesSent: 1000 }],
      ["p", { type: "candidate-pair", nominated: true, currentRoundTripTime: 0.012 }],
    ]);
    const first = summarise(report(0), null, 0);
    const next = summarise(report(312500), first, 1000);
    eq(next.kbps, 2500);
    eq(statsText(next), "2500 kbps · 1280x720 at 30 fps · 12 ms round trip");
  });

  test("the /join/ link carries the address, the key and the title in its fragment", () => {
    eq(parseLink("#whip=%2Fwhip%2Fbrowser%2Fcam&key=abc&title=Pulpit"), { url: "/whip/browser/cam", key: "abc", title: "Pulpit" });
    eq(parseLink(""), { url: "", key: "", title: "" });
  });

  test("the device lists keep the choice, and No camera is a choice", () => {
    const r = buildForm();
    fillSelect(r.camera, [{ id: "c1", label: "FaceTime" }, { id: "c2", label: "Brio" }], "c2", "No camera");
    eq([...r.camera.options].map((o) => o.text), ["No camera", "FaceTime", "Brio"]);
    eq(r.camera.value, "c2");
    fillSelect(r.camera, [{ id: "c1", label: "FaceTime" }], "off", "No camera");
    eq(r.camera.value, "off");
  });

  test("the state line counts down a reconnect and Go live becomes Stop", () => {
    const r = buildForm();
    paintState(r, { state: "reconnecting", error: "lost", retryIn: 4000 });
    eq(r.state.textContent, "Reconnecting in 4 s");
    eq(r.go.textContent, "Stop");
    paintState(r, { state: "stopped", error: "", retryIn: 0 });
    eq(r.go.textContent, "Go live");
  });

  await browserChannelTests(test, eq, ok);
}
