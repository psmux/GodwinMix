// A phone's camera as a mixer source, with one scan.
//
// The code is a link to /join/ on the mixer's https address with the browser
// channel's key in the fragment. The phone opens it, needs no operator token
// (the channel key is what lets a publisher in), and publishes under a name
// of its own, so every phone that scans it becomes a source of its own:
// `browser-anas-phone`, `browser-safari-ios-k3f9`. The channel's auto source
// makes each one, as it does for any stream.
//
// A phone's browser gives a page its camera only over https, and only a mixer
// that listens on the network can be reached from a phone at all, so when
// either is missing this says how to fix it rather than show a code that
// cannot work.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { errorToast, toast } from "../../shell/toast.js";
import { qrPath } from "../channels/qr.js";
import { copy } from "../channels/keyed.js";
import { ensureBrowserChannel, ensureIngest, sourceIdFor } from "./browser-channel.js";

const LOOPBACK = new Set(["localhost", "127.0.0.1", "[::1]", "::1"]);
const HTTPS_HELP = "https://github.com/psmux/GodwinMix/blob/main/docs/how-to/serve-https.md";

/** The https address a phone on the same network can open, or "" when there is none. */
export function lanAddress(info) {
  const urls = (info && info.tls && info.tls.urls) || [];
  return urls.find((u) => {
    try {
      return !LOOPBACK.has(new URL(u).hostname);
    } catch {
      return false;
    }
  }) || "";
}

/** The link a phone opens: /join/ with the channel and its key in the fragment. */
export function phoneLink(base, app, key) {
  const url = new URL("join/", base);
  url.hash = new URLSearchParams({ channel: app, key }).toString();
  return url.href;
}

/** Why a phone cannot reach this mixer, or "" when it can. */
export function unreachable(info) {
  if (!info || !info.tls) {
    return "HTTPS is off on this mixer, and a phone's browser only lets a page use its camera over https. Turn it back on with [control.tls] enabled = true, restart the mixer, and come back here.";
  }
  if (!lanAddress(info)) {
    return "This mixer only listens on this computer, so a phone cannot reach it. In the desktop app, switch on \"Let other devices on this network connect\" in Settings. On a server, set [control] bind = \"0.0.0.0:8080\" and restart the mixer.";
  }
  return "";
}

/** What the Add source row does: make sure the mixer can take a phone, then show the code. */
export async function openPhoneCamera(client) {
  try {
    await ensureIngest(client, (text) => toast({ text }));
    const info = await client.call("core.info", {});
    const why = unreachable(info);
    if (why) return showProblem(why);
    const { channel, key } = await ensureBrowserChannel(client);
    return showCode(client, channel, phoneLink(lanAddress(info), channel.app || channel.id, key), info.tls.fingerprint);
  } catch (e) {
    errorToast(e, "A phone's camera");
    return null;
  }
}

function showProblem(why) {
  const body = el("div.col.phone-cam", {}, [
    el("p", { text: why }),
    el("a", { href: HTTPS_HELP, target: "_blank", rel: "noopener", text: "How the mixer serves https" }),
  ]);
  const close = el("button.btn.primary", { text: "Close" });
  const m = modal({ title: "A phone's camera", body, footer: [close] });
  close.onclick = () => m.close();
  return m;
}

function showCode(client, channel, link, fingerprint) {
  const qr = qrPath(link);
  const picture = qr
    ? el("div.phone-cam-qr", { html: `<svg viewBox="0 0 ${qr.size} ${qr.size}" shape-rendering="crispEdges" role="img" aria-label="QR code of the link for a phone"><rect width="${qr.size}" height="${qr.size}" fill="#fff"/><path d="${qr.d}" fill="#000"/></svg>` })
    : el("p.dim", { text: "The link is too long for a QR code. Copy it and send it to the phone instead." });
  const live = el("ul.phone-cam-live.sm");
  const copyButton = el("button.btn", { type: "button", text: "Copy link" });
  copyButton.onclick = () => copy(copyButton, link);
  const body = el("div.col.phone-cam", {}, [
    picture,
    el("ol.sm.phone-cam-steps", {}, [
      el("li", { text: "Point the phone's camera app at the code and open the link." }),
      el("li", { text: `The first time, the phone warns about the certificate. It is this mixer's own: check the fingerprint starts ${String(fingerprint || "").slice(0, 11)} and accept it.` }),
      el("li", { text: "Give the phone a name if you like, then press Go live. It appears in Sources by itself." }),
    ]),
    el("code.phone-cam-link.sm", { text: link }),
    live,
  ]);
  const paint = (ch) => paintLive(live, ch);
  paint(channel);
  const off = client.on("event", ({ name, params }) => {
    if (name === "channel.changed" && params.channel && params.channel.id === channel.id) paint(params.channel);
  });
  const done = el("button.btn.primary", { text: "Done" });
  stylesheet();
  const m = modal({ title: "Add a phone's camera", body, footer: [copyButton, done], onClose: off });
  done.onclick = () => m.close();
  return m;
}

/** The phones sending now, each with the source it became. */
function paintLive(list, channel) {
  const streams = (channel.streams || []).filter((s) => s.state === "live");
  list.replaceChildren(
    ...(streams.length
      ? streams.map((s) => el("li", { text: `${s.name} is live, as the source ${sourceIdFor(channel, s.name)}.` }))
      : [el("li.dim", { text: "No phone is sending yet. Each one that scans the code shows up here." })]),
  );
}

function stylesheet() {
  if (document.querySelector("style[data-phone-cam]")) return;
  document.head.appendChild(el("style", {
    "data-phone-cam": "1",
    text: `.phone-cam { gap: var(--gap); max-width: 420px; }
.phone-cam-qr { background: #fff; padding: 12px; border-radius: 12px; width: min(280px, 70vw); margin: 0 auto; }
.phone-cam-qr svg { display: block; width: 100%; height: auto; }
.phone-cam-steps { margin: 0; padding-left: 20px; display: grid; gap: 4px; }
.phone-cam-link { word-break: break-all; user-select: all; }
.phone-cam-live { margin: 0; padding-left: 20px; }`,
  }));
}
