// A channel's settings: its name, whether it takes encoders at all, how the
// key travels, whether a live stream becomes a source, its keys, and removing
// it. The switches are saved together with Save; a key revoked is revoked at
// once, because that is what somebody pressing Revoke means.

import { el } from "../../shell/dom.js";
import { modal, confirmModal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { field, toggle } from "./fields.js";
import { keyList } from "./keys.js";

/** What Save sends: only what changed, so a stale form never undoes an event. */
export function settingsParams(channel, v) {
  const params = { id: channel.id };
  const name = String(v.name || "").trim();
  if (name && name !== channel.name) params.name = name;
  for (const k of ["enabled", "auto_source", "key_mode"]) if (v[k] !== undefined && v[k] !== channel[k]) params[k] = v[k];
  return params;
}

export function editChannel(view, channel) {
  const name = field("Name", { value: channel.name, note: `The address stays …/${channel.app}, so encoders already set up keep working.` });
  const enabled = toggle("Take encoders", channel.enabled, "Off turns every publisher away, and says why in the log.");
  const auto = toggle("Put each live stream in Sources", channel.auto_source, `As ${channel.app}-main and so on. It goes when the encoder stops, unless a scene uses it.`);
  const mode = keyMode(channel.key_mode);
  const keys = keyList(view, channel);
  const save = el("button.btn.primary", { text: "Save" });
  const remove = el("button.btn.danger", { text: "Remove channel" });
  const body = el("div.chn-settings", {}, [
    name.node,
    el("div.chn-group", {}, [enabled.node, auto.node]),
    el("div.chn-field", {}, [el("label", { text: "How encoders give their key" }), mode.node]),
    keys.node,
    el("div.chn-danger", {}, [el("span.grow", { text: "Removing it turns its encoders away and stops everything it sends on." }), remove]),
  ]);
  const m = modal({ title: `${channel.name} settings`, body, wide: true, footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), save] });
  m.el.classList.add("chn-dialog");

  save.onclick = async () => {
    const params = settingsParams(channel, { name: name.value(), enabled: enabled.value(), auto_source: auto.value(), key_mode: mode.value() });
    if (Object.keys(params).length === 1) return m.close();
    save.disabled = true;
    try {
      view.accept(await view.client.call("channel.set", params));
      m.close();
      toast({ text: "Saved." });
    } catch (e) {
      errorToast(e, "Save");
      save.disabled = false;
    }
  };
  remove.onclick = async () => {
    if (!(await confirmModal(`Remove ${channel.name}? Its encoders are turned away and its destinations stop.`, "Remove channel"))) return;
    try {
      await view.client.call("channel.remove", { id: channel.id });
      view.model.remove(channel.id);
      view.render();
      m.close();
    } catch (e) {
      errorToast(e, "Remove channel");
    }
  };
  return m;
}

/** Two cards to choose between, each showing what the encoder's box will hold. */
function keyMode(current) {
  let value = current || "query";
  const cards = [
    ["query", "Key in the address", "main?psk=KEY", "The usual. The stream name stays free, so one encoder can send several."],
    ["stream", "Key is the stream name", "KEY", "For an encoder with only one box for both."],
  ].map(([id, title, sample, note]) => {
    const card = el("button.chn-mode", { type: "button", role: "radio", "aria-checked": String(id === value), onclick: () => pick(id) }, [
      el("strong", { text: title }),
      el("code", { text: sample }),
      el("small.chn-dim", { text: note }),
    ]);
    card.dataset.mode = id;
    return card;
  });
  function pick(id) {
    value = id;
    for (const c of cards) c.setAttribute("aria-checked", String(c.dataset.mode === id));
  }
  return { node: el("div.chn-modes", { role: "radiogroup" }, cards), value: () => value };
}
