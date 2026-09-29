// Where a channel is sent on to, as a strip of platform tiles.
//
// Each tile is the platform's mark inside a ring that says how the link is:
// grey when off, dashed while it waits for the encoder, turning while it
// connects, green when live, amber turning when it is trying again, red when
// it gave up, with the reason in the tile. The switch turns one on or off
// without opening anything. Adding one is a tile, a pasted key and done.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { platform } from "../../client/destinations.js";
import { brandMark } from "./brands.js";
import { keyed, write } from "./keyed.js";
import { tileState, ringState } from "./model.js";
import { addDestination, editDestination } from "./destination-form.js";

/** The platforms offered straight on an empty strip. The rest are behind More. */
export const QUICK = ["youtube", "facebook", "twitch", "kick"];

export function destinationStrip(view, first) {
  let channel = first;
  const tiles = el("div.chn-tiles");
  const rows = new Map();
  const add = el("button.chn-tile.chn-addtile", { type: "button", onclick: () => addDestination(view, channel) }, [
    el("span.chn-ring", {}, [el("span.chn-plus", { text: "+" })]),
    el("span.chn-tlabel", { text: "Add" }),
  ]);
  const quick = el("div.chn-quick");
  const lede = el("span.chn-dim");
  const node = el("section.chn-dist", {}, [
    el("div.chn-disthead", {}, [el("span.chn-kicker", { text: "Send on to" }), lede]),
    tiles,
    quick,
  ]);

  function update(next) {
    channel = next;
    const list = next.destinations || [];
    write(lede, "textContent", list.length ? "Straight from the encoder, nothing re-encoded." : "Pick a platform and paste its stream key.");
    keyed(tiles, rows, list, (d) => d.id, () => destinationTile(view, () => channel), (d) => d.platform);
    if (add.parentNode !== tiles || tiles.lastChild !== add) tiles.appendChild(add);
    add.hidden = !list.length;
    quick.hidden = list.length > 0;
    if (!quick.firstChild) quick.append(...QUICK.map((id) => quickTile(view, () => channel, id)), moreTile(view, () => channel));
  }

  update(first);
  return { node, update };
}

function quickTile(view, getChannel, id) {
  const p = platform(id);
  return el("button.chn-qtile", { type: "button", title: `Send to ${p.title}`, onclick: () => addDestination(view, getChannel(), p) }, [
    brandMark(id, 28),
    el("span", { text: p.title }),
  ]);
}

function moreTile(view, getChannel) {
  return el("button.chn-qtile.more", { type: "button", onclick: () => addDestination(view, getChannel()) }, [
    el("span.chn-more", { text: "…" }),
    el("span", { text: "More" }),
  ]);
}

function destinationTile(view, getChannel) {
  let dest = null;
  const ring = el("span.chn-ring");
  const label = el("span.chn-tlabel");
  const words = el("span.chn-tstate");
  const error = el("span.chn-terr");
  const main = el("button.chn-tmain", { type: "button", onclick: () => editDestination(view, getChannel(), dest) }, [ring, label, words]);
  const box = el("input", { type: "checkbox" });
  const toggle = el("label.chn-switch", {}, [box, el("span.chn-knob")]);
  const node = el("div.chn-tile", {}, [main, toggle, error]);
  box.onchange = () => setEnabled(view, getChannel(), dest, box);

  return {
    node,
    update(d) {
      dest = d;
      const p = platform(d.platform) || platform("custom");
      if (!ring.firstChild) ring.appendChild(brandMark(d.platform, 34));
      node.dataset.state = ringState(d);
      write(label, "textContent", d.label || p.title);
      write(words, "textContent", tileState(d));
      write(box, "checked", !!d.enabled);
      box.setAttribute("aria-label", `Send to ${d.label || p.title}`);
      const why = d.state === "failed" || d.state === "reconnecting" ? d.error || "" : "";
      // Trying again says why too: a person looking at an amber ring wants
      // to know whether it is their server or their key.
      write(error, "textContent", why);
      error.hidden = !error.textContent;
      node.title = why ? `${p.title}: ${why}` : `${p.title}, ${tileState(d).toLowerCase()}`;
    },
  };
}

async function setEnabled(view, channel, dest, box) {
  box.disabled = true;
  try {
    view.accept(await view.client.call("channel.destination.set", { id: channel.id, destination: dest.id, enabled: box.checked }));
  } catch (e) {
    box.checked = !box.checked;
    errorToast(e, box.checked ? "Stop sending" : "Start sending");
  } finally {
    box.disabled = false;
  }
}
