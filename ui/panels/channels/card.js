// One channel as a card: its address, who is publishing, and where it goes on
// to. Built once, then written into as the channel changes.

import { el, svg } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { isLive, liveCount, bases } from "./model.js";
import { keyed, write, copy } from "./keyed.js";
import { streamRow } from "./streams.js";
import { destinationStrip } from "./distribute.js";
import { connectSection } from "./connect.js";
import { editChannel } from "./edit.js";

const GEAR = "M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4zM19.4 13.5l1.6 1.2-1.6 2.8-1.9-.7a7 7 0 0 1-1.7 1l-.3 2h-3.2l-.3-2a7 7 0 0 1-1.7-1l-1.9.7-1.6-2.8 1.6-1.2a7 7 0 0 1 0-2l-1.6-1.2 1.6-2.8 1.9.7a7 7 0 0 1 1.7-1l.3-2h3.2l.3 2a7 7 0 0 1 1.7 1l1.9-.7 1.6 2.8-1.6 1.2a7 7 0 0 1 0 2z";
const PLUG = "M9 3v5M15 3v5M6 8h12v3a6 6 0 0 1-12 0zM12 17v4";
const CHEVRON = "M6 9l6 6 6-6";

export function channelCard(view, first) {
  let channel = first;
  const dot = el("span.chn-dot");
  const name = el("h3.chn-name");
  const address = el("code.chn-addr");
  const copyBtn = el("button.chn-copy", { type: "button", text: "Copy", title: "Copy the server address" });
  copyBtn.onclick = () => copy(copyBtn, serverOf(view, channel));
  const pill = el("span.chn-pill");
  const streams = el("div.chn-streams");
  const rows = new Map();
  const waiting = el("div.chn-waiting");
  const strip = destinationStrip(view, first);

  const section = connectSection(view, () => channel, () => openSettings(view, channel));
  const connect = el("button.btn.chn-connect", { type: "button", "aria-expanded": "false", title: "Server, stream keys and full URLs, to copy" }, [svg(PLUG, 15), el("span", { text: "Connect" }), svg(CHEVRON, 14)]);
  const fold = (want) => {
    const open = section.toggle(want);
    connect.setAttribute("aria-expanded", String(open));
    node.classList.toggle("connecting", open);
    if (open) section.node.scrollIntoView?.({ block: "nearest" });
  };
  connect.onclick = () => fold();
  const settings = el("button.btn.icon.chn-gear", { type: "button", title: "Channel settings", "aria-label": "Channel settings", onclick: () => openSettings(view, channel) }, [svg(GEAR, 16)]);

  const node = el("article.chn-card", {}, [
    el("header.chn-head", {}, [
      dot,
      el("div.chn-title", {}, [name, el("div.chn-addrline", {}, [address, copyBtn])]),
      pill,
      el("div.chn-actions", {}, [connect, settings]),
    ]),
    section.node,
    streams,
    strip.node,
  ]);

  function update(next) {
    channel = next;
    const live = isLive(next);
    node.classList.toggle("live", live);
    node.classList.toggle("off", !next.enabled);
    write(dot, "className", "chn-dot" + (live ? " live" : next.enabled ? "" : " off"));
    write(name, "textContent", next.name || next.id);
    write(address, "textContent", serverOf(view, next));
    const count = liveCount(next);
    write(pill, "textContent", !next.enabled ? "Off" : live ? (count > 1 ? `Live, ${count} streams` : "Live") : "Waiting");
    write(pill, "className", "chn-pill" + (live ? " live" : next.enabled ? "" : " off"));
    section.update(next);
    drawStreams(next);
    strip.update(next);
  }

  function drawStreams(next) {
    const list = next.streams || [];
    if (!list.length || !next.enabled) {
      rows.clear();
      if (waiting.parentNode !== streams) streams.replaceChildren(waiting);
      // Rebuilt only when what it offers changes, so a button stays put
      // under a press while the channel's other numbers move.
      const mode = next.enabled ? "waiting" : "off";
      if (waiting.dataset.mode !== mode) waiting.replaceChildren(...waitingLine(view, next, () => fold(true)));
      waiting.dataset.mode = mode;
      waiting.classList.toggle("off", !next.enabled);
      return;
    }
    if (waiting.parentNode === streams) waiting.remove();
    keyed(streams, rows, list, (s) => s.name, () => streamRow(view, () => channel, section.streamUrl), (s) => s.state);
  }

  update(first);
  return {
    node,
    update,
    tick: () => { for (const row of rows.values()) row.tick?.(); },
  };
}

function serverOf(view, channel) {
  const base = bases(view.model, channel)[0];
  return base ? `${base}/${channel.app}` : (channel.publish && channel.publish.server) || "";
}

function waitingLine(view, channel, openConnect) {
  if (!channel.enabled) {
    const on = el("button.btn.sm", { text: "Switch it on" });
    on.onclick = async () => {
      on.disabled = true;
      try {
        view.accept(await view.client.call("channel.set", { id: channel.id, enabled: true }));
      } catch (e) {
        errorToast(e, "Switch it on");
        on.disabled = false;
      }
    };
    return [el("span", { text: "Switched off. Encoders are turned away until it is on again." }), on];
  }
  return [
    el("span.chn-radar"),
    el("span", { text: "Waiting for an encoder. " }),
    el("button.chn-link", { type: "button", text: "Where do I point it?", onclick: openConnect }),
  ];
}

function openSettings(view, channel) {
  return editChannel(view, channel);
}
