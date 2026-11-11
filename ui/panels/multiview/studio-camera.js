// A PTZ camera's controls over Studio mode's Preview, when what Preview holds
// is a camera: the source itself, or a scene with exactly one camera in it.
// Never over Programme, so nothing on air moves by accident. The overlay is
// ../camera/preview.js, fetched the first time Preview holds a camera.

import { hubFor } from "../camera/hub.js";
import { sceneKit } from "./studio-next.js";

/** The camera Preview holds, or null. */
export function cameraIn(hub, kit, next) {
  if (!next || !hub.cams.length) return null;
  if (next.kind === "source") return hub.cameraFor(next.id);
  const summary = kit && kit.summary(next.id);
  const cams = ((summary && summary.sources) || []).map((id) => hub.cameraFor(id)).filter(Boolean);
  return cams.length === 1 ? cams[0] : null;
}

/** Read the camera list while Studio mode is on, and let it go when it is off. */
export function watchCameras(panel, on) {
  if (!panel.client || !panel.client.call) return;
  if (!panel.camHub) {
    panel.camHub = hubFor(panel.client);
    panel.offs.push(panel.camHub.on((what) => what !== "people" && placeCamera(panel)));
  }
  if (on && !panel.offCams) panel.offCams = panel.camHub.want("cams");
  if (!on && panel.offCams) {
    panel.offCams();
    panel.offCams = null;
    placeCamera(panel);
  }
}

/** Show the overlay for the camera in Preview, or hide it. */
export function placeCamera(panel) {
  const hub = panel.camHub;
  if (!hub) return;
  const cam = panel.offCams && !panel.previewWrap.hidden ? cameraIn(hub, sceneKit(panel), panel.next) : null;
  if (!cam && !panel.camOverlay) return;
  if (panel.camOverlay) return panel.camOverlay.show(cam);
  if (panel.camLoading) return;
  panel.camLoading = import("../camera/preview.js").then((m) => {
    panel.camOverlay = m.previewOverlay(panel, hub);
    panel.offs.push(() => panel.camOverlay.show(null));
    placeCamera(panel);
  }).catch(() => (panel.camLoading = null));
}
