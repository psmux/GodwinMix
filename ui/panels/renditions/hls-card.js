// The playback card under an HLS output's row: the link, a Copy button, a
// QR code for a phone, and a small preview where the browser can play HLS.
//
// No player is shipped. hls.js is about 300 kB minified even in its light
// build, most of what the whole page weighs, and it would be compiled into
// every binary for a preview. Safari, the macOS desktop app, iPhones,
// Android and recent Chrome play HLS in a plain <video>; anything else gets
// "Open in a player" and a sentence saying why.

import { withShow } from "../../client/transport-rpc.js";
import { el } from "../../shell/dom.js";
import { qrPath } from "../channels/qr.js";
import { copy } from "../channels/keyed.js";
import { stylesheet } from "./shared.js";

const LOOPBACK = /^(localhost|127\.|\[::1\])/;

/**
 * The link for viewers: the output's own `playback.master_url_path`, which
 * carries its viewer key, on `base`. Null until the output has made one.
 */
export function hlsUrl(output, base) {
  const path = output && output.playback && output.playback.master_url_path;
  // A show other than the first is reached with its `?show=`, as the page is.
  return path ? withShow(new URL(path, base)).href : null;
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
export async function reachableBase(client) {
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
  // The page's own address plays the preview: the network address is for a
  // phone, and a core bound to loopback does not answer on it.
  const own = (client.transport && client.transport.base) || location.origin;
  let base = own;
  let current = output;
  let url = null;
  const local = () => hlsUrl(current, own);
  const draw = () => {
    url = hlsUrl(current, base);
    link.textContent = url || "The link appears once the output has started.";
    copyBtn.disabled = !url;
    const q = url && qrPath(url);
    qr.innerHTML = q ? `<svg viewBox="0 0 ${q.size} ${q.size}" shape-rendering="crispEdges" role="img" aria-label="QR code of the link"><rect width="${q.size}" height="${q.size}" fill="#fff"/><path d="${q.d}" fill="#000"/></svg>` : "";
  };
  draw();
  reachableBase(client).then((b) => { base = b; draw(); });
  copyBtn.onclick = () => url && copy(copyBtn, url);
  watch.onclick = () => url && togglePlayer(player, watch, local());
  return {
    node,
    update(next) {
      const before = local();
      current = next;
      draw();
      const live = next.state === "live" && !!url;
      waiting.hidden = live;
      watch.disabled = !live;
      if ((!live || local() !== before) && !player.hidden) togglePlayer(player, watch, before);
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
