// The camera and the microphone: which ones there are, which one this
// browser used last time, and opening them.

import { ENCODER } from "./whip.js";

const MEMORY = "gmx.join.devices";

/**
 * Why this page cannot have a camera, or "" when it can. A browser hides
 * `navigator.mediaDevices` altogether outside a secure context, and says
 * nothing about why.
 */
export function secureProblem(win = window) {
  if (win.navigator && win.navigator.mediaDevices && win.navigator.mediaDevices.getUserMedia) return "";
  if (win.isSecureContext === false) {
    return `This page is open at ${win.location.origin}, which is not a secure address, so the browser will not let it use a camera or a microphone. Open the mixer's page at http://localhost on the mixer's own machine, or over https.`;
  }
  return "This browser does not offer a camera or a microphone to web pages.";
}

/** The device ids this browser picked last time, `{camera, mic}`. */
export function remembered(storage = safeStorage()) {
  try {
    return JSON.parse(storage.getItem(MEMORY) || "{}") || {};
  } catch {
    return {};
  }
}

export function remember(kind, deviceId, storage = safeStorage()) {
  try {
    const now = remembered(storage);
    now[kind] = deviceId;
    storage.setItem(MEMORY, JSON.stringify(now));
  } catch {
    // Private windows and blocked storage: the choice is simply not kept.
  }
}

function safeStorage() {
  try {
    return window.localStorage;
  } catch {
    return { getItem: () => null, setItem: () => {} };
  }
}

/** Cameras and microphones, each `{id, label}`. Labels are empty until permission is given. */
export async function listDevices(md = navigator.mediaDevices) {
  const all = await md.enumerateDevices();
  const pick = (kind, word) =>
    all
      .filter((d) => d.kind === kind && d.deviceId)
      .map((d, i) => ({ id: d.deviceId, label: d.label || `${word} ${i + 1}` }));
  return { cameras: pick("videoinput", "Camera"), mics: pick("audioinput", "Microphone") };
}

/** What getUserMedia is asked for the picture. `exact` when a person chose it. */
export function videoConstraints(deviceId, exact) {
  const c = {
    width: { ideal: ENCODER.width },
    height: { ideal: ENCODER.height },
    frameRate: { ideal: ENCODER.frameRate },
  };
  if (deviceId) c.deviceId = exact ? { exact: deviceId } : { ideal: deviceId };
  return c;
}

/**
 * What getUserMedia is asked for the sound. `processing` off is for a music
 * microphone: no echo cancellation, no noise suppression, no gain riding.
 */
export function audioConstraints(deviceId, exact, processing) {
  const c = {
    echoCancellation: processing,
    noiseSuppression: processing,
    autoGainControl: processing,
  };
  if (deviceId) c.deviceId = exact ? { exact: deviceId } : { ideal: deviceId };
  return c;
}

/** One track of one kind. Null when `deviceId` is "off". */
export async function openTrack(kind, deviceId, opts = {}, md = navigator.mediaDevices) {
  if (deviceId === "off") return null;
  const constraints =
    kind === "video"
      ? { video: videoConstraints(deviceId, opts.exact) }
      : { audio: audioConstraints(deviceId, opts.exact, opts.processing !== false) };
  const stream = await md.getUserMedia(constraints);
  return (kind === "video" ? stream.getVideoTracks() : stream.getAudioTracks())[0] || null;
}

/** The sentence for a getUserMedia refusal, by its name. */
export function mediaErrorText(e, what) {
  const name = e && e.name;
  if (name === "NotAllowedError") return `The browser was not allowed to use the ${what}. Allow it in the address bar's site settings and try again.`;
  if (name === "NotFoundError" || name === "OverconstrainedError") return `No ${what} was found. Plug one in, or pick another.`;
  if (name === "NotReadableError") return `The ${what} is in use by another program, or the system would not open it. Close the other program and try again.`;
  return `The ${what} would not open: ${(e && e.message) || e}`;
}
