// A card's Connect section: every key the channel has, each with what an
// encoder is given for the chosen protocol (Server, Stream key and the full
// URL for RTMP; Server, Stream ID and Passphrase for SRT; the WHIP URL and
// its bearer token), folded until it is opened.
//
// The protocol, the address and the stream name above the keys apply to all
// of them, so a person can make the variant they want (SRT on the LAN
// address, cam2) and copy it. A key read from the core is kept here, for this
// card, only while the section is open; closing it forgets them all. Nothing
// goes to storage.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { bases, streamName } from "./model.js";
import { NAMES, baseFor, waysFields, waysIn } from "./ways.js";
import { keyed } from "./keyed.js";
import { keyBlock, reveal } from "./connect-key.js";

export function connectSection(view, getChannel, manage) {
  const secrets = new Map();
  const blocks = new Map();
  let open = false;
  let at = 0;
  let way = "";
  let stream = "main";
  let hosts = [];
  let drawn = "";

  const ways = el("div.chn-seg.chn-ways", { role: "group", "aria-label": "Which protocol" });
  const picker = el("div.chn-seg", { role: "group", "aria-label": "Which address" });
  const name = el("input.chn-mono.chn-streamin", { type: "text", value: stream, autocomplete: "off", spellcheck: "false", "aria-label": "Stream name" });
  const nameBox = el("label.chn-cname", {}, [el("span.chn-kicker", { text: "Stream name" }), name]);
  const note = el("span.chn-dim.chn-cnote", { text: "On this channel the key is the stream name." });
  const list = el("div.chn-ckeys");
  const empty = el("div.chn-cempty");
  const node = el("section.chn-connectbox", { hidden: true }, [
    el("div.chn-cbar", {}, [ways, picker, nameBox, note]),
    list,
    empty,
    el("p.chn-cfoot", {}, [
      el("span", { text: "Give each encoder its own key, so one can be taken back alone. " }),
      el("button.chn-link", { type: "button", text: "Manage keys", onclick: () => manage() }),
    ]),
  ]);

  const base = (protocol = way) => {
    const c = getChannel();
    const host = hosts[at] || hosts[0] || location.hostname;
    // RTMP keeps the addresses the core gave for its port, exactly.
    const given = protocol === "rtmp" && bases(view.model, c).find((b) => hostOf(b) === host);
    return given || baseFor(view.model, c, protocol, host);
  };

  const ctx = {
    channel: getChannel,
    fields: (secret, protocol = way) => waysFields(protocol, getChannel(), secret, base(protocol), stream),
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

  function segment(host, items, chosen, label, pick) {
    host.replaceChildren(...items.map((item, i) => el("button" + (i === chosen ? ".on" : ""), { type: "button", text: label(item), title: String(item), onclick: () => { pick(i); redraw(); } })));
    host.hidden = items.length < 2;
  }

  function drawPickers(channel) {
    const on = waysIn(channel);
    if (!on.includes(way)) way = on[0] || "rtmp";
    segment(ways, on, on.indexOf(way), (p) => NAMES[p], (i) => { way = on[i]; drawPickers(getChannel()); });
    segment(picker, hosts, at, (h) => h, (i) => { at = i; drawPickers(getChannel()); });
  }

  function update(channel) {
    const found = hostsOf(view.model, channel);
    if (found.join() !== hosts.join()) {
      hosts = found;
      at = Math.min(at, Math.max(0, hosts.length - 1));
    }
    const byStream = channel.key_mode === "stream";
    nameBox.hidden = byStream;
    note.hidden = !byStream;
    const keys = channel.keys || [];
    keyed(list, blocks, keys, (k) => k.id, (k) => ({ ...keyBlock(ctx, k), update: () => {} }), (k) => `${k.label}|${k.hint}`);
    // A channel sends a change every time its numbers move. Only what the
    // fields are made of redraws them, so a Copy is never remade under a press.
    const shape = [channel.app, channel.key_mode, channel.publish && channel.publish.server, waysIn(channel).join(), channel.rtmps && channel.rtmps.port, hosts.join()].join("|");
    if (shape !== drawn) {
      drawPickers(channel);
      redraw();
    }
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
    const protocol = waysIn(getChannel()).includes(s.protocol) ? s.protocol : "rtmp";
    return secret ? waysFields(protocol, getChannel(), secret, base(protocol), s.name).url : null;
  }

  return { node, update, toggle, streamUrl, get open() { return open; } };
}

/** The hosts an encoder can reach the mixer at, first one first. */
function hostsOf(model, channel) {
  const from = bases(model, channel).map(hostOf);
  const all = [...from, ...(model.hosts || [])];
  return all.filter((h, i) => h && all.indexOf(h) === i);
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
