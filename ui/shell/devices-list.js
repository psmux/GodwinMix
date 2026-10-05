// The device tokens this mixer has handed out, each with a Revoke button.
// Part of Help > Open on another device (devices.js).

import { el } from "./dom.js";
import { confirmModal } from "./modal.js";
import { toast } from "./toast.js";

const SCOPES = { read: "watch only", operate: "operate", admin: "admin" };

/** When a token was made, as a person reads it. */
function when(created) {
  const at = new Date(created);
  return Number.isNaN(at.getTime()) ? "" : at.toLocaleString();
}

function row(client, device, refresh) {
  const revoke = el("button.btn.sm", {
    text: "Revoke",
    onclick: async () => {
      const sure = await confirmModal(`Sign out "${device.label}" (${device.id})? Its next call is refused, and scanning a new code signs it in again.`, "Revoke");
      if (!sure) return;
      try {
        await client.call("token.revoke", { id: device.id });
        toast({ text: `${device.label} is signed out.` });
      } catch (e) {
        toast({ text: (e && e.message) || String(e) });
      }
      refresh();
    },
  });
  return el("div.row", {}, [
    el("div.col.grow", {}, [
      el("strong", { text: device.label }),
      el("span.sm.dim", { text: `${device.id}, ${SCOPES[device.scope] || device.scope}, made ${when(device.created)}` }),
    ]),
    revoke,
  ]);
}

/** The list, and a way to draw it again after a change. */
export function deviceList(client) {
  const node = el("div.col");
  const refresh = async () => {
    let devices;
    try {
      devices = (await client.call("token.list", {})).tokens || [];
    } catch (e) {
      // A token without admin cannot list or make device tokens; the form
      // above says the same when it is used, so one line here is enough.
      node.replaceChildren(el("p.sm.dim", { text: (e && e.message) || String(e) }));
      return;
    }
    const rows = devices.map((d) => row(client, d, refresh));
    node.replaceChildren(
      el("h3", { text: "Signed in devices", style: { margin: "8px 0 0" } }),
      ...(rows.length ? rows : [el("p.sm.dim", { text: "None yet. A device appears here once its code is made." })]),
    );
  };
  refresh();
  return { node, refresh };
}
