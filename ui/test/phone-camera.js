// A phone's camera: its name on the mixer, the code the mixer shows, the
// flip, the wake lock and the size it asks for. No camera, no core, no lock:
// each part takes what it reads as an argument.

import { slugify, deviceName, rename, suffix } from "../join/name.js";
import { lanAddress, phoneLink, unreachable } from "../panels/sources/phone-camera.js";
import { flipPlan, nextCamera, otherFacing } from "../join/flip.js";
import { ScreenAwake } from "../join/wake.js";
import { videoConstraints } from "../join/devices.js";
import { parseLink, whipUrl } from "../join/page.js";

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

function memory() {
  const box = new Map();
  return { getItem: (k) => box.get(k) ?? null, setItem: (k, v) => box.set(k, v) };
}

export async function phoneCameraTests(test, eq, ok) {
  test("a typed name becomes a stream name: lower case, hyphens, no apostrophe", () => {
    eq(slugify("Ana's phone"), "anas-phone");
    eq(slugify("  Pulpit  Cam #2 "), "pulpit-cam-2");
    eq(slugify("Café Señor"), "cafe-senor");
    eq(slugify("!!!"), "");
    ok(slugify("x".repeat(60)).length <= 40, "at most 40 characters");
  });

  test("two of the same phone get different names, and each keeps its own", () => {
    const a = memory();
    const b = memory();
    const first = deviceName(a, () => "safari-ios-k3f9");
    const other = deviceName(b, () => "safari-ios-m7qd");
    eq(first.stream, "safari-ios-k3f9");
    ok(first.stream !== other.stream, "the suffix tells them apart");
    eq(deviceName(a, () => "never-used").stream, "safari-ios-k3f9", "the same phone comes back as the same source");
    ok(/^[a-z2-9]{4}$/.test(suffix()), suffix());
  });

  test("a person's name for the phone replaces the made up one, and an empty one changes nothing", () => {
    const s = memory();
    deviceName(s, () => "chrome-android-ab12");
    eq(rename("Ana's phone", s), { stream: "anas-phone", label: "Ana's phone" });
    eq(deviceName(s).stream, "anas-phone");
    eq(rename("   ", s).stream, "anas-phone");
  });

  test("the phone link names the channel, not a stream, and carries the key in the fragment", () => {
    const link = phoneLink("https://192.168.1.20:8080/", "browser", "k3y");
    eq(link, "https://192.168.1.20:8080/join/#channel=browser&key=k3y");
    eq(parseLink(new URL(link).hash), { url: "", key: "k3y", title: "", channel: "browser" });
    eq(whipUrl("browser", "anas-phone"), "/whip/browser/anas-phone");
    eq(parseLink("#whip=%2Fwhip%2Fbrowser%2Fcam&key=abc&title=Pulpit"), { url: "/whip/browser/cam", key: "abc", title: "Pulpit", channel: "" });
  });

  test("the code uses the mixer's LAN https address, never localhost", () => {
    const tls = { urls: ["https://192.168.1.20:8080/", "https://studio.local:8080/", "https://localhost:8080/"] };
    eq(lanAddress({ tls }), "https://192.168.1.20:8080/");
    eq(lanAddress({ tls: { urls: ["https://localhost:8080/"] } }), "");
    eq(unreachable({ tls }), "");
  });

  test("a mixer a phone cannot reach says how to fix it rather than show a code", () => {
    ok(/only listens on this computer/.test(unreachable({ tls: { urls: ["https://localhost:8080/"] } })), "loopback");
    ok(/Let other devices on this network connect/.test(unreachable({ tls: { urls: [] } })), "names the desktop setting");
    ok(/HTTPS is off/.test(unreachable({ tls: null })), "no https");
  });

  test("the flip asks for the other side first, then the next camera in the list", () => {
    const cams = [{ id: "front" }, { id: "back" }, { id: "wide" }];
    const facing = { getSettings: () => ({ facingMode: "user", deviceId: "front" }) };
    eq(flipPlan(facing, cams).plan, [{ facing: "environment" }, { deviceId: "back" }]);
    const laptop = { getSettings: () => ({ deviceId: "wide" }) };
    eq(flipPlan(laptop, cams).plan, [{ deviceId: "front" }], "round to the first");
    eq(nextCamera([{ id: "only" }], "only"), null);
    eq(otherFacing("environment"), "user");
  });

  test("a phone held upright is asked for 720x1280, and a flip names a side, not a device", () => {
    const tall = videoConstraints("c1", false, false, "", true);
    eq([tall.width.exact, tall.height.exact], [720, 1280]);
    const flip = videoConstraints("c1", true, false, "environment", false);
    eq(flip.facingMode, { exact: "environment" });
    eq(flip.deviceId, undefined);
  });

  // The wake lock against a fake: taken when live, let go when stopped,
  // and taken again when the page comes back, since a hidden page loses it.
  const calls = [];
  const doc = Object.assign(new EventTarget(), { visibilityState: "visible" });
  const nav = {
    wakeLock: {
      request: async (type) => {
        calls.push("request " + type);
        const lock = new EventTarget();
        lock.release = async () => { calls.push("release"); lock.dispatchEvent(new Event("release")); };
        return lock;
      },
    },
  };
  const awake = new ScreenAwake(nav, doc);
  await awake.want(true);
  const held = !!awake.lock;
  await awake.lock.release();
  doc.dispatchEvent(new Event("visibilitychange"));
  await wait(0);
  const again = !!awake.lock;
  awake.want(false);
  await wait(0);
  test("the screen is kept on while live, again after the page comes back, and let go on stop", () => {
    ok(held, "taken when live");
    ok(again, "taken again on visibilitychange");
    eq(calls, ["request screen", "release", "request screen", "release"]);
    eq(awake.lock, null);
    ok(!new ScreenAwake({}, null).possible, "a browser with no wake lock is no error");
  });
  awake.destroy();
}
