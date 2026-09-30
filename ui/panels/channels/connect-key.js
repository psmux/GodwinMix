// One key in a card's Connect section: its label, and the three things an
// encoder is given (Server, Stream key and the full URL), each with Copy.
// The key is dots until Show. Show and Copy ask the core for it once, and
// the section keeps it only while it is open.

import { el, svg } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { obsFields, masked } from "./model.js";
import { copy } from "./keyed.js";
import { qrPath } from "./qr.js";

const EYE = "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z";
const SHUT = "M3 3l18 18M10.6 5.1A10.4 10.4 0 0 1 12 5c6.4 0 10 7 10 7a17 17 0 0 1-2.6 3.4M6.6 6.6C3.7 8.4 2 12 2 12s3.6 7 10 7a9.6 9.6 0 0 0 5.4-1.6M9.9 9.9a3 3 0 0 0 4.2 4.2";
const KEY = "M14.5 9.5a4 4 0 1 1-2.9-3.8 4 4 0 0 1 2.9 3.8zM13.3 12.3 20 19M17 16l2-2M19 18l1.5-1.5";

/**
 * @param {object} ctx  {channel(), base(), stream(), known(keyId), secret(keyId)}.
 *   `known` answers from the section's memory and never asks; `secret` asks
 *   the core when memory has nothing, and is null when the core said no.
 */
export function keyBlock(ctx, key) {
  let shown = false;
  const eye = el("button.btn.sm.chn-eye", { type: "button" });
  const rows = el("div.chn-cfields");
  const qr = el("div.chn-cqr");
  const node = el("section.chn-ckey", {}, [
    el("header.chn-ckeyhead", {}, [svg(KEY, 15), el("strong.grow", { text: key.label || "Unnamed key" }), el("code.chn-hint", { text: "…" + (key.hint || "????") }), eye]),
    el("div.chn-cbody", {}, [rows, qr]),
  ]);

  const fields = (secret) => obsFields(ctx.channel(), secret, ctx.base(), ctx.stream());

  function draw() {
    const secret = shown ? ctx.known(key.id) : null;
    const f = fields(secret || masked(key.hint));
    rows.replaceChildren(
      line("Server", f.server, async () => f.server),
      line("Stream key", f.key, async () => (await real())?.key, !secret),
      line("Full URL", f.url, async () => (await real())?.url, !secret),
    );
    eye.replaceChildren(svg(shown ? SHUT : EYE, 15), el("span", { text: shown ? "Hide" : "Show" }));
    eye.setAttribute("aria-pressed", String(shown));
    eye.setAttribute("aria-label", `${shown ? "Hide" : "Show"} ${key.label || "the key"}`);
    qr.replaceChildren(...(secret ? qrOf(f.url) : []));
    qr.hidden = !secret;
  }

  async function real() {
    const secret = await ctx.secret(key.id);
    return secret ? fields(secret) : null;
  }

  eye.onclick = async () => {
    if (!shown && !(await ctx.secret(key.id))) return;
    shown = !shown;
    draw();
  };

  draw();
  return { node, draw, hide: () => { shown = false; draw(); } };
}

function line(label, value, read, hidden) {
  const code = el("code" + (hidden ? ".chn-masked" : ""), { text: value });
  const button = el("button.btn.chn-copybtn", { type: "button", text: "Copy", "aria-label": `Copy the ${label.toLowerCase()}` });
  button.onclick = async () => {
    const text = await read();
    if (text) copy(button, text);
  };
  return el("div.chn-obsrow", {}, [el("span.chn-obslabel", { text: label }), el("div.chn-obsval", {}, [code, button])]);
}

function qrOf(url) {
  const qr = qrPath(url);
  if (!qr) return [el("p.chn-dim", { text: "Too long for a QR code. Copy the full URL instead." })];
  return [
    el("div.chn-qrpic", { html: `<svg viewBox="0 0 ${qr.size} ${qr.size}" shape-rendering="crispEdges" role="img" aria-label="QR code of the full URL"><rect width="${qr.size}" height="${qr.size}" fill="#fff"/><path d="${qr.d}" fill="#000"/></svg>` }),
    el("span.chn-dim", { text: "Scan it in a phone encoder." }),
  ];
}

/** Ask the core for one key. A refusal is a toast and a null. */
export async function reveal(client, channelId, keyId) {
  try {
    return (await client.call("channel.key.reveal", { id: channelId, key: keyId })).secret;
  } catch (e) {
    errorToast(e, "Show the key");
    return null;
  }
}
