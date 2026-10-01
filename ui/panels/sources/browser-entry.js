// The rows the add source picker shows for this browser's own camera and
// microphone. Small and separate, so the picker carries two rows and the
// publisher itself loads only when one is pressed.

import { errorToast } from "../../shell/toast.js";

const open = (client, camera, onSource) =>
  import("./browser-device.js")
    .then((m) => m.openBrowserDevice(client, { camera, onSource }))
    .catch((e) => errorToast(e, "This browser's camera"));

/**
 * Picker rows for one category: Cameras and Microphones have one each.
 * `opts` are the picker's: the source goes to `onAdded` once the mixer has
 * it, which is how a scene's Add sources puts it in that scene.
 */
export function browserEntries(client, category, opts = {}) {
  const placed = opts.onAdded || opts.onExisting;
  if (category === "cameras") {
    return [{
      icon: "camera",
      name: "This browser's camera",
      note: "The camera and microphone on the computer showing this page",
      label: "Add",
      run: () => open(client, true, placed),
      added: () => false,
    }];
  }
  if (category === "audio") {
    return [{
      icon: "mic",
      name: "This browser's microphone",
      note: "The microphone on the computer showing this page, with its camera off",
      label: "Add",
      run: () => open(client, false, placed),
      added: () => false,
    }];
  }
  return [];
}
