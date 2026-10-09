// A channel's keys, one row each with its label and its last four characters,
// and Revoke, which asks once. One key per person or per encoder is what
// makes taking one back cost nobody else anything.

import { el, svg } from "../../shell/dom.js";
import { confirmModal } from "../../shell/modal.js";
import { errorToast } from "../../shell/toast.js";
import { field, keyField } from "./fields.js";
import { showKey } from "./reveal.js";

const KEY = "M14.5 9.5a4 4 0 1 1-2.9-3.8 4 4 0 0 1 2.9 3.8zM13.3 12.3 20 19M17 16l2-2M19 18l1.5-1.5";

export function keyList(view, first) {
  let channel = first;
  const rows = el("ul.chn-keys");
  const label = field("New key for", { placeholder: "Camera 2, or Pastor's phone" });
  const make = el("button.btn", { type: "button", text: "Make a key" });
  const typed = keyField("Password", "The password after ?psk=");
  typed.node.appendChild(el("small.chn-dim", { text: "Keep the password your encoders already send after ?psk=. Left empty, the mixer makes a key." }));
  const own = el("details.chn-more-opts", {}, [el("summary", { text: "Use a password you already have" }), typed.node]);
  const node = el("section.chn-keybox", {}, [
    el("div.chn-keyhead", {}, [el("strong", { text: "Keys" }), el("span.chn-dim", { text: "Give each encoder its own, so one can be taken back alone." })]),
    rows,
    el("div.chn-newkey", {}, [label.node, make]),
    own,
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
      k.imported ? el("span.chn-typed", { text: "Typed", title: "A password a person typed in, not one the mixer made" }) : null,
      el("code.chn-hint", { text: "…" + (k.hint || "????") }),
      revoke,
    ]);
  }

  make.onclick = async () => {
    make.disabled = true;
    try {
      const answer = await view.client.call("channel.key.add", keyParams(channel.id, label.value(), typed.value()));
      channel = await view.client.call("channel.get", { id: channel.id }).catch(() => channel);
      view.accept(channel);
      draw();
      label.input.value = "";
      typed.input.value = "";
      showKey(view.client, view.model, channel, answer.key);
    } catch (e) {
      if (e && e.data && e.data.field === "secret") {
        own.open = true;
        typed.bad();
      }
      errorToast(e, "Make a key");
    } finally {
      make.disabled = false;
    }
  };

  draw();
  return { node };
}

/** What Make a key sends: a label and a typed password, only when given. */
export function keyParams(id, label, password) {
  const params = { id };
  if (label.trim()) params.label = label.trim();
  if (password.trim()) params.secret = password.trim();
  return params;
}

function made(when) {
  const d = new Date(when);
  return isNaN(d) ? "" : "Made " + d.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}
