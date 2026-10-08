// A channel passes on what an encoder sends it. It does not send the mixer's
// programme, and nothing on this tab used to say so.
//
// A tester added YouTube to the channel Live, pasted the key, pressed Start
// sending, and watched YouTube Studio say "No data" for as long as they cared
// to wait. The tile said "Waits for the stream", which is true and is not a
// sentence anybody reads as "this is the wrong place". What they wanted was
// Outputs, Add destination, which sends the programme.
//
// So while a channel has destinations and no encoder, the strip says what a
// channel is for and offers the other thing: the same platform as an output.
// From the add form that is one press, because the key is in the form. From
// a tile already saved it is the key pasted once more, because the channel
// keeps its keys sealed and never hands one back.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { platform, joinKey } from "../../client/destinations.js";
import { schemeError } from "../outputs/destination.js";
import { followStart } from "../outputs/failure.js";
import { isLive } from "./model.js";
import { write } from "./keyed.js";
import { field, keyField, toggle } from "./fields.js";

/**
 * The sentence a form or a strip shows while nothing is sending to the
 * channel. `where` is what would get nothing: the strip's tiles, or the one
 * platform a form is adding.
 */
export function waitingWords(channel, where = "the platforms below") {
  return `Nothing is sending to ${channel.name} yet, so nothing goes on to ${where}. ` +
    "A channel passes on what an encoder (OBS, a phone, a camera) sends to it. " +
    "To send the picture this mixer makes, send the programme to the platform instead.";
}

/** An output id for this platform that is not taken yet: youtube, youtube-2. */
export function freeOutputId(client, base) {
  const taken = new Set(((client.state && client.state.outputs) || []).map((o) => o.id));
  let id = base;
  for (let n = 2; taken.has(id); n++) id = `${base}-${n}`;
  return id;
}

/** The `output.add` params for a platform, a server and a key, or an error. */
export function programmeParams(client, p, server, key) {
  const base = String(server || p.server || "").trim();
  const secret = String(key || "").trim();
  if (!base) return { error: "Fill in the server address." };
  if (p.key && !secret && !p.keyOptional) return { error: "Paste the stream key." };
  const uri = p.key ? joinKey(base, secret) : base;
  const wrong = schemeError(p, uri);
  if (wrong) return { error: wrong };
  return { params: { id: freeOutputId(client, p.id), uri, policy: p.policy || "own" } };
}

/** Add the platform under Outputs. True when the core took it. */
export async function sendProgramme(client, p, server, key) {
  const asked = programmeParams(client, p, server, key);
  if (asked.error) {
    toast({ kind: "warning", text: asked.error });
    return false;
  }
  try {
    await client.call("output.add", asked.params);
  } catch (e) {
    errorToast(e, `Send the programme to ${p.title}`);
    return false;
  }
  const id = asked.params.id;
  toast({ text: `Connecting the programme to ${p.title}. It is under Outputs as ${id}, which says when it is live, or why not.` });
  followStart(client, id, (text) => toast({ kind: "info", text }));
  return true;
}

/** The note under a channel's tiles, shown while it has places to go and no encoder. */
export function waitingNote(view) {
  const words = el("p.chn-dim");
  const buttons = el("div.chn-idle-acts");
  const node = el("div.chn-note.chn-idle", { role: "status", hidden: true }, [words, buttons]);
  let shown = "";
  function update(channel) {
    const waiting = (channel.destinations || []).filter((d) => d.enabled && d.state === "waiting");
    const on = waiting.length > 0 && !isLive(channel);
    write(node, "hidden", !on);
    if (!on) return;
    write(words, "textContent", waitingWords(channel));
    const key = waiting.map((d) => d.id).join(",");
    if (key === shown) return;
    shown = key;
    buttons.replaceChildren(...waiting.map((d) => {
      const p = platform(d.platform) || platform("custom");
      const label = d.label || p.title;
      return el("button.btn.sm", { type: "button", text: `Send the programme to ${label} instead`, onclick: () => moveForm(view, channel, d) });
    }));
  }
  return { node, update };
}

/** From a saved tile: the key again, then the output, then the tile off the channel. */
export function moveForm(view, channel, dest) {
  const p = platform(dest.platform) || platform("custom");
  const label = dest.label || p.title;
  const server = p.fixed ? null : field("Server", { placeholder: dest.uri_host || p.example || "rtmp://your.server/live", mono: true });
  const key = p.key ? keyField(p.keyOptional ? "Stream key, if it has one" : "Stream key", "Paste the stream key") : null;
  const drop = toggle(`Take ${label} off ${channel.name}`, true, "So an encoder sent to the channel later cannot use the same key at the same time.");
  const go = el("button.btn.primary", { text: "Send the programme" });
  const m = modal({
    title: `Send the programme to ${label}`,
    body: el("div.chn-dform", {}, [
      el("p.chn-dim", { text: `This adds ${p.title} under Outputs, which sends what is on programme. The channel keeps its keys sealed and cannot hand this one back, so paste it once more.`, style: { marginTop: "0" } }),
      server && server.node,
      key && key.node,
      drop.node,
    ]),
    footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), go],
  });
  go.onclick = async () => {
    go.disabled = true;
    const sent = await sendProgramme(view.client, p, server ? server.value() : p.server, key && key.value());
    if (!sent) {
      go.disabled = false;
      return;
    }
    m.close();
    if (!drop.value()) return;
    try {
      view.accept(await view.client.call("channel.destination.remove", { id: channel.id, destination: dest.id }));
    } catch (e) {
      errorToast(e, `Take ${label} off ${channel.name}`);
    }
  };
  (key || server)?.focus();
  return m;
}
