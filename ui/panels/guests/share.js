// The link to send a guest, and the code to scan, as `guest.invite.create`
// and `guest.invite.link` answer them. The code is the mixer's own SVG.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast } from "../../shell/toast.js";

export function openLink(answer, name) {
  const field = el("input.guests-link", { type: "text", readOnly: true, value: answer.link, "aria-label": "Invite link", onfocus: (e) => e.target.select() });
  const copy = el("button.btn.primary", { text: "Copy link", onclick: () => copyLink(answer.link) });
  const code = el("div.guests-qr", { html: answer.qr_svg || "", role: "img", "aria-label": `QR code that opens the invite for ${name}` });
  const body = el("div.col", {}, [
    el("p", { text: `Send ${name} this link, or let them scan the code with a phone. It opens their camera and plays them the show without their own voice.` }),
    code,
    field,
    answer.note ? el("p.sm.guests-note", { role: "alert", text: answer.note }) : null,
    el("p.sm.dim", { text: "Anyone with the link can join as this guest until it runs out or you revoke it, so send it only to them." }),
  ]);
  modal({ title: `Invite for ${name}`, body, footer: [copy] });
}

async function copyLink(link) {
  try {
    await navigator.clipboard.writeText(link);
    toast({ text: "The invite link is copied." });
  } catch {
    toast({ kind: "warning", text: "This browser would not copy it. Select the link and copy it by hand." });
  }
}
