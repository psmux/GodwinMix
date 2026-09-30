// The playback card under an HLS output's row: the link, a Copy button, a
// QR code for a phone, and a small preview where the browser can play HLS.
//
// No player is shipped. hls.js is about 300 kB minified even in its light
// build, most of what the whole page weighs, and it would be compiled into
// every binary for a preview. Safari, the macOS desktop app, iPhones,
// Android and recent Chrome play HLS in a plain <video>; anything else gets
// "Open in a player" and a sentence saying why.

import { el } from "../../shell/dom.js";
import { qrPath } from "../channels/qr.js";
import { copy } from "../channels/keyed.js";
import { stylesheet } from "./shared.js";

const LOOPBACK = /^(localhost|127\.|\[::1\])/;

/** The master playlist's address for an output, on the page's own port. */
export function hlsUrl(id, base) {
  return new URL(`/hls/${encodeURIComponent(id)}/master.m3u8`, base).href;
}

/** True when a plain <video> element here plays HLS by itself. */
export function playsHls() {
  const v = document.createElement("video");
  return !!(v.canPlayType && v.canPlayType("application/vnd.apple.mpegurl"));
}

/**
 * The address a phone can reach. The page may be open on 127.0.0.1, which
 * means nothing to a phone, so the machine's own network address is asked
 * for and put in its place when the core knows one.
 */
async function reachableBase(client) {
  const base = (client.transport && client.transport.base) || location.origin;
  const u = new URL(base);
  if (!LOOPBACK.test(u.hostname)) return u.href;
  try {
    const hosts = (await client.call("channel.list", {})).hosts || [];
    if (hosts[0]) u.hostname = hosts[0];
  } catch {
    /* no channels plugin: the loopback address is still right for this machine */
  }
  return u.href;
}

/** A card that follows one output's status. Call `update` with each status. */
export function hlsCard(client, output) {
  stylesheet();
  const link = el("code.rnd-url");
  const copyBtn = el("button.btn", { type: "button", text: "Copy" });
  const qr = el("div.rnd-qr");
  const watch = el("button.btn", { type: "button", text: "Watch here" });
  const player = el("div.rnd-player", { hidden: true });
  const waiting = el("p.rnd-dim.rnd-waiting", { text: "The link works as soon as the first segment is made, a few seconds after it goes live." });
  const node = el("div.rnd-hls", {}, [
    qr,
    el("div.rnd-hlsmain", {}, [el("span.rnd-kicker", { text: "Link for viewers" }), el("div.rnd-urlrow", {}, [link, copyBtn]), waiting, el("div.rnd-urlrow", {}, [watch]), player]),
  ]);
  let url = hlsUrl(output.id, (client.transport && client.transport.base) || location.origin);
  const draw = () => {
    link.textContent = url;
    const q = qrPath(url);
    qr.innerHTML = q ? `<svg viewBox="0 0 ${q.size} ${q.size}" shape-rendering="crispEdges" role="img" aria-label="QR code of the link"><rect width="${q.size}" height="${q.size}" fill="#fff"/><path d="${q.d}" fill="#000"/></svg>` : "";
  };
  draw();
  reachableBase(client).then((b) => { url = hlsUrl(output.id, b); draw(); });
  copyBtn.onclick = () => copy(copyBtn, url);
  watch.onclick = () => togglePlayer(player, watch, url);
  return {
    node,
    update(next) {
      const live = next.state === "live";
      waiting.hidden = live;
      watch.disabled = !live;
      if (!live && !player.hidden) togglePlayer(player, watch, url);
    },
  };
}

/** Open the preview, or the reason there is none; close it again. */
function togglePlayer(player, button, url) {
  if (!player.hidden) {
    const v = player.querySelector("video");
    if (v) v.removeAttribute("src"), v.load();
    player.replaceChildren();
    player.hidden = true;
    button.textContent = "Watch here";
    return;
  }
  player.hidden = false;
  button.textContent = "Close the preview";
  if (playsHls()) {
    player.append(el("video", { src: url, controls: true, autoplay: true, muted: true, playsInline: true }));
    return;
  }
  player.append(
    el("p.rnd-dim", { text: "This browser does not play HLS by itself, and the page carries no player of its own so it stays quick to load. Safari, phones, VLC and smart TVs open the link directly." }),
    el("a.btn", { href: url, target: "_blank", rel: "noopener", text: "Open in a player" })
  );
}
