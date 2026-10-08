// Help > Open on another device, against a stub of core.info and token.*,
// and the page taking a token out of its own address.

import { lanUrls, signInUrl, openDevices } from "../shell/devices.js";
import { takeTokenFromAddress, storedToken, storeToken } from "../client/index.js";

const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));

async function until(predicate, ms, what) {
  const started = Date.now();
  while (!predicate()) {
    if (Date.now() - started > ms) throw new Error(`timed out waiting for ${what}`);
    await tick(20);
  }
}

/** A client that answers core.info with `tls` and keeps device tokens in a list. */
function stub(tls, share = { restarting: true, how: "supervised", message: "" }) {
  const devices = [];
  const calls = [];
  const listeners = new Map();
  const client = {
    calls,
    devices,
    tls,
    started: 1000,
    on: (name, fn) => {
      if (!listeners.has(name)) listeners.set(name, new Set());
      listeners.get(name).add(fn);
      return () => listeners.get(name).delete(fn);
    },
    emit: (name) => [...(listeners.get(name) || [])].forEach((fn) => fn()),
    call: async (method, params) => {
      calls.push([method, params]);
      if (method === "core.info") return { version: "test", tls: client.tls, started_ms: client.started };
      if (method === "network.share") return share;
      if (method === "token.create") {
        const d = { id: "phone", label: params.label, scope: params.scope, created: "2026-10-05T10:00:00Z" };
        devices.push(d);
        return { ...d, token: "s3cret/+=" };
      }
      if (method === "token.list") return { tokens: devices.slice() };
      if (method === "token.revoke") return { revoked: devices.splice(0, 1)[0] };
      return {};
    },
  };
  return client;
}

function closeAll() {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
}

export async function lanDeviceTests(test, eq, ok) {
  const tls = { source: "self_signed", fingerprint: "AB:CD", names: [], urls: ["https://192.168.1.20:8443/", "https://studio.local:8443/", "https://localhost:8443/"] };

  test("a phone is offered the LAN addresses and never this machine's loopback", () => {
    eq(lanUrls({ tls }, "http://127.0.0.1:8443"), ["https://192.168.1.20:8443/", "https://studio.local:8443/"]);
    eq(lanUrls({ tls: { urls: ["https://localhost:9000/"] } }, "http://127.0.0.1:9000"), []);
    eq(lanUrls({}, "http://192.168.1.20:8080"), ["http://192.168.1.20:8080/"], "no HTTPS falls back to the page's own LAN address");
    eq(lanUrls({}, "http://localhost:8080"), []);
  });

  test("the token rides in the fragment, encoded", () => {
    eq(signInUrl("https://10.0.0.2:8443/", "a/b+c"), "https://10.0.0.2:8443/#token=a%2Fb%2Bc");
    eq(signInUrl("https://10.0.0.2:8443", "x"), "https://10.0.0.2:8443/#token=x");
  });

  const before = storedToken();
  const replaced = [];
  const fakeHistory = { replaceState: (s, t, url) => replaced.push(url) };
  const found = takeTokenFromAddress({ hash: "#token=a%2Fb&panel=x", pathname: "/", search: "?show=main" }, fakeHistory);
  const kept = storedToken();
  const none = takeTokenFromAddress({ hash: "#panel=x", pathname: "/", search: "" }, fakeHistory);
  storeToken(before);
  test("the page stores a token from its address and takes it off the bar", () => {
    eq(found, "a/b");
    eq(kept, "a/b");
    eq(replaced, ["/?show=main#panel=x"], "the rest of the fragment stays");
    eq(none, null, "an address with no token is left alone");
  });

  closeAll();
  const lone = stub({ urls: ["https://localhost:9000/"], fingerprint: "AB" });
  await openDevices(lone);
  const lonely = document.querySelector(".dialog[aria-label='Open on another device']");
  const allow = lonely && [...lonely.querySelectorAll("button")].find((b) => b.textContent === "Allow other devices");
  test("a mixer on loopback offers to let devices in, with no flags or config to edit", () => {
    ok(lonely, "the dialog opened");
    ok(allow, "an Allow other devices button");
    ok(!/--bind|control\.bind|config/.test(lonely.textContent), lonely.textContent);
    ok(!lonely.querySelector("svg"), "no QR code yet");
  });
  allow.click();
  await until(() => lone.calls.some(([m]) => m === "network.share"), 3000, "network.share");
  await until(() => /Restarting the mixer/.test(lonely.textContent), 3000, "the restarting note");
  // The old mixer still answers for a moment, as it did: still on loopback.
  await tick(1200);
  const waited = document.querySelector(".dialog[aria-label='Open on another device']") === lonely;
  test("the card waits for the new mixer rather than the old one letting go of its port", () => ok(waited));
  // The new one, started later and listening on the network.
  lone.tls = tls;
  lone.started = 2000;
  await until(() => [...document.querySelectorAll(".dialog[aria-label='Open on another device'] button")].some((b) => b.textContent === "Make a code"), 3000, "the card again, with a code to make");
  test("Allow other devices restarts the mixer and opens the card again ready for a code", () => {
    eq(lone.calls.find(([m]) => m === "network.share")[1], { enabled: true });
    eq(document.querySelectorAll(".dialog[aria-label='Open on another device']").length, 1, "one card, not two");
  });
  closeAll();

  const stuck = stub({ urls: [] }, { restarting: false, how: "none", message: "Nothing would start this mixer again." });
  await openDevices(stuck);
  const still = document.querySelector(".dialog[aria-label='Open on another device']");
  [...still.querySelectorAll("button")].find((b) => b.textContent === "Allow other devices").click();
  await until(() => still.textContent.includes("Nothing would start this mixer again."), 3000, "the mixer's own answer");
  test("a mixer that cannot restart from here says why in its own words and stays open", () => {
    ok([...still.querySelectorAll("button")].find((b) => b.textContent === "Allow other devices").disabled === false);
  });
  closeAll();

  const client = stub(tls);
  await openDevices(client);
  const box = document.querySelector(".dialog[aria-label='Open on another device']");
  const make = [...box.querySelectorAll("button")].find((b) => b.textContent === "Make a code");
  make.click();
  await until(() => box.querySelector("svg[role=img]"), 3000, "the QR code");
  await until(() => box.textContent.includes("Revoke"), 3000, "the device in the list");
  test("making a code asks for an operate token by default and draws its QR code", () => {
    const create = client.calls.find(([m]) => m === "token.create");
    eq(create[1].scope, "operate");
    ok(box.textContent.includes("AB:CD"), "the fingerprint is shown");
  });
  [...box.querySelectorAll("button")].find((b) => b.textContent === "Revoke").click();
  await until(() => document.querySelector(".dialog[aria-label='Are you sure?']"), 3000, "the confirmation");
  [...document.querySelectorAll(".dialog[aria-label='Are you sure?'] button")].find((b) => b.textContent === "Revoke").click();
  await until(() => client.calls.some(([m]) => m === "token.revoke"), 3000, "token.revoke");
  await until(() => box.textContent.includes("None yet"), 3000, "the list to empty");
  test("revoking asks first, then takes the device off the list", () => {
    eq(client.calls.find(([m]) => m === "token.revoke")[1], { id: "phone" });
  });
  closeAll();
}
