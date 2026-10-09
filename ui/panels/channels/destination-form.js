// Adding a destination to a channel, and changing one.
//
// The add is a grid of platform tiles, then one box for the key. A platform
// with a published ingest never shows its server at all unless asked; one
// that hands out an address per stream asks for both. The key is a password
// box with a Show button, is trimmed, and is never read back: an edit that
// leaves it alone sends none.

import { el, on } from "../../shell/dom.js";
import { modal, confirmModal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { PLATFORMS, platform } from "../../client/destinations.js";
import { schemeError } from "../outputs/destination.js";
import { brandMark } from "./brands.js";
import { tileState, isLive } from "./model.js";
import { field, keyField, streamChoice } from "./fields.js";
import { formatStep, channelShape } from "../renditions/format-step.js";
import { isRefusal, showRefusal } from "../renditions/refusal.js";
import { waitingWords, sendProgramme } from "./to-programme.js";
import { LOCAL, addLocal, localLine } from "./local.js";

/** The tile grid, or straight to the form when the platform is known. */
export function addDestination(view, channel, chosen) {
  if (chosen) return addForm(view, channel, chosen);
  const grid = el("div.chn-pgrid");
  const m = modal({ title: `Send ${channel.name} on to`, body: el("div", {}, [el("p.chn-dim", { text: "Pick where it goes. For most platforms all you need is the stream key.", style: { marginTop: "0" } }), grid]), wide: true });
  // Record and Watch link first: they need nothing from any platform.
  for (const p of LOCAL) {
    grid.appendChild(el("button.chn-ptile", { type: "button", style: `--brand: ${p.colour}`, title: localLine(p.id, channel), onclick: () => { m.close(); addLocal(view, channel, p.id); } }, [
      brandMark(p.id, 44),
      el("span.chn-ptitle", { text: p.title }),
      el("span.chn-phint", { text: p.hint }),
    ]));
  }
  for (const p of PLATFORMS) {
    grid.appendChild(el("button.chn-ptile", { type: "button", style: `--brand: ${p.colour}`, onclick: () => { m.close(); addForm(view, channel, p); } }, [
      brandMark(p.id, 44),
      el("span.chn-ptitle", { text: p.title }),
      el("span.chn-phint", { text: p.hint || "" }),
    ]));
  }
  return m;
}

/** Everything the core needs to add this one, or the sentence saying what is missing. */
export function addParams(p, channel, v) {
  const server = String(v.server ?? p.server ?? "").trim();
  const key = String(v.key || "").trim();
  if (!server) return { error: p.key ? "Paste the server address as well." : "Fill in the address.", field: "server" };
  const wrong = schemeError(p, server);
  if (wrong) return { error: wrong, field: "server" };
  if (p.key && !key && !p.keyOptional) return { error: "Paste the stream key.", field: "key" };
  const params = { id: channel.id, platform: p.id, server, enabled: true };
  if (key) params.key = key;
  if (v.stream && v.stream !== "*") params.stream = v.stream;
  if (v.label && v.label.trim()) params.label = v.label.trim();
  return { params };
}

function addForm(view, channel, p) {
  const server = field("Server", { value: p.server || "", placeholder: p.example || "rtmp://your.server/live", mono: true });
  const key = p.key ? keyField(p.keyOptional ? "Stream key, if it has one" : "Stream key", "Paste the stream key") : null;
  const stream = streamChoice(channel, "*");
  const label = field("Name on the tile", { placeholder: p.title });
  const start = el("button.btn.primary", { text: "Start sending" });
  const more = el("details.chn-more-opts", {}, [el("summary", { text: "More options" }), label.node, stream && stream.node]);
  // Copy first, as the strip promises; a platform's own format one press away.
  const format = formatStep(view.client, { platform: p.id, platformTitle: p.title, shape: channelShape(channel), id: () => p.id });
  const refused = el("div", { hidden: true });
  // Said before the key is pasted, not after: this is where a person who
  // wanted the programme on YouTube finds out a channel is something else.
  const idle = !isLive(channel);
  const instead = idle ? el("button.btn", { text: "Send the programme instead" }) : null;
  const body = el("div.chn-dform", {}, [
    idle ? el("p.chn-addnote", { role: "note", text: waitingWords(channel, p.title) }) : null,
    el("div.chn-dhead", {}, [brandMark(p.id, 48), el("div", {}, [el("strong", { text: p.title }), el("p.chn-dim", { text: p.where })])]),
    p.fixed ? el("div.chn-fixed", {}, [el("span.chn-dim", { text: "Server " }), el("code", { text: p.server })]) : server.node,
    key && key.node,
    format.node,
    more,
    refused,
  ]);
  const m = modal({ title: `Send ${channel.name} to ${p.title}`, body, footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), instead, start] });
  if (instead) {
    // The same server and key, under Outputs, and nothing added to the channel.
    instead.onclick = async () => {
      instead.disabled = true;
      const sent = await sendProgramme(view.client, p, p.fixed ? p.server : server.value(), key && key.value());
      if (sent) m.close();
      else instead.disabled = false;
    };
  }
  on(body, "keydown", (e) => { if (e.key === "Enter" && e.target.tagName === "INPUT") { e.preventDefault(); start.click(); } });
  start.onclick = async () => {
    const asked = addParams(p, channel, { server: server.value(), key: key && key.value(), stream: stream && stream.value(), label: label.value() });
    if (asked.error) {
      (asked.field === "key" && key ? key : server).bad();
      toast({ kind: "warning", text: asked.error });
      return;
    }
    start.disabled = true;
    await format.ready;
    const rendition = format.value();
    if (rendition !== undefined) asked.params.rendition = rendition;
    if (!(await send(asked.params))) start.disabled = false;
  };
  async function send(params) {
    try {
      view.accept(await view.client.call("channel.destination.add", params));
    } catch (e) {
      if (isRefusal(e)) showRefusal(refused, e, (request) => send({ ...params, rendition: request }));
      else errorToast(e, p.title);
      return false;
    }
    m.close();
    toast({ text: isLive(channel) ? `${p.title} added. It goes live with ${channel.name}.` : `${p.title} added. It waits for an encoder to send to ${channel.name}; nothing reaches ${p.title} until one does.` });
    return true;
  }
  (key || server).focus();
  return m;
}

