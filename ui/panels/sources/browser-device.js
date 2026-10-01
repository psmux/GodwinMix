// This browser's camera and microphone as a mixer source.
//
// Picking it is the whole of adding it, as for a camera on the mixer: the
// card opens the devices, starts sending by itself, and once the channel has
// made the source it is handed to `onSource`, which is how the picker puts it
// in the scene. The card opens folded, as a bar. It has to stay on the page,
// because the camera belongs to this tab; opened, it is where the operator
// switches device, mutes or stops it. It opens by itself only when something
// has gone wrong that the operator needs to read.

import { errorToast, toast } from "../../shell/toast.js";
import { ensureBrowserChannel, ensureIngest } from "./browser-channel.js";
import { primeBrowserDevices } from "./browser-entry.js";
import { BrowserDock } from "./browser-dock.js";

let current = null;

/**
 * What a picker row does: make sure the mixer can take a browser, then open
 * the card with the device the row names. `choice` is `{camera, cameraId?,
 * micId?, onSource?}`.
 */
export async function addBrowserDevice(client, choice) {
  await ensureIngest(client, (text) => toast({ text }));
  const dock = await openBrowserDevice(client, choice);
  // Allowed a camera, the browser names its devices: the picker lists them next time.
  setTimeout(primeBrowserDevices, 3000);
  return dock;
}

/**
 * Open the card, or bring back the one already open. `camera: false` starts
 * with the microphone alone. `onSource` is called once with the mixer source,
 * as soon as there is one.
 */
export async function openBrowserDevice(client, { camera = true, cameraId, micId, onSource } = {}) {
  const devices = { cameraId, micId };
  if (current) return current.use(devices).whenSource(onSource);
  let got;
  try {
    got = await ensureBrowserChannel(client);
  } catch (e) {
    errorToast(e, "This browser's camera");
    return null;
  }
  if (current) return current.use(devices).whenSource(onSource);
  current = new BrowserDock(client, got, camera, devices, () => {
    current = null;
  });
  return current.whenSource(onSource);
}
