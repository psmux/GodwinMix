// Add Channel: one box for a name, the address it will have drawn under
// it as it is typed, and what feeds it: this mixer's programme unless another
// is picked, since that is what most people want. An encoder's channel shows
// its key on Create. Under More options, the address and password encoders
// already use, for a person moving channels over from another server such as
// Livebox.

import { el, on } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { slugify, inUrl } from "./model.js";
import { field, keyField } from "./fields.js";
import { showKey } from "./reveal.js";
import { KINDS, feedParams } from "./feed.js";
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

/**
 * What Create sends. A typed address is kept as typed, capitals and all. With
 * no feed, an encoder feeds it, as a channel always was.
 */
export function addParams(name, address, password, feed) {
  const params = { name: name.trim(), app: address.trim() || slugify(name) };
  if (password.trim()) params.secret = password.trim();
  if (feed && feed.kind !== "encoder") params.feed = feed;
  return params;
}

/** The three choices, the programme first and picked. */
function feedChoice() {
  const url = el("input.chn-feedurl", { type: "text", placeholder: "rtmp://server/app/stream", autocomplete: "off", spellcheck: "false", "aria-label": "Address to pull", hidden: true });
  const radios = KINDS.map((k) => el("input", { type: "radio", name: "chn-new-feed", value: k.kind, checked: k.kind === "programme" }));
  const kind = () => radios.find((r) => r.checked).value;
  for (const r of radios) r.onchange = () => { url.hidden = kind() !== "pull"; if (!url.hidden) url.focus(); };
  const node = el("fieldset.chn-feed.chn-newfeed", {}, [
    el("legend.chn-kicker", { text: "Fed by" }),
    el("div.chn-feedopts", { role: "radiogroup", "aria-label": "Fed by" }, KINDS.map((k, i) => el("label.chn-feedopt", {}, [radios[i], el("span", { text: k.label })]))),
    url,
  ]);
  /** `{feed}` or `{error}`. */
  const value = () => {
    const asked = feedParams("", kind(), url.value);
    return asked.error ? { error: asked.error } : { feed: asked.params.feed };
  };
  return { node, value, url };
}

function readyWords(channel) {
  const f = channel.feed || {};
  if (f.kind === "programme") return `${channel.name} is fed by this mixer's programme. Add where it goes under Send on to.`;
  if (f.kind === "pull") return `${channel.name} is pulling ${f.uri_host || "the address"}.`;
  return `${channel.name} is ready for an encoder.`;
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
  const preview = el("div.chn-preview", {}, [el("span.chn-kicker", { text: "Its address" }), el("code", {}, [base, slug])]);
  const more = moreOptions();
  const feed = feedChoice();
  const create = el("button.btn.primary", { text: "Create channel", disabled: true });
  const m = modal({
    title: "Add Channel",
    body: el("div.chn-create", {}, [
      el("label.chn-kicker", { text: "Name", for: "chn-name" }),
      input,
      preview,
      feed.node,
      el("p.chn-dim", { text: "A channel is one live stream with its own address. It gets its picture from this mixer's programme, from an encoder publishing to it, or from a URL it pulls, and sends it on to the platforms you add." }),
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
    const fed = feed.value();
    if (fed.error) {
      toast({ kind: "warning", text: fed.error });
      feed.url.focus();
      return;
    }
    create.disabled = true;
    let answer;
    try {
      answer = await client.call("channel.add", addParams(name, more.address.value(), more.password.value(), fed.feed));
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
    toast({ text: readyWords(answer.channel) });
    if (answer.channel.feed && answer.channel.feed.kind !== "encoder") return null;
    return showKey(client, current?.model, answer.channel, answer.key);
  };
  return m;
}
