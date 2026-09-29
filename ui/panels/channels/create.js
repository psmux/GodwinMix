// Add RTMP Channel: one box for a name, the address it will have drawn under
// it as it is typed, and on Create the card with the key on it.

import { el, on } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { slugify } from "./model.js";
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

export async function addChannel(client) {
  stylesheet();
  const input = el("input.chn-bigin", { type: "text", placeholder: "Sunday service", autocomplete: "off", "aria-label": "Channel name" });
  const base = el("span.chn-dim");
  const slug = el("strong.chn-slug");
  const preview = el("div.chn-preview", {}, [el("span.chn-kicker", { text: "Encoders will publish to" }), el("code", {}, [base, slug])]);
  const create = el("button.btn.primary", { text: "Create channel", disabled: true });
  const m = modal({
    title: "Add RTMP Channel",
    body: el("div.chn-create", {}, [
      el("label.chn-kicker", { text: "Name", for: "chn-name" }),
      input,
      preview,
      el("p.chn-dim", { text: "Each channel has its own address and keys. OBS, a phone or a hardware encoder can publish to it, several at once." }),
    ]),
    footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), create],
  });
  input.id = "chn-name";
  const draw = () => {
    const s = slugify(input.value);
    slug.textContent = s || "your-channel";
    slug.classList.toggle("empty", !s);
    create.disabled = !s;
  };
  on(input, "input", draw);
  on(input, "keydown", (e) => { if (e.key === "Enter" && !create.disabled) create.click(); });
  draw();
  input.focus();
  firstBase(client).then((b) => { base.textContent = (b || "rtmp://this-mixer:1935") + "/"; });

  create.onclick = async () => {
    const name = input.value.trim();
    create.disabled = true;
    let answer;
    try {
      answer = await client.call("channel.add", { name, app: slugify(name) });
    } catch (e) {
      errorToast(e, "Add RTMP Channel");
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
