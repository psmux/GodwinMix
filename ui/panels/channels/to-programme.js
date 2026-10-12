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
// a tile already saved it is one press too: `channel.destination.to_output`
// reads the sealed key inside the core. Asking for the key again, and
// dropping the format picked for it, was reported as a bug (the output ran
// at the programme's 6 Mb/s, not the 3 Mb/s chosen).

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { platform, joinKey } from "../../client/destinations.js";
import { schemeError } from "../outputs/destination.js";
import { followStart } from "../outputs/failure.js";
import { isLive } from "./model.js";
import { write } from "./keyed.js";
import { toggle } from "./fields.js";

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

/** The `output.add` params for a platform, a server, a key and a format, or an error. */
export function programmeParams(client, p, server, key, rendition) {
  const base = String(server || p.server || "").trim();
  const secret = String(key || "").trim();
  if (!base) return { error: "Fill in the server address." };
  if (p.key && !secret && !p.keyOptional) return { error: "Paste the stream key." };
  const uri = p.key ? joinKey(base, secret) : base;
  const wrong = schemeError(p, uri);
  if (wrong) return { error: wrong };
  const params = { id: freeOutputId(client, p.id), uri, policy: p.policy || "own" };
  // Same as the source means nothing here: the programme has its encoder.
  if (rendition && rendition.preset !== "copy") params.rendition = rendition;
  return { params };
}

/** Add the platform under Outputs. True when the core took it. */
export async function sendProgramme(client, p, server, key, rendition) {
  const asked = programmeParams(client, p, server, key, rendition);
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
  announce(client, p, asked.params.id);
  return true;
}

function announce(client, p, id) {
  toast({ text: `Connecting the programme to ${p.title}. It is under Outputs as ${id}, which says when it is live, or why not.` });
  followStart(client, id, (text) => toast({ kind: "info", text }));
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

/** From a saved tile: its server, key and format, under Outputs, in one press. */
export function moveForm(view, channel, dest) {
  const p = platform(dest.platform) || platform("custom");
  const label = dest.label || p.title;
  const drop = toggle(`Take ${label} off ${channel.name}`, true, "So an encoder sent to the channel later cannot use the same key at the same time.");
  const go = el("button.btn.primary", { text: "Send the programme" });
  const where = dest.uri_host ? ` at ${dest.uri_host}` : "";
  const m = modal({
    title: `Send the programme to ${label}`,
    body: el("div.chn-dform", {}, [
      el("p.chn-dim", { text: `This adds ${label}${where} under Outputs, which sends what is on programme, with the stream key and format you already gave it.`, style: { marginTop: "0" } }),
      drop.node,
    ]),
    footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), go],
  });
  go.onclick = async () => {
    go.disabled = true;
    let added;
    try {
      added = await view.client.call("channel.destination.to_output", { id: channel.id, destination: dest.id, remove: drop.value() });
    } catch (e) {
      errorToast(e, `Send the programme to ${label}`);
      go.disabled = false;
      return;
    }
    m.close();
    announce(view.client, p, (added && added.id) || dest.id);
    if (drop.value()) view.client.call("channel.get", { id: channel.id }).then((c) => view.accept(c), () => {});
  };
  go.focus();
  return m;
}
