// A channel an encoder is meant to feed, with somewhere to go and nothing
// arriving.
//
// A tester added Custom RTMP to the channel Live, as they would in Livebox,
// and expected the mixer's picture to go there. The channel waited for an
// encoder for ever, because a channel then took its picture only from an
// encoder. Now a channel can be fed by this mixer's programme, and that is
// the one press offered here. A channel fed by the programme or by a pull
// never shows this note: something is meant to be on its way already, and
// its status line says what.

import { el } from "../../shell/dom.js";
import { isLive } from "./model.js";
import { write } from "./keyed.js";
import { feedKind } from "./feed.js";
import { feedFromProgramme } from "./fed-by.js";

/** Whether nothing is coming because nothing was ever going to: an encoder's channel with none. */
export function waitsForEncoder(channel) {
  return channel.enabled !== false && feedKind(channel) === "encoder" && !isLive(channel);
}

/**
 * The sentence a form or a strip shows on such a channel. `where` is what
 * would get nothing: the strip's tiles, or the one platform a form is adding.
 */
export function waitingWords(channel, where = "the platforms below") {
  return `No encoder is sending to ${channel.name} yet, so nothing goes on to ${where}. ` +
    "To send the picture this mixer makes, feed the channel from this mixer's programme.";
}

/** The note under a channel's tiles, shown while it has places to go and no encoder. */
export function waitingNote(view) {
  const words = el("p.chn-dim");
  const buttons = el("div.chn-idle-acts");
  const node = el("div.chn-note.chn-idle", { role: "status", hidden: true }, [words, buttons]);
  let shown = "";
  function update(channel) {
    const waiting = (channel.destinations || []).some((d) => d.enabled && d.state === "waiting");
    const on = waiting && waitsForEncoder(channel);
    write(node, "hidden", !on);
    if (!on) {
      shown = "";
      return;
    }
    write(words, "textContent", waitingWords(channel));
    if (shown === channel.id) return;
    shown = channel.id;
    buttons.replaceChildren(feedFromProgramme(view, channel));
  }
  return { node, update };
}
