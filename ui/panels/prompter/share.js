// Open on a screen: the prompter page here, or a code a phone or a tablet
// scans. The code carries a read token made for it, so the screen follows
// the prompter and cannot change anything else on the mixer.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { qrPath } from "../channels/qr.js";
import { lanUrls } from "../../shell/devices.js";

function picture(url) {
  const qr = qrPath(url);
  if (!qr) return el("p.dim", { text: "The address is too long for a code. Copy the link instead." });
  return el("div", {
    style: { width: "240px", maxWidth: "70vw", alignSelf: "center" },
    html: `<svg viewBox="0 0 ${qr.size} ${qr.size}" shape-rendering="crispEdges" role="img" aria-label="QR code that opens the prompter"><rect width="${qr.size}" height="${qr.size}" fill="#fff"/><path d="${qr.d}" fill="#000"/></svg>`,
  });
}

/** A link for another device, with a read token when this desk may make one. */
async function link(client) {
  const info = await client.call("core.info", {}).catch(() => ({}));
  const base = (lanUrls(info, location.origin)[0] || location.origin + "/").replace(/\/?$/, "/");
  const open = !(info.token && info.token.id) || info.token.id === "open";
  if (open) return { url: `${base}prompter/`, note: "This mixer asks for no token, so the link opens the prompter as it is." };
  try {
    const made = await client.call("token.create", { label: "Prompter screen", scope: "read" });
    return { url: `${base}prompter/#token=${encodeURIComponent(made.token)}`, note: "The link carries a read token: the screen can follow the prompter and nothing more. Help > Open on another device lists it, to take it back." };
  } catch (e) {
    return { url: `${base}prompter/`, note: `A token for the screen could not be made (${e.message || e}). The screen will ask for one; an admin can make a read token in Help > Open on another device.` };
  }
}

export async function openShare(client) {
  const here = el("button.btn.primary", { text: "Open here", onclick: () => window.open("/prompter/", "gmx-prompter") });
  const body = el("div.col", {}, [el("p.dim", { text: "Making a link for another screen" })]);
  modal({ title: "Open the prompter on a screen", body, footer: [here] });
  const { url, note } = await link(client);
  body.replaceChildren(
    el("p", { text: "Scan this with a phone or a tablet on the same network, or open the link on the computer behind the glass." }),
    picture(url),
    el("input", { type: "text", readOnly: true, value: url, "aria-label": "Prompter link", onfocus: (e) => e.target.select() }),
    el("p.sm.dim", { text: note }),
  );
}
