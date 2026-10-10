// Opening, swapping and listing the publisher's camera and microphone.
// Each function takes the publisher (`pub.r` its form, `pub.tracks` what is
// open, `pub.session` the publish if there is one) and changes only those.

import { fillSelect } from "./form.js";
import { listDevices, openTrack, mediaErrorText } from "./devices.js";

const WHAT = { video: "camera", audio: "microphone" };

/**
 * Open one device in place of the one open now. A person's own pick is
 * `exact`; a remembered one is a hint, so a camera unplugged since last time
 * gives way to whichever is there.
 */
export async function openKind(pub, kind, deviceId, exact) {
  const what = WHAT[kind];
  let track = null;
  try {
    track = await openTrack(kind, deviceId, { exact, processing: pub.r.processing.checked, loose: !!pub.shaper });
    pub.r.error.textContent = "";
  } catch (e) {
    pub.r.error.textContent = mediaErrorText(e, what);
  }
  const old = pub.tracks[kind];
  if (old && track) track.enabled = old.enabled;
  await install(pub, kind, track);
}

/**
 * Put `track` in place of the one open now, stopping that one, and on the
 * wire without a new offer if there is a publish.
 */
export async function install(pub, kind, track) {
  const old = pub.tracks[kind];
  if (old && old !== track) old.stop();
  pub.tracks[kind] = track;
  if (track) track.onended = () => (pub.r.error.textContent = `The ${WHAT[kind]} stopped. Pick it again, or another one.`);
  if (kind === "video" && pub.shaper) pub.shaper.setTrack(track);
  const sent = pub.sendable ? pub.sendable(kind, track) : track;
  if (pub.session) await pub.session.setTrack(kind, sent).catch(() => {});
  pub.paintTracks();
}

/** Fill both device lists, keeping what is open chosen. */
export async function refreshDevices(pub) {
  try {
    const { cameras, mics } = await listDevices();
    const id = (kind) => pub.tracks[kind]?.getSettings?.().deviceId || "";
    fillSelect(pub.r.camera, cameras, pub.tracks.video ? id("video") : "off", "No camera");
    fillSelect(pub.r.mic, mics, id("audio"), "");
    // A phone has a front and a back camera; one camera has nothing to flip to.
    pub.r.flip.hidden = cameras.length < 2;
  } catch (e) {
    pub.r.error.textContent = mediaErrorText(e, "list of devices");
  }
}
