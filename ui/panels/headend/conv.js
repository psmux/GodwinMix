// A channel's conversion: a profile picked from the built in and saved
// ones, or custom settings of its own (custom-form.js, which is also the
// profile editor's form).

import { el } from "../../shell/dom.js";
import { profileWords } from "./setup.js";
import { choice, field } from "./form.js";
import { customForm } from "./custom-form.js";

export { customForm, encoderChoices } from "./custom-form.js";

export const STARTER = { video: { codec: "h264", width: 1280, height: 720, fit: "pad", bitrate_kbps: 1500, keyframe_ms: 2000, deinterlace: "auto" }, audio: { codec: "aac", bitrate_kbps: 128 } };

/**
 * The profile picker with the custom form under it. `value()` is the
 * channel's {profile, custom?}.
 */
export function convPicker(setup, channel, opts = {}) {
  const profiles = (setup && setup.profiles) || [];
  const rows = [
    ...profiles.filter((p) => p.builtin).map((p) => [p.id, p.name || p.id]),
    ...profiles.filter((p) => !p.builtin).map((p) => [p.id, `${p.name || p.id} (saved)`]),
  ];
  const current = (channel && channel.profile) || "copy";
  if (current !== "custom" && !rows.some(([id]) => id === current)) rows.push([current, `${current} (not on this station)`]);
  rows.push(["custom", "Custom settings for this channel"]);
  const pick = choice(rows, current, opts.label || "Profile");
  const words = el("p.hs-note.dim", { "aria-live": "polite" });
  const slot = el("div.hs-customslot");
  let custom = null;
  const sync = () => {
    const id = pick.value;
    if (id === "custom") {
      if (!custom) {
        // Custom starts from the channel's own settings, else the profile it
        // took when that one converts, else 720p H.264 with AAC sound.
        const was = profiles.find((p) => p.id === current);
        custom = customForm((channel && channel.custom) || (was && (was.video || was.audio) ? was : STARTER), setup && setup.encoders);
        slot.replaceChildren(custom.node);
      }
      slot.hidden = false;
      words.textContent = "Settings of this channel's own. To share them, save them as a profile under Profiles.";
    } else {
      slot.hidden = true;
      words.textContent = profileWords(profiles.find((p) => p.id === id));
    }
  };
  pick.addEventListener("change", sync);
  sync();
  return {
    node: el("div.hs-conv", {}, [field(opts.label || "Profile", pick), words, slot]),
    pick,
    get custom() { return custom; },
    value() {
      if (pick.value !== "custom") return { profile: pick.value };
      return { profile: "custom", custom: custom.read() };
    },
  };
}
