// A card's Connect section: every key the channel has, each with the Server,
// Stream key and full URL an encoder is given, folded until it is opened.
//
// The address and the stream name above the keys apply to all of them, so a
// person can make the variant they want (main_720p, cam2) and copy it. A key
// read from the core is kept here, for this card, only while the section is
// open; closing it forgets them all. Nothing goes to storage.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { bases, obsFields, streamName } from "./model.js";
import { keyed } from "./keyed.js";
import { keyBlock, reveal } from "./connect-key.js";

export function connectSection(view, getChannel, manage) {
  const secrets = new Map();
  const blocks = new Map();
  let open = false;
  let at = 0;
  let stream = "main";
  let urls = [];
  let drawn = "";

  const picker = el("div.chn-seg", { role: "group", "aria-label": "Which address" });
  const name = el("input.chn-mono.chn-streamin", { type: "text", value: stream, autocomplete: "off", spellcheck: "false", "aria-label": "Stream name" });
  const nameBox = el("label.chn-cname", {}, [el("span.chn-kicker", { text: "Stream name" }), name]);
  const note = el("span.chn-dim.chn-cnote", { text: "On this channel the key is the stream name." });
  const list = el("div.chn-ckeys");
  const empty = el("div.chn-cempty");
  const node = el("section.chn-connectbox", { hidden: true }, [
    el("div.chn-cbar", {}, [picker, nameBox, note]),
    list,
    empty,
    el("p.chn-cfoot", {}, [
      el("span", { text: "Give each encoder its own key, so one can be taken back alone. " }),
      el("button.chn-link", { type: "button", text: "Manage keys", onclick: () => manage() }),
    ]),
  ]);

  const ctx = {
    channel: getChannel,
    base: () => urls[at] || urls[0] || "",
    stream: () => stream,
    known: (id) => secrets.get(id) || null,
    async secret(id) {
      if (secrets.has(id)) return secrets.get(id);
      const got = await reveal(view.client, getChannel().id, id);
      if (got && open) secrets.set(id, got);
      return got;
    },
  };

  const redraw = () => { for (const b of blocks.values()) b.draw(); };

  name.oninput = () => {
    stream = streamName(name.value);
    redraw();
  };

  function drawPicker() {
    picker.replaceChildren(...urls.map((u, i) => el("button" + (i === at ? ".on" : ""), { type: "button", text: hostOf(u), title: u, onclick: () => { at = i; drawPicker(); redraw(); } })));
    picker.hidden = urls.length < 2;
  }

  function update(channel) {
    const next = bases(view.model, channel);
    if (next.join() !== urls.join()) {
      urls = next;
      at = Math.min(at, Math.max(0, urls.length - 1));
      drawPicker();
    }
    const byStream = channel.key_mode === "stream";
    nameBox.hidden = byStream;
    note.hidden = !byStream;
    const keys = channel.keys || [];
    keyed(list, blocks, keys, (k) => k.id, (k) => ({ ...keyBlock(ctx, k), update: () => {} }), (k) => `${k.label}|${k.hint}`);
    // A channel sends a change every time its numbers move. Only what the
    // fields are made of redraws them, so a Copy is never remade under a press.
    const shape = [channel.app, channel.key_mode, channel.publish && channel.publish.server].join("|");
    if (shape !== drawn) redraw();
    drawn = shape;
    if (empty.hidden !== !!keys.length || !empty.firstChild) {
      empty.replaceChildren(...(keys.length ? [] : noKeys(view, getChannel, secrets)));
      empty.hidden = !!keys.length;
    }
  }

  function toggle(want = !open) {
    open = want;
    node.hidden = !open;
    if (!open) {
      secrets.clear();
      for (const b of blocks.values()) b.hide();
    }
    return open;
  }

  /** The whole URL a live stream came in on, for its row's Copy. */
  async function streamUrl(s) {
    // In "key is the stream name" mode the core names the stream after the
    // key's id, so the URL is the key alone, never the name.
    const secret = s.key && (await ctx.secret(s.key));
    return secret ? obsFields(getChannel(), secret, ctx.base(), s.name).url : null;
  }

  return { node, update, toggle, streamUrl, get open() { return open; } };
}

function noKeys(view, getChannel, secrets) {
  const make = el("button.btn.primary", { type: "button", text: "Make a key" });
  make.onclick = async () => {
    make.disabled = true;
    try {
      const { key } = await view.client.call("channel.key.add", { id: getChannel().id });
      secrets.set(key.id, key.secret);
      view.accept(await view.client.call("channel.get", { id: getChannel().id }));
    } catch (e) {
      errorToast(e, "Make a key");
      make.disabled = false;
    }
  };
  return [el("span", { text: "This channel has no keys, so nobody can publish to it yet." }), make];
}

function hostOf(url) {
  return String(url).replace(/^\w+:\/\//, "").replace(/[:/].*$/, "");
}
