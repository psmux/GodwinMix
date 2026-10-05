// What this browser is called on the mixer: the stream it publishes, and so
// the source it becomes. One name per device, kept in this browser, so the
// same phone comes back as the same source and two phones never collide.
//
//   safari-ios-k3f9        made up the first time, from the browser and system
//   anas-phone             typed by the person holding it, as "Ana's phone"

const MEMORY = "gmx.join.name";

/**
 * The browser and the system it runs on, as a slug. "chrome-macos",
 * "firefox-windows", "safari-ios". Two of the same phone share it, which is
 * why `deviceName` puts a suffix of its own on the end.
 */
export function streamNameFor(ua = navigator.userAgent, platform = navigator.userAgentData?.platform || navigator.platform || "") {
  const u = String(ua);
  const browser = /Edg\//.test(u) ? "edge" : /OPR\//.test(u) ? "opera" : /Firefox\//.test(u) ? "firefox"
    : /Chrome\//.test(u) ? "chrome" : /Safari\//.test(u) ? "safari" : "browser";
  const p = `${platform} ${u}`.toLowerCase();
  const os = /iphone|ipad|ios/.test(p) ? "ios" : /android/.test(p) ? "android" : /mac/.test(p) ? "macos"
    : /win/.test(p) ? "windows" : /cros/.test(p) ? "chromeos" : /linux/.test(p) ? "linux" : "";
  return [browser, os].filter(Boolean).join("-");
}

/**
 * A name a person typed, as a stream name: lower case letters, digits and
 * single hyphens, at most 40 characters. "Ana's phone" is "anas-phone".
 * Empty when nothing usable is left.
 */
export function slugify(text) {
  return String(text || "")
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/['’]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40)
    .replace(/-+$/, "");
}

/** Four letters and digits, for the end of a made up name. */
export function suffix(random = Math.random) {
  let out = "";
  for (let i = 0; i < 4; i++) out += "abcdefghijkmnpqrstuvwxyz23456789"[Math.floor(random() * 32)];
  return out;
}

/**
 * This device's stream name and the label it was typed as, `{stream, label}`.
 * Made and kept the first time it is asked for; `label` is "" until somebody
 * names the device.
 */
export function deviceName(storage = safeStorage(), made = () => `${streamNameFor()}-${suffix()}`) {
  const kept = read(storage);
  if (kept.stream) return { stream: kept.stream, label: kept.label || "" };
  const fresh = { stream: made(), label: "" };
  write(storage, fresh);
  return fresh;
}

/**
 * Name this device. Answers the new `{stream, label}`, or the old one when
 * `label` has nothing in it a stream name can use.
 */
export function rename(label, storage = safeStorage()) {
  const stream = slugify(label);
  if (!stream) return deviceName(storage);
  const named = { stream, label: String(label).trim() };
  write(storage, named);
  return named;
}

function read(storage) {
  try {
    return JSON.parse(storage.getItem(MEMORY) || "{}") || {};
  } catch {
    return {};
  }
}

function write(storage, value) {
  try {
    storage.setItem(MEMORY, JSON.stringify(value));
  } catch {
    // Private windows and blocked storage: the name lasts as long as the page.
  }
}

function safeStorage() {
  try {
    return window.localStorage;
  } catch {
    return { getItem: () => null, setItem: () => {} };
  }
}
