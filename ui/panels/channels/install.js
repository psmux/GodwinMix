// A mixer without the ingest plugin has no channel methods. Say so, and put
// the button that installs it where the channels would be.

import { el } from "../../shell/dom.js";
import { installPlugin } from "../welcome/install.js";

export function installCard(client, reload) {
  const note = el("span.chn-dim");
  const button = el("button.btn.primary", {
    text: "Install the ingest plugin",
    onclick: async () => {
      if (await installPlugin(client, "ingest", button, note)) reload();
    },
  });
  return el("div.chn-note", {}, [
    el("h3", { text: "RTMP channels come with the ingest plugin" }),
    el("p.chn-dim", { text: "It is not on this mixer yet. Installing it takes nothing off air." }),
    el("div.row", {}, [button, note]),
  ]);
}
