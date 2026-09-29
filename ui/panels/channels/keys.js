// A channel's keys, one row each with its label and its last four characters,
// and Revoke, which asks once. One key per person or per encoder is what
// makes taking one back cost nobody else anything.

import { el, svg } from "../../shell/dom.js";
import { confirmModal } from "../../shell/modal.js";
import { errorToast } from "../../shell/toast.js";
import { field } from "./fields.js";
import { showKey } from "./reveal.js";

const KEY = "M14.5 9.5a4 4 0 1 1-2.9-3.8 4 4 0 0 1 2.9 3.8zM13.3 12.3 20 19M17 16l2-2M19 18l1.5-1.5";

export function keyList(view, first) {
  let channel = first;
  const rows = el("ul.chn-keys");
  const label = field("New key for", { placeholder: "Camera 2, or Pastor's phone" });
  const make = el("button.btn", { type: "button", text: "Make a key" });
  const node = el("section.chn-keybox", {}, [
    el("div.chn-keyhead", {}, [el("strong", { text: "Keys" }), el("span.chn-dim", { text: "Give each encoder its own, so one can be taken back alone." })]),
    rows,
    el("div.chn-newkey", {}, [label.node, make]),
  ]);

  function draw() {
    const keys = channel.keys || [];
    rows.replaceChildren(...(keys.length ? keys.map(row) : [el("li.chn-nokeys", { text: "No keys. Nobody can publish until you make one." })]));
  }

  function row(k) {
    const revoke = el("button.btn.danger.sm", { type: "button", text: "Revoke" });
    revoke.onclick = async () => {
      const who = k.label || `the key ending ${k.hint}`;
      if (!(await confirmModal(`Revoke ${who}? An encoder using it is cut off and cannot publish with it again.`, "Revoke"))) return;
      revoke.disabled = true;
      try {
        channel = await view.client.call("channel.key.remove", { id: channel.id, key: k.id });
        view.accept(channel);
        draw();
      } catch (e) {
        errorToast(e, "Revoke");
        revoke.disabled = false;
      }
    };
    return el("li.chn-key", {}, [
      svg(KEY, 16),
      el("span.grow", {}, [el("strong", { text: k.label || "Unnamed key" }), el("small.chn-dim", { text: made(k.created) })]),
      el("code.chn-hint", { text: "…" + (k.hint || "????") }),
      revoke,
    ]);
  }

  make.onclick = async () => {
    make.disabled = true;
    const wanted = label.value().trim();
    try {
      const answer = await view.client.call("channel.key.add", wanted ? { id: channel.id, label: wanted } : { id: channel.id });
      channel = await view.client.call("channel.get", { id: channel.id }).catch(() => channel);
      view.accept(channel);
      draw();
      label.input.value = "";
      showKey(view.client, view.model, channel, answer.key);
    } catch (e) {
      errorToast(e, "Make a key");
    } finally {
      make.disabled = false;
    }
  };

  draw();
  return { node };
}

function made(when) {
  const d = new Date(when);
  return isNaN(d) ? "" : "Made " + d.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}