/** What an edit changed, and nothing else. */
export function editParams(channel, dest, v) {
  const params = { id: channel.id, destination: dest.id };
  if (v.label !== undefined && v.label.trim() !== (dest.label || "")) params.label = v.label.trim();
  if (v.stream !== undefined && v.stream !== (dest.stream || "*")) params.stream = v.stream;
  if (v.key && v.key.trim()) params.key = v.key.trim();
  if (v.server && v.server.trim()) params.server = v.server.trim();
  return params;
}

export function editDestination(view, channel, dest) {
  const p = platform(dest.platform) || platform("custom");
  const label = field("Name on the tile", { value: dest.label || "", placeholder: p.title });
  const stream = streamChoice(channel, dest.stream || "*");
  const server = p.fixed ? null : field("Server", { placeholder: dest.uri_host || p.example || "", mono: true, note: "Left empty, the one in use is kept." });
  const key = p.key ? keyField("Stream key", "kept", true) : null;
  const format = formatStep(view.client, { platform: p.id, platformTitle: p.title, shape: channelShape(channel), current: dest.rendition || { preset: "copy" }, id: () => dest.id });
  const refused = el("div", { hidden: true });
  const status = el("div.chn-dstatus", { "data-state": dest.state }, [el("strong", { text: tileState(dest) }), dest.error ? el("span", { text: dest.error }) : null]);
  const save = el("button.btn.primary", { text: "Save" });
  const remove = el("button.btn.danger", { text: "Remove" });
  const m = modal({
    title: dest.label || p.title,
    body: el("div.chn-dform", {}, [el("div.chn-dhead", {}, [brandMark(p.id, 48), status]), label.node, stream && stream.node, server && server.node, key && key.node, format.node, refused]),
    footer: [remove, el("span.grow"), el("button.btn", { text: "Cancel", onclick: () => m.close() }), save],
  });
  remove.onclick = async () => {
    if (!(await confirmModal(`Stop sending ${channel.name} to ${dest.label || p.title} and forget its key?`, "Remove"))) return;
    await act(view, m, "channel.destination.remove", { id: channel.id, destination: dest.id }, "Remove");
  };
  save.onclick = async () => {
    const params = editParams(channel, dest, { label: label.value(), stream: stream ? stream.value() : undefined, key: key && key.value(), server: server && server.value() });
    await format.ready;
    const rendition = format.value();
    if (rendition !== undefined) params.rendition = rendition;
    if (Object.keys(params).length === 2) return m.close();
    await act(view, m, "channel.destination.set", params, "Save", refused);
  };
  return m;
}

async function act(view, m, method, params, what, refused) {
  try {
    view.accept(await view.client.call(method, params));
    m.close();
    return true;
  } catch (e) {
    if (refused && isRefusal(e)) showRefusal(refused, e, (request) => act(view, m, method, { ...params, rendition: request }, what, refused));
    else errorToast(e, what);
    return false;
  }
}
