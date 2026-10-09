// Installing the page as an app: when the Install app row shows and in which
// form, when the service worker is registered at all, and which requests the
// worker answers. The last is the one that matters during a show: the worker
// must leave the protocol and every media stream alone.

import { installWay, wantsWorker, environment, listenForOffer, promptInstall, onInstallChange } from "../shell/install.js";
import { installRow } from "../shell/phone-more.js";
import { route, NEVER } from "../sw.js";
import { authorityLinks, trustNote, TRUST_HELP } from "../shell/trust.js";

const ENV = { desktop: false, standalone: false, ios: false, offer: false, secure: true, worker: true, loopback: false, authority: false };
const SCOPE = "http://mixer.test:8080/";

function req(path, more = {}) {
  return { method: "GET", url: new URL(path, more.scope || SCOPE).href, mode: "cors", headers: new Headers(), ...more };
}

function fakeWindow(ua, extra = {}) {
  return {
    navigator: { userAgent: ua, platform: extra.platform || "", maxTouchPoints: extra.touch || 0, standalone: extra.standalone },
    matchMedia: () => ({ matches: !!extra.display }),
    isSecureContext: extra.secure !== false,
  };
}

export async function installTests(test, eq, ok) {
  test("Install app shows for an offer, says Share on iOS, and is gone once installed or in the desktop app", () => {
    eq(installWay({ ...ENV, offer: true }), "prompt");
    eq(installWay({ ...ENV, ios: true }), "ios");
    eq(installWay({ ...ENV, ios: true, offer: true }), "prompt", "an iOS browser that offers is taken at its word");
    eq(installWay(ENV), null, "no offer and not iOS: nothing to press");
    eq(installWay({ ...ENV, offer: true, standalone: true }), null, "already opened as an app");
    eq(installWay({ ...ENV, ios: true, standalone: true }), null);
    eq(installWay({ ...ENV, offer: true, desktop: true }), null, "the desktop app is an app already");
  });

  test("a phone on the network address is pointed at trusting the mixer, once the mixer has an authority", () => {
    eq(installWay({ ...ENV, authority: true }), "trust");
    eq(installWay({ ...ENV, authority: true, loopback: true }), null, "on the mixer's own machine there is nothing to trust");
    eq(installWay({ ...ENV, authority: false }), null, "HTTPS off, or the operator's own certificate");
    eq(installWay({ ...ENV, authority: true, offer: true }), "prompt", "trusted already: the browser offered");
    eq(installWay({ ...ENV, authority: true, ios: true }), "ios", "Add to Home Screen works without it");
    eq(installWay({ ...ENV, authority: true, standalone: true }), null);
  });

  test("the authority is offered as /ca.crt for Apple and /ca.pem for Android, with its fingerprint", () => {
    eq(authorityLinks("https://192.168.1.20:8080"), { apple: "https://192.168.1.20:8080/ca.crt", android: "https://192.168.1.20:8080/ca.pem" });
    eq(authorityLinks("https://m.local:8080/#token=x").apple, "https://m.local:8080/ca.crt");
    const note = trustNote({ tls: { authority: "AB:CD" } }, "https://192.168.1.20:8080/");
    ok(note.textContent.includes("Trust this mixer on your phone"));
    ok(note.textContent.includes("AB:CD"));
    eq([...note.querySelectorAll("a")].map((a) => a.getAttribute("href")), ["https://192.168.1.20:8080/ca.crt", "https://192.168.1.20:8080/ca.pem", TRUST_HELP]);
    eq(trustNote({ tls: { fingerprint: "x" } }, "https://a/"), null, "no authority, no note");
    eq(trustNote({}, "https://a/"), null);
  });

  test("the service worker is registered only in a secure context, outside the desktop app", () => {
    ok(wantsWorker(ENV));
    ok(!wantsWorker({ ...ENV, secure: false }), "http on a LAN address cannot have one");
    ok(!wantsWorker({ ...ENV, worker: false }));
    ok(!wantsWorker({ ...ENV, desktop: true }));
  });

  test("an iPad that calls itself a Mac is still iOS, and a home screen launch counts as installed", () => {
    ok(environment(fakeWindow("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)", { platform: "MacIntel", touch: 5 })).ios);
    ok(!environment(fakeWindow("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)", { platform: "MacIntel" })).ios, "a Mac with no touch screen");
    ok(environment(fakeWindow("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0)", { standalone: true })).standalone);
    ok(environment(fakeWindow("Mozilla/5.0 (Linux; Android 15)", { display: true })).standalone);
    ok(environment(fakeWindow("Mozilla/5.0 GodwinMix-Desktop/0.2.3")).desktop);
    ok(!environment(fakeWindow("x", { secure: false })).secure);
  });

  test("the worker answers for the page and its files and nothing else", () => {
    eq(route(req("/", { mode: "navigate" }), SCOPE), "page");
    eq(route(req("/?token=abc", { mode: "navigate" }), SCOPE), "page");
    eq(route(req("/join/", { mode: "navigate" }), SCOPE), "page");
    for (const path of ["/boot.js", "/shell/phone.js", "/panels/sources/panel.js", "/themes/dark.css", "/icons/icon-192.png", "/manifest.webmanifest", "/join/page.js"]) {
      eq(route(req(path), SCOPE), "file", path);
    }
    eq(route(req("/elsewhere.js"), SCOPE), null, "a path that is not the page's own");
  });

  test("the worker never touches the API, /rpc, the socket, media, metrics or plugins", () => {
    const never = [
      "/api/status", "/api/v1/status", "/api/v1/snapshot/cam", "/rpc", "/rpc/core.info", "/ws", "/mcp", "/metrics",
      "/mjpeg/program", "/mjpeg/item/cam", "/pcm/program", "/opus/program", "/whep/program", "/whep/program/s1",
      "/whip/ch/stream", "/hls/out/master.m3u8", "/plugins/index.json", "/plugins/ndi/ui/panel.js",
      "/presets/x/theme.css", "/test/", "/legacy", "/sw.js",
    ];
    for (const path of never) {
      eq(route(req(path), SCOPE), null, path);
      eq(route(req(path, { mode: "navigate" }), SCOPE), null, path + " opened as a page");
    }
    ok(NEVER.includes("/api") && NEVER.includes("/rpc") && NEVER.includes("/ws"));
    eq(route(req("/index.html", { method: "POST" }), SCOPE), null, "only GET");
    eq(route(req("/", { headers: new Headers({ upgrade: "websocket" }) }), SCOPE), null, "a WebSocket upgrade");
    eq(route(req("http://other.test/boot.js"), SCOPE), null, "another origin");
  });

  test("behind a path prefix the worker answers inside its scope only", () => {
    const scope = "https://proxy.test/mixer/";
    eq(route(req("/mixer/boot.js", { scope }), scope), "file");
    eq(route(req("/mixer/api/status", { scope }), scope), null);
    eq(route(req("/mixer/", { scope, mode: "navigate" }), scope), "page");
    eq(route(req("/boot.js", { scope }), scope), null, "outside the scope");
  });

  test("the row is a button with an offer, a note on iOS, and nothing otherwise", () => {
    let pressed = 0;
    const button = installRow("prompt", () => { pressed += 1; });
    eq(button.tagName, "BUTTON");
    ok(button.textContent.includes("Install app"));
    button.click();
    eq(pressed, 1);
    const note = installRow("ios", () => {});
    eq(note.tagName, "DIV", "a line to read, not a dead button");
    ok(note.textContent.includes("Share, then Add to Home Screen"));
    eq(note.querySelector("button"), null);
    eq(installRow(null, () => {}), null);
    const trust = installRow("trust", () => {});
    eq(trust.tagName, "A", "a link to the steps, not a button");
    eq(trust.getAttribute("href"), TRUST_HELP);
  });

  // The browser's offer, faked: kept, shown once on the press, then gone.
  listenForOffer(window);
  let told = 0;
  const stop = onInstallChange(() => { told += 1; });
  let prompted = 0;
  const offer = new Event("beforeinstallprompt", { cancelable: true });
  offer.prompt = () => { prompted += 1; return Promise.resolve(); };
  offer.userChoice = Promise.resolve({ outcome: "dismissed" });
  window.dispatchEvent(offer);
  const offered = installWay(environment());
  const accepted = await promptInstall();
  const after = installWay(environment());
  stop();
  test("the browser's offer is kept from its own bar, used once by the row, then dropped", () => {
    ok(offer.defaultPrevented, "the browser's bar would cover the tab bar mid show");
    eq(offered, "prompt");
    eq(prompted, 1);
    eq(accepted, false);
    eq(after, null, "an offer is good for one prompt");
    ok(told >= 2, "the More screen hears about the offer and about it going");
  });
}
