// Which camera a guest who has not picked one gets. A browser asked for "a
// camera" takes the system's first, and on a laptop with DroidCam or OBS
// installed that is often the program rather than the webcam: the guest then
// joined showing DroidCam's "not connected" card. Pure functions over the
// `{id, label}` list devices.js makes, so the tests need no camera.

/** Programs that pose as a camera, by the label the browser gives them. */
const VIRTUAL = /droidcam|\bobs[- ](virtual )?cam|virtual|manycam|snap camera|xsplit|vcam|nvidia broadcast|\bcamo\b|mmhmm|iriun|epoccam|ivcam|e2esoft|splitcam|youcam|vmix|\bndi\b|camera hub/i;

export function isVirtualCamera(label) {
  return VIRTUAL.test(label || "");
}

/**
 * The cameras best first: the real ones in the browser's own order, then the
 * programs posing as one. Nothing is dropped, so the list still offers them.
 */
export function rankCameras(cameras) {
  const virtual = (c) => isVirtualCamera(c.label);
  return [...cameras.filter((c) => !virtual(c)), ...cameras.filter(virtual)];
}

/**
 * The camera to use in place of `current` when nobody chose one: the first
 * real camera when `current` is a virtual one, or "" to keep it. Labels are
 * only there once the browser has been allowed a camera.
 */
export function realCameraFor(current, cameras) {
  const now = cameras.find((c) => c.id === current);
  if (!now || !isVirtualCamera(now.label)) return "";
  const best = rankCameras(cameras)[0];
  return best && !isVirtualCamera(best.label) ? best.id : "";
}

/**
 * The real cameras to try, best first, when the browser's own choice would
 * not open at all. Only cameras with a label the browser gave: without one
 * there is no telling a webcam from DroidCam.
 */
export function camerasToTry(cameras) {
  return rankCameras(cameras.filter((c) => c.named !== false)).filter((c) => !isVirtualCamera(c.label));
}
