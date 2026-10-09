// A channel's watch link under its tiles: the link with Copy, a QR code
// for a phone, a preview where the browser plays HLS, and the code to put
// the stream on a web page of your own. The card is the one an HLS output
// of the programme has, so the two read alike.
//
// Loaded only once a channel has a watch link switched on.

import { el } from "../../shell/dom.js";
import { hlsCard, hlsUrl, reachableBase } from "../renditions/hls-card.js";
import { keyed, write, copy } from "./keyed.js";
import { embedCode } from "./model.js";
import { reachable } from "../../shell/devices-share.js";
import { run } from "../../shell/commands.js";

/** One link's card, with its embed code below. Call `update` with each state. */
export function watchCard(view, channel, first) {
  const client = view.client;
  const card = hlsCard(client, first);
  const code = el("textarea.chn-embed", { readOnly: true, rows: 7, spellcheck: false, "aria-label": "Embed code" });
  const copyCode = el("button.btn", { type: "button", text: "Copy the code" });
  const embed = el("details.chn-more-opts", {}, [
    el("summary", { text: "Embed on a web page" }),
    el("p.chn-dim", { text: "Paste this into a page. Safari and phones play the link by themselves; hls.js, fetched from a CDN, plays it everywhere else." }),
    code,
    copyCode,
  ]);
  // A mixer that answers on this computer only gives a phone nothing to open.
  const alone = el("p.chn-dim", { hidden: true }, [
    "This link opens on this computer only, because the mixer is not on the network yet. ",
    el("button.btn.sm", { type: "button", text: "Let other devices in", onclick: () => run("help.devices") }),
  ]);
  client.call("core.info", {}).then((info) => { alone.hidden = reachable(info); }).catch(() => {});
  const node = el("div.chn-watch", {}, [el("span.chn-kicker", { text: `Watch ${channel.name}` }), alone, card.node, embed]);
  let base = (client.transport && client.transport.base) || location.origin;
  let dest = first;
  const draw = () => write(code, "value", embedCode(hlsUrl(dest, base) || "the link appears once it has started"));
  reachableBase(client).then((b) => { base = b; draw(); });
  copyCode.onclick = () => copy(copyCode, code.value);
  return {
    node,
    update(next) {
      dest = next;
      card.update(next);
      draw();
    },
  };
}

/** The cards for every watch link of a channel that is switched on. */
export function watchCards(view, box, rows, channel) {
  const links = (channel.destinations || []).filter((d) => d.platform === "hls" && d.enabled);
  keyed(box, rows, links, (d) => d.id, (d) => watchCard(view, channel, d));
  box.hidden = !links.length;
}
