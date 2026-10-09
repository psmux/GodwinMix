// Where a channel is sent on to, as a strip of platform tiles.
//
// Each tile is the platform's mark inside a ring that says how the link is:
// grey when off, dashed while it waits for the encoder, turning while it
// connects, green when live, amber turning when it is trying again, red when
// it gave up, with the reason in the tile. The switch turns one on or off
// without opening anything. Adding one is a tile, a pasted key and done.
//
// Record and Watch link sit on the same strip (`local.js`). A watch link
// that is on has its card, with the link, a QR code and an embed code,
// below the tiles; that code is loaded only once there is one.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { platform } from "../../client/destinations.js";
import { brandMark } from "./brands.js";
import { keyed, write } from "./keyed.js";
import { tileState, ringState, startedAt } from "./model.js";
import { bulkButton } from "./bulk.js";
import { addDestination, editDestination } from "./destination-form.js";
import { waitingNote } from "./to-programme.js";
import { LOCAL, isLocal, local, addLocal, editLocal, localPlan } from "./local.js";

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
  // What a channel is for, while it has somewhere to go and nothing to send.
  const waiting = waitingNote(view);
  const bulk = bulkButton(view, () => channel);
  const watch = el("div.chn-watches", { hidden: true });
  const watchRows = new Map();
  const node = el("section.chn-dist", {}, [
    el("div.chn-disthead", {}, [
      el("span.chn-kicker", { text: "Send on to" }),
      el("span.chn-aka", { text: "(push destinations)" }),
      lede,
      bulk,
    ]),
    tiles,
    watch,
    waiting.node,
    quick,
  ]);

  function update(next) {
    channel = next;
    const list = next.destinations || [];
    const converted = list.some((d) => d.rendition && d.rendition.preset !== "copy");
    write(lede, "textContent", !list.length ? "Pick a platform and paste its stream key." : converted ? "Copied from the encoder, or converted here where a platform asked for its own format." : "Straight from the encoder, nothing re-encoded.");
    keyed(tiles, rows, list, (d) => d.id, () => destinationTile(view, () => channel), (d) => d.platform);
    if (add.parentNode !== tiles || tiles.lastChild !== add) tiles.appendChild(add);
    add.hidden = !list.length;
    quick.hidden = list.length > 0;
    waiting.update(next);
    if (!quick.firstChild) quick.append(...QUICK.map((id) => quickTile(view, () => channel, id)), ...LOCAL.map((p) => localTile(view, () => channel, p)), moreTile(view, () => channel));
    const links = list.some((d) => d.platform === "hls" && d.enabled);
    if (links || watchRows.size) import("./watch.js").then((w) => w.watchCards(view, watch, watchRows, channel));
  }

  update(first);
  // A live tile's duration moves with the panel's one second clock.
  const tick = () => { for (const t of rows.values()) t.tick(); };
  return { node, update, tick };
}

function quickTile(view, getChannel, id) {
  const p = platform(id);
  return el("button.chn-qtile", { type: "button", title: `Send to ${p.title}`, onclick: () => addDestination(view, getChannel(), p) }, [
    brandMark(id, 28),
    el("span", { text: p.title }),
  ]);
}

function localTile(view, getChannel, p) {
  return el("button.chn-qtile", { type: "button", title: p.hint, onclick: () => addLocal(view, getChannel(), p.id) }, [
    brandMark(p.id, 28),
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
  let began = 0;
  const ring = el("span.chn-ring");
  const label = el("span.chn-tlabel");
  const words = el("span.chn-tstate");
  const error = el("span.chn-terr");
  const plan = el("span.chn-tplan");
  const open = () => (isLocal(dest.platform) ? editLocal : editDestination)(view, getChannel(), dest);
  const main = el("button.chn-tmain", { type: "button", onclick: open }, [ring, label, words]);
  const box = el("input", { type: "checkbox" });
  const toggle = el("label.chn-switch", {}, [box, el("span.chn-knob")]);
  const node = el("div.chn-tile", {}, [main, toggle, plan, error]);
  box.onchange = () => setEnabled(view, getChannel(), dest, box);

  return {
    node,
    update(d) {
      dest = d;
      const p = platform(d.platform) || local(d.platform) || platform("custom");
      if (!ring.firstChild) ring.appendChild(brandMark(d.platform, 34));
      node.dataset.state = ringState(d);
      write(label, "textContent", d.label || p.title);
      const seen = view.model && view.model.seenAt.get(getChannel().id + "#" + d.id);
      began = d.state === "live" && d.since_ms != null ? startedAt(d.since_ms, seen) : 0;
      write(words, "textContent", tileState(d, began));
      write(box, "checked", !!d.enabled);
      // What the planner made of it: "Copied", or the encoder, whole on hover.
      const id = getChannel().id;
      const own = isLocal(d.platform) ? localPlan(d) : null;
      write(plan, "textContent", own ? own.line : view.plans ? view.plans.line(id, d.id, true) : "");
      write(plan, "title", own ? own.title : view.plans ? view.plans.line(id, d.id, false) : "");
      box.setAttribute("aria-label", `Send to ${d.label || p.title}`);
      const why = ["failed", "reconnecting", "connecting"].includes(d.state) ? d.error || "" : "";
      // Trying again says why too, as does a first dial that already failed:
      // an amber ring wants to say whether it is the server or the key.
      write(error, "textContent", why);
      error.hidden = !error.textContent;
      node.title = why ? `${p.title}: ${why}` : `${p.title}, ${tileState(d).toLowerCase()}`;
    },
    tick() {
      if (began) write(words, "textContent", tileState(dest, began));
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
