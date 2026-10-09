// Bring channels from Livebox: a box to paste the addresses encoders already
// publish to, and a line for each saying what came of it.
//
// Every line is one channel.add with the channel name as the address and the
// password as the first key, the same call Add Channel makes (and an agent or
// gmx would make). An address this mixer already has is offered the password
// as one more key instead, with channel.key.add. Nothing here shows a password
// back: the lines name the channel and the line it came from.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { parseLivebox } from "./livebox-parse.js";
import { current, stylesheet } from "./panel.js";
import { inUrl } from "./model.js";

const EXAMPLE = "rtmp://192.168.1.10:1935/Church/main?psk=password\nrtmp://192.168.1.10:1935/Youth/main?psk=password";

export function importLivebox(client) {
  stylesheet();
  const box = el("textarea.chn-paste", { placeholder: EXAMPLE, "aria-label": "Livebox addresses, one a line", spellcheck: "false", autocomplete: "off" });
  const results = el("ul.chn-results", { role: "status" });
  const go = el("button.btn.primary", { text: "Bring them in" });
  const m = modal({
    title: "Bring channels from Livebox",
    wide: true,
    body: el("div.chn-create", {}, [
      el("p.chn-dim", { text: "Paste one address a line, the way an encoder publishes to Livebox. The Stream URL and Stream key from a Livebox channel's dashboard work too, one under the other." }),
      box,
      el("p.chn-dim", { text: "Each channel keeps its name and password. Point the encoders at this mixer's address and nothing else in them changes." }),
      results,
    ]),
    footer: [el("button.btn", { text: "Close", onclick: () => m.close() }), go],
  });
  box.focus();
  go.onclick = async () => {
    go.disabled = true;
    results.replaceChildren();
    const found = parseLivebox(box.value);
    if (!found.length) results.append(el("li.bad", { text: "Nothing to bring in yet. Paste an address such as " + EXAMPLE.split("\n")[0] }));
    for (const entry of found) results.append(await bring(client, entry));
    go.disabled = false;
  };
  return m;
}

/** One line: made, already here with a way to add the password, or why not. */
export async function bring(client, entry) {
  if (entry.error) return el("li.bad", { text: entry.error });
  const where = `Line ${entry.line}`;
  try {
    const answer = await client.call("channel.add", { name: entry.app, app: entry.app, secret: entry.secret });
    current?.accept(answer.channel);
    return el("li.made", { text: `${where}: made ${answer.channel.name}. Encoders publishing to …/${inUrl(answer.channel.app)}/${entry.stream} with their password are let in.` });
  } catch (e) {
    const other = e && e.data && e.data.field === "app" && e.data.channel;
    if (!other) return el("li.bad", { text: `${where}: ${(e && e.message) || e}` });
    return already(client, entry, other, where);
  }
}

function already(client, entry, id, where) {
  const add = el("button.btn.sm", { type: "button", text: `Add this password to ${id}` });
  const row = el("li", {}, [el("span", { text: `${where}: ${entry.app} is already the channel ${id}.` }), add]);
  add.onclick = async () => {
    add.disabled = true;
    try {
      await client.call("channel.key.add", { id, label: "Livebox", secret: entry.secret });
      const channel = await client.call("channel.get", { id }).catch(() => null);
      if (channel) current?.accept(channel);
      row.replaceChildren(el("span", { text: `${where}: added the password to ${id} as a key of its own.` }));
    } catch (e) {
      row.replaceChildren(el("span", { text: `${where}: ${(e && e.message) || e}` }));
      row.classList.add("bad");
    }
  };
  return row;
}
