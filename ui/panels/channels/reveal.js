// The card shown straight after a key is made: the two boxes OBS asks for,
// laid out as OBS lays them out, the full URL for an encoder with one box,
// each with a Copy button, and a QR code of the full URL for a phone
// encoder to scan.
//
// The key is not lost when this card closes. The mixer keeps it sealed, and
// the channel's Connect section shows it again, so the card says that.

import { el, svg } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { errorToast } from "../../shell/toast.js";
import { obsFields, bases } from "./model.js";
import { copy } from "./keyed.js";
import { qrPath } from "./qr.js";

const LOCK = "M7 11V8a5 5 0 0 1 10 0v3M5 11h14v10H5z";

/** Straight after channel.add or channel.key.add, with the secret in hand. */
export async function showKey(client, model, channel, key) {
  const urls = model ? bases(model, channel) : await listBases(client, channel);
  const body = el("div.chn-reveal-body");
  const m = modal({ title: `Point an encoder at ${channel.name}`, body, wide: true, footer: [el("button.btn.primary", { text: "Done", onclick: () => m.close() })] });
  m.el.classList.add("chn-dialog");
  draw(body, client, model, channel, key, urls, 0, m);
  return m;
}

async function listBases(client, channel) {
  try {
    const list = await client.call("channel.list", {});
    const urls = (list.rtmp && list.rtmp.urls) || [];
    if (urls.length) return urls;
  } catch {
    /* the channel's own address below is enough */
  }
  return bases({ rtmp: {} }, channel);
}

function draw(body, client, model, channel, key, urls, at, m) {
  const f = obsFields(channel, key.secret, urls[at]);
  const redraw = (i) => draw(body, client, model, channel, key, urls, i, m);
  const picker = urls.length > 1
    ? el("div.chn-seg", { role: "group", "aria-label": "Which address" }, urls.map((u, i) =>
      el("button" + (i === at ? ".on" : ""), { type: "button", text: hostOf(u), onclick: () => redraw(i) })))
    : null;
  const obs = el("div.chn-obs", {}, [
    el("div.chn-obshead", {}, [el("span.chn-kicker", { text: "In OBS" }), el("span.chn-dim", { text: "Settings, Stream, Service: Custom" })]),
    picker,
    line("Server", f.server),
    line("Stream Key", f.key, true),
    line("Full URL", f.url),
  ]);
  const left = el("div.chn-obscol", {}, [obs, againNote(client, model, channel, m)]);
  body.replaceChildren(el("div.chn-reveal", {}, [left, qrCard(f.url)]));
}

function line(label, value, big) {
  const code = el("code" + (big ? ".chn-secret" : ""), { text: value });
  const button = el("button.btn.chn-copybtn", { type: "button", text: "Copy", "aria-label": `Copy the ${label.toLowerCase()}` });
  button.onclick = () => copy(button, value);
  return el("div.chn-obsrow", {}, [el("span.chn-obslabel", { text: label }), el("div.chn-obsval", {}, [code, button])]);
}

function againNote(client, model, channel, m) {
  return el("div.chn-once", {}, [
    svg(LOCK, 18),
    el("span.grow", { text: "You can see this key again any time under Connect on the channel." }),
    el("button.btn", { type: "button", text: "Make another key", onclick: () => { m.close(); newKey(client, model, channel); } }),
  ]);
}

async function newKey(client, model, channel) {
  try {
    const answer = await client.call("channel.key.add", { id: channel.id });
    showKey(client, model, channel, answer.key);
  } catch (e) {
    errorToast(e, "Make a key");
  }
}

function qrCard(url) {
  const qr = qrPath(url);
  if (!qr) return el("div.chn-qr", {}, [el("p.chn-dim", { text: "The address is too long for a QR code. Copy it instead." })]);
  const pic = el("div.chn-qrpic", { html: `<svg viewBox="0 0 ${qr.size} ${qr.size}" shape-rendering="crispEdges" role="img" aria-label="QR code of the full URL"><rect width="${qr.size}" height="${qr.size}" fill="#fff"/><path d="${qr.d}" fill="#000"/></svg>` });
  return el("div.chn-qr", {}, [pic, el("strong", { text: "On a phone" }), el("span.chn-dim", { text: "Scan it in the encoder app, or copy the full URL." })]);
}

function hostOf(url) {
  return String(url).replace(/^\w+:\/\//, "").replace(/[:/].*$/, "");
}
