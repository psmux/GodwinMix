// On a channel's card: the line saying whether it is live and with what, or
// why not, with small marks for what it does; and "Fed by", the three ways it
// gets its picture, one press each. A pull's address is never read back, so
// the box shows only where it pulls from.

import { el } from "../../shell/dom.js";
import { toast, errorToast } from "../../shell/toast.js";
import { write } from "./keyed.js";
import { KINDS, feedKind, statusOf, badgesOf, feedParams } from "./feed.js";

const MARKS = { programme: "Programme", pull: "Pull", push: "Push", record: "Rec", convert: "Convert" };

/**
 * LIVE with what arrives, or Offline with why, and the marks. A card has the
 * word in its header already, so it asks for the line without it.
 */
export function statusLine(withLabel = true) {
  const state = el("strong.chn-state", { hidden: !withLabel });
  const text = el("span.chn-statetext");
  const marks = el("span.chn-marks");
  const node = el("div.chn-status", { role: "status" }, [state, text, marks]);
  return {
    node,
    update(channel) {
      const s = statusOf(channel);
      write(node, "className", `chn-status ${s.state}`);
      write(state, "textContent", s.label);
      write(text, "textContent", s.text);
      const list = badgesOf(channel);
      const shape = list.map((b) => b.id).join(" ");
      if (marks.dataset.shape === shape) return;
      marks.dataset.shape = shape;
      marks.replaceChildren(...list.map((b) => el("span.chn-mark", { "data-mark": b.id, title: b.title, text: MARKS[b.id] })));
    },
  };
}

let groups = 0;

/** The three choices. `get()` answers the channel as it is now. */
export function fedBy(view, get) {
  const name = `chn-feed-${++groups}`;
  const options = KINDS.map((k) => {
    const input = el("input", { type: "radio", name, value: k.kind });
    return { kind: k.kind, input, node: el("label.chn-feedopt", {}, [input, el("span", { text: k.label })]) };
  });
  const url = el("input.chn-feedurl", { type: "text", autocomplete: "off", spellcheck: "false", "aria-label": "Address to pull" });
  const go = el("button.btn.sm.primary", { type: "button", text: "Pull" });
  const row = el("div.chn-feedurlrow", { hidden: true }, [url, go]);
  const node = el("fieldset.chn-feed", {}, [el("legend.chn-kicker", { text: "Fed by" }), el("div.chn-feedopts", { role: "radiogroup", "aria-label": "Fed by" }, options.map((o) => o.node)), row]);
  // A press on Pull opens the box; the box is left alone until Pull or another choice.
  let choosing = false;

  function show(kind) {
    for (const o of options) o.input.checked = o.kind === kind;
    row.hidden = kind !== "pull";
  }

  async function save(kind, address) {
    const c = get();
    const asked = feedParams(c.id, kind, address);
    if (asked.error) {
      toast({ kind: "warning", text: asked.error });
      url.focus();
      return;
    }
    go.disabled = true;
    try {
      view.accept(await view.client.call("channel.set", asked.params));
      choosing = false;
      url.value = "";
    } catch (e) {
      errorToast(e, "Fed by");
      if (kind !== "pull") show(feedKind(c));
    }
    go.disabled = false;
  }

  for (const o of options) {
    o.input.onchange = () => {
      if (o.kind === "pull") {
        choosing = true;
        show("pull");
        url.focus();
        return;
      }
      choosing = false;
      save(o.kind);
    };
  }
  go.onclick = () => save("pull", url.value);
  url.onkeydown = (e) => { if (e.key === "Enter") go.click(); };

  return {
    node,
    update(channel) {
      const kind = feedKind(channel);
      if (!choosing) show(kind);
      const from = channel.feed && channel.feed.uri_host;
      write(url, "placeholder", kind === "pull" && from ? `Pulling ${from}. Paste a new address to change it` : "rtmp://server/app/stream, srt://, rtsp:// or https:// for HLS");
      write(go, "textContent", kind === "pull" ? "Change" : "Pull");
    },
  };
}

/** The one press for a channel nothing sends to: the mixer's own picture. */
export function feedFromProgramme(view, channel, text = "Feed it from this mixer's programme") {
  const b = el("button.btn.sm", { type: "button", text });
  b.onclick = async () => {
    b.disabled = true;
    try {
      view.accept(await view.client.call("channel.set", feedParams(channel.id, "programme").params));
    } catch (e) {
      errorToast(e, text);
      b.disabled = false;
    }
  };
  return b;
}
