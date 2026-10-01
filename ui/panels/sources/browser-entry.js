// The rows the add source picker shows for this browser's own cameras and
// microphones, beside the ones on the mixer, and added the same way: press
// Add and the source goes into the scene. The publisher itself loads only
// when a row is pressed.

import { errorToast } from "../../shell/toast.js";

/** This browser's devices that have names, which they have once it has been allowed a camera. */
let known = { cameras: [], mics: [] };

/** Read this browser's devices again, for the next time the picker opens. */
export async function primeBrowserDevices() {
  try {
    const all = await navigator.mediaDevices.enumerateDevices();
    known = { cameras: named(all, "videoinput"), mics: named(all, "audioinput") };
  } catch {
    /* no media devices on this page; the generic rows stand */
  }
}

// Chrome lists "default" and "communications" beside the real microphones.
const named = (all, kind) =>
  all.filter((d) => d.kind === kind && d.label && d.deviceId && !["default", "communications"].includes(d.deviceId))
    .map((d) => ({ id: d.deviceId, label: d.label }));

if (typeof navigator !== "undefined" && navigator.mediaDevices) {
  primeBrowserDevices();
  navigator.mediaDevices.addEventListener?.("devicechange", primeBrowserDevices);
}

/** Is this page open on the mixer's own computer? Then its devices are the mixer's too. */
export function sameMachine(host = typeof location !== "undefined" ? location.hostname : "") {
  return ["localhost", "127.0.0.1", "::1", "[::1]"].includes(host);
}

const add = (client, choice, onSource) =>
  import("./browser-device.js")
    .then((m) => m.addBrowserDevice(client, { ...choice, onSource }))
    .catch((e) => errorToast(e, choice.camera ? "This browser's camera" : "This browser's microphone"));

function note(kind) {
  if (sameMachine()) {
    return `Through this browser, on the mixer's own computer. If this ${kind} is listed above too, add only one of the two: they share the device.`;
  }
  return `Through this browser, on the computer showing this page`;
}

/**
 * Picker rows for one category: one per camera or microphone this browser
 * has, or one row that asks for them when it has not been allowed yet.
 * `opts` are the picker's: the source goes to `onAdded` once the mixer has
 * it, which is how the scene it was added from gets it.
 */
export function browserEntries(client, category, opts = {}, devices = known) {
  const placed = opts.onAdded || opts.onExisting;
  const row = (name, kind, choice, icon) => ({
    icon, name, note: note(kind), label: "Add", added: () => false,
    run: () => add(client, choice, placed),
  });
  if (category === "cameras") {
    if (!devices.cameras.length) return [row("This browser's camera", "camera", { camera: true }, "camera")];
    return devices.cameras.map((d) => row(`${d.label} (this browser)`, "camera", { camera: true, cameraId: d.id }, "camera"));
  }
  if (category === "audio") {
    if (!devices.mics.length) return [row("This browser's microphone", "microphone", { camera: false }, "mic")];
    return devices.mics.map((d) => row(`${d.label} (this browser)`, "microphone", { camera: false, micId: d.id }, "mic"));
  }
  return [];
}
