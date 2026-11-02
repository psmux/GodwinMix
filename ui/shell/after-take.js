// What Preview holds after a Take is the core's setting, not this page's:
// `program.take {}` (the Take on a Stream Deck through Companion) takes what
// the core leaves armed, so the core has to know. A choice made in Settings
// is sent when it is made and again on every connect, so a core that
// restarted hears it back. A page where nobody chose sends nothing, and the
// last page to choose decides for every client.

import { settings, onSettingsChanged } from "./settings.js";

export function afterTakeOnCore(client) {
  const send = () => {
    const s = settings();
    if (!s.afterTakeChosen) return;
    client.call("scene.preview.set", { after_take: s.afterTake }).catch(() => {});
  };
  client.on("open", send);
  onSettingsChanged((_, key) => key === "afterTake" && send());
}
