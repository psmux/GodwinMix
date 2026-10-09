// Add Channel: one box for a name, the address it will have drawn under
// it as it is typed, and on Create the card with the key on it. Under More
// options, the address and password encoders already use, for a person moving
// channels over from another server such as Livebox.

import { el, on } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { slugify, inUrl } from "./model.js";
import { field, keyField } from "./fields.js";
import { showKey } from "./reveal.js";
import { current, bringForward, stylesheet } from "./panel.js";

/** Where the address will be, before the channel exists to say so. */
async function firstBase(client) {
  try {
    const list = await client.call("channel.list", {});
    return (list.rtmp && list.rtmp.urls && list.rtmp.urls[0]) || "";
  } catch {
    return "";
  }
}

/** What Create sends. A typed address is kept as typed, capitals and all. */
export function addParams(name, address, password) {
  const params = { name: name.trim(), app: address.trim() || slugify(name) };
  if (password.trim()) params.secret = password.trim();
  return params;
}

function moreOptions() {
  const address = field("Address", { placeholder: "Church", mono: true, note: "Keep the address your encoders already use, for example Church" });
  const password = keyField("Password", "The password after ?psk=");
  password.node.appendChild(el("small.chn-dim", { text: "Keep the password your encoders already send after ?psk=" }));
  const node = el("details.chn-more-opts", {}, [el("summary", { text: "More options" }), address.node, password.node]);
  return { node, address, password };
}

export async function addChannel(client) {
  stylesheet();
  const input = el("input.chn-bigin", { type: "text", placeholder: "Sunday service", autocomplete: "off", "aria-label": "Channel name" });
  const base = el("span.chn-dim");
  const slug = el("strong.chn-slug");
  const preview = el("div.chn-preview", {}, [el("span.chn-kicker", { text: "Encoders will publish to" }), el("code", {}, [base, slug])]);
  const more = moreOptions();
  const create = el("button.btn.primary", { text: "Create channel", disabled: true });
  const m = modal({
    title: "Add Channel",
    body: el("div.chn-create", {}, [
      el("label.chn-kicker", { text: "Name", for: "chn-name" }),
      input,
      preview,
      el("p.chn-dim", { text: "Each channel has its own address and keys. OBS, a phone or a hardware encoder can publish to it by RTMP, several at once. SRT and WHIP are switched on in its settings." }),
      more.node,
    ]),
    footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), create],
  });
  input.id = "chn-name";
  const draw = () => {
    const s = more.address.value().trim() || slugify(input.value);
    // As an encoder has to send it: a space is %20 in an RTMP address.
    slug.textContent = inUrl(s) || "your-channel";
    slug.classList.toggle("chn-unset", !s);
    create.disabled = !slugify(input.value) && !slugify(more.address.value());
  };
  on(input, "input", draw);
  on(more.address.input, "input", draw);
  on(input, "keydown", (e) => { if (e.key === "Enter" && !create.disabled) create.click(); });
  draw();
  input.focus();
  firstBase(client).then((b) => { base.textContent = (b || `rtmp://${location.hostname}:1935`) + "/"; });

  create.onclick = async () => {
    const name = input.value.trim() || more.address.value().trim();
    create.disabled = true;
    let answer;
    try {
      answer = await client.call("channel.add", addParams(name, more.address.value(), more.password.value()));
    } catch (e) {
      const at = e && e.data && e.data.field;
      if (at === "app" || at === "secret") more.node.open = true;
      if (at === "app") more.address.bad();
      if (at === "secret") more.password.bad();
      errorToast(e, "Add Channel");
      create.disabled = false;
      return;
    }
    m.close();
    current?.accept(answer.channel);
    bringForward();
    toast({ text: `${answer.channel.name} is ready for an encoder.` });
    return showKey(client, current?.model, answer.channel, answer.key);
  };
  return m;
}
