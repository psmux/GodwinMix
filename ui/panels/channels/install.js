// A mixer without channel methods. Either the piece that takes encoders in is
// not set up yet, and it sets itself up as soon as this card is shown, or it
// is here and older than channels, which is said as it is: setting it up
// again changes nothing.

import { el } from "../../shell/dom.js";
import { installPlugin } from "../welcome/install.js";
import { listPlugins, hasPlugin } from "../../client/kinds.js";

export function installCard(client, reload) {
  const note = el("span.chn-dim");
  const title = el("h3", { text: "RTMP channels are not set up yet" });
  const line = el("p.chn-dim", { text: "Setting them up now. This happens once, takes about a minute, and nothing goes off air." });
  const button = el("button.btn.primary", {
    text: "Try again",
    onclick: async () => {
      if (await installPlugin(client, "ingest", button, note)) reload();
    },
  });
  listPlugins(client)
    .then(async (plugins) => {
      if (!hasPlugin(plugins, "ingest")) {
        if (await installPlugin(client, "ingest", button, note)) reload();
        return;
      }
      title.textContent = "This mixer's channel server is older than RTMP channels";
      line.textContent = "It takes encoders on its own port, as before. Channels arrive with its next version.";
      button.remove();
    })
    .catch(() => {});
  return el("div.chn-note", {}, [title, line, el("div.row", {}, [button, note])]);
}
