// A mixer without channel methods. Either the ingest plugin is missing, and
// the button that installs it goes where the channels would be, or it is here
// and older than channels, which is said as it is: installing it again
// changes nothing.

import { el } from "../../shell/dom.js";
import { installPlugin } from "../welcome/install.js";
import { listPlugins, hasPlugin } from "../../client/kinds.js";

export function installCard(client, reload) {
  const note = el("span.chn-dim");
  const title = el("h3", { text: "RTMP channels come with the ingest plugin" });
  const line = el("p.chn-dim", { text: "It is not on this mixer yet. Installing it takes nothing off air." });
  const button = el("button.btn.primary", {
    text: "Install the ingest plugin",
    onclick: async () => {
      if (await installPlugin(client, "ingest", button, note)) reload();
    },
  });
  listPlugins(client)
    .then((plugins) => {
      if (!hasPlugin(plugins, "ingest")) return;
      title.textContent = "This ingest plugin is older than RTMP channels";
      line.textContent = "It takes encoders on its own port, as before. Channels arrive with its next version.";
      button.remove();
    })
    .catch(() => {});
  return el("div.chn-note", {}, [title, line, el("div.row", {}, [button, note])]);
}
