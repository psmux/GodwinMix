// The rows the add source picker shows for this browser's own camera and
// microphone. Small and separate, so the picker carries two rows and the
// publisher itself loads only when one is pressed.

import { errorToast } from "../../shell/toast.js";

const open = (client, camera) =>
  import("./browser-device.js")
    .then((m) => m.openBrowserDevice(client, { camera }))
    .catch((e) => errorToast(e, "This browser's camera"));

/** Picker rows for one category: Cameras and Microphones have one each. */
export function browserEntries(client, category) {
  if (category === "cameras") {
    return [{
      icon: "camera",
      name: "This browser's camera",
      note: "The camera and microphone on the computer showing this page",
      label: "Open",
      run: () => open(client, true),
      added: () => false,
    }];
  }
  if (category === "audio") {
    return [{
      icon: "mic",
      name: "This browser's microphone",
      note: "The microphone on the computer showing this page, with its camera off",
      label: "Open",
      run: () => open(client, false),
      added: () => false,
    }];
  }
  return [];
}
