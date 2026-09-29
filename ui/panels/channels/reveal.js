// The card an encoder is set up from: the two boxes OBS asks for, laid out as
// OBS lays them out, each with a Copy button, and a QR code of the whole
// address for a phone encoder to scan.
//
// A key is shown here once, straight after it is made. The core keeps only
// its last four characters, so the card says so, and offers another key
// rather than a way to see this one again.

import { el, svg } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { errorToast } from "../../shell/toast.js";
import { obsFields, bases } from "./model.js";
import { copy } from "./keyed.js";
import { qrPath } from "./qr.js";
import { field } from "./fields.js";

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

/** From the card, when no key is in hand: the server, and a way to make a key. */
export async function showConnect(client, model, channel) {
  const urls = bases(model, channel);
  const body = el("div.chn-reveal-body");
  const m = modal({ title: `Point an encoder at ${channel.name}`, body, wide: true, footer: [el("button.btn", { text: "Close", onclick: () => m.close() })] });
  m.el.classList.add("chn-dialog");
  draw(body, client, model, channel, null, urls, 0, m);
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
  const f = obsFields(channel, key ? key.secret : "", urls[at]);
  const redraw = (i) => draw(body, client, model, channel, key, urls, i, m);
  const picker = urls.length > 1
    ? el("div.chn-seg", { role: "group", "aria-label": "Which address" }, urls.map((u, i) =>
      el("button" + (i === at ? ".on" : ""), { type: "button", text: hostOf(u), onclick: () => redraw(i) })))
    : null;
  const obs = el("div.chn-obs", {}, [
    el("div.chn-obshead", {}, [el("span.chn-kicker", { text: "In OBS" }), el("span.chn-dim", { text: "Settings, Stream, Service: Custom" })]),
    picker,
    line("Server", f.server),
    key ? line("Stream Key", f.key, true) : keyless(channel),
  ]);
  const left = el("div.chn-obscol", {}, [obs, key ? onceNote(client, model, channel, m) : makeKey(client, model, channel, m)]);
  const right = key ? qrCard(f.url) : null;
  body.replaceChildren(el("div.chn-reveal" + (right ? "" : ".nokey"), {}, [left, right]));
}

function line(label, value, big) {
  const code = el("code" + (big ? ".chn-secret" : ""), { text: value });
  const button = el("button.btn.chn-copybtn", { type: "button", text: "Copy", "aria-label": `Copy the ${label.toLowerCase()}` });
  button.onclick = () => copy(button, value);
  return el("div.chn-obsrow", {}, [el("span.chn-obslabel", { text: label }), el("div.chn-obsval", {}, [code, button])]);
}

function keyless(channel) {
  const what = channel.key_mode === "stream" ? "the key itself" : "main?psk= and then the key";
  return el("div.chn-obsrow", {}, [el("span.chn-obslabel", { text: "Stream Key" }), el("div.chn-obsval.muted", {}, [el("span", { text: `Your key goes here, as ${what}. Keys are shown once, when they are made.` })])]);
}

function onceNote(client, model, channel, m) {
  return el("div.chn-once", {}, [
    svg(LOCK, 18),
    el("span.grow", { text: "This key is shown once. Copy it now: the mixer keeps only its last four characters." }),
    el("button.btn", { type: "button", text: "Make another key", onclick: () => { m.close(); newKey(client, model, channel); } }),
  ]);
}

function makeKey(client, model, channel, m) {
  const label = field("Who is it for?", { placeholder: "Camera 2, or Pastor's phone" });
  const go = el("button.btn.primary", { type: "button", text: "Make a key" });
  go.onclick = async () => {
    go.disabled = true;
    const made = await addKey(client, channel, label.value().trim());
    if (!made) { go.disabled = false; return; }
    m.close();
    showKey(client, model, channel, made);
  };
  label.input.addEventListener("keydown", (e) => { if (e.key === "Enter") go.click(); });
  return el("div.chn-newkey", {}, [label.node, go]);
}

async function newKey(client, model, channel) {
  const made = await addKey(client, channel, "");
  if (made) showKey(client, model, channel, made);
}

async function addKey(client, channel, label) {
  try {
    const answer = await client.call("channel.key.add", label ? { id: channel.id, label } : { id: channel.id });
    return answer.key;
  } catch (e) {
    errorToast(e, "Make a key");
    return null;
  }
}

function qrCard(url) {
  const qr = qrPath(url);
  if (!qr) return el("div.chn-qr", {}, [el("p.chn-dim", { text: "The address is too long for a QR code. Copy it instead." })]);
  const pic = el("div.chn-qrpic", { html: `<svg viewBox="0 0 ${qr.size} ${qr.size}" shape-rendering="crispEdges" role="img" aria-label="QR code of the whole address"><rect width="${qr.size}" height="${qr.size}" fill="#fff"/><path d="${qr.d}" fill="#000"/></svg>` });
  const full = el("button.chn-full", { type: "button", text: url, title: "Copy the whole address" });
  full.onclick = () => copy(full, url);
  return el("div.chn-qr", {}, [pic, el("strong", { text: "On a phone" }), el("span.chn-dim", { text: "Scan it in the encoder app, or copy the whole address." }), full]);
}

function hostOf(url) {
  return String(url).replace(/^\w+:\/\//, "").replace(/[:/].*$/, "");
}
