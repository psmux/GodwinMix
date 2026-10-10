// Flip a phone's camera, front to back and back again.
//
// A phone says which way a camera faces (`facingMode` in the track's
// settings), so the flip asks for the other way round. A laptop's cameras
// mostly say nothing, and then the flip moves to the next camera in the list.
// Many phones cannot hold two cameras open at once, so the one in use is
// stopped first; if nothing else opens, it is opened again.

import { listDevices, openTrack, remember } from "./devices.js";
import { install, refreshDevices } from "./tracks.js";

/** Which way a track's camera faces: "user", "environment", or "" when it does not say. */
export function facingOf(track) {
  const s = (track && track.getSettings && track.getSettings()) || {};
  return s.facingMode || "";
}

export function otherFacing(facing) {
  return facing === "environment" ? "user" : "environment";
}

/** The camera after `currentId` in `cameras`, round to the first. Null with fewer than two. */
export function nextCamera(cameras, currentId) {
  if (!cameras || cameras.length < 2) return null;
  const at = cameras.findIndex((c) => c.id === currentId);
  return cameras[(at + 1) % cameras.length].id;
}

/** The ways to try, in order: the other facing first when there is one, then the next camera. */
export function flipPlan(track, cameras) {
  const facing = facingOf(track);
  const current = track?.getSettings?.().deviceId || "";
  const plan = [];
  if (facing) plan.push({ facing: otherFacing(facing) });
  const next = nextCamera(cameras, current);
  if (next) plan.push({ deviceId: next });
  return { plan, current };
}

/** Flip `pub`'s camera. Answers true when another camera is now open. */
export async function flipCamera(pub, md = navigator.mediaDevices) {
  const old = pub.tracks.video;
  if (!old) return false;
  const { cameras } = await listDevices(md).catch(() => ({ cameras: [] }));
  const { plan, current } = flipPlan(old, cameras);
  if (!plan.length) return false;
  const enabled = old.enabled;
  old.stop();
  for (const step of plan) {
    const track = await openTrack("video", step.deviceId, { exact: true, facing: step.facing, loose: !!pub.shaper }, md).catch(() => null);
    if (track && track.getSettings().deviceId !== current) {
      track.enabled = enabled;
      remember("camera", track.getSettings().deviceId);
      await install(pub, "video", track);
      refreshDevices(pub);
      return true;
    }
    if (track) track.stop();
  }
  // Nothing else would open: the camera that was there comes back.
  const back = await openTrack("video", current, { exact: true, loose: !!pub.shaper }, md).catch(() => null);
  if (back) back.enabled = enabled;
  await install(pub, "video", back);
  pub.r.error.textContent = "The other camera would not open. This one stays.";
  return false;
}
