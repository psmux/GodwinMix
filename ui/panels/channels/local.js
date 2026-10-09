// Record and Watch link: the two destinations a channel keeps on this
// machine. Neither has a key or a far end. Record writes the stream to a
// file as it arrives; Watch link serves it as HLS from the mixer's own port.
// Both copy the stream, so neither costs an encode.

import { el, on, coarse } from "../../shell/dom.js";
import { modal, confirmModal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { brandMark } from "./brands.js";
import { field, streamChoice } from "./fields.js";
import { tileState, isLive, recordName, fmtBytes, fmtUptime } from "./model.js";

/** The two tiles, in the order the pickers show them. */
export const LOCAL = [
  { id: "file", title: "Record", colour: "#3a3f4b", hint: "Saved on this mixer, copied" },
  { id: "hls", title: "Watch link", colour: "#2f6fde", hint: "A link anyone on your network opens" },
];

export const isLocal = (id) => id === "file" || id === "hls";
export const local = (id) => LOCAL.find((p) => p.id === id);

/** What the picker says under a tile, in the channel's own words. */
export function localLine(id, channel, date = new Date()) {
  if (id === "file") return `Record. Saves ${recordName(channel, date)}, copied, not converted.`;
  return "Watch link. An HLS link with a QR code and an embed code, copied, not converted.";
}

/** Everything `channel.destination.add` needs for one. */
export function localParams(id, channel, v = {}) {
  const params = { id: channel.id, platform: id, enabled: true };
  const folder = String(v.folder || "").trim();
  if (id === "file" && folder) params.server = folder;
  if (v.stream && v.stream !== "*") params.stream = v.stream;
  return params;
}

/** The line under a tile: the file a recording writes, or how a link is made. */
export function localPlan(d) {
  if (d.platform === "file" && d.file) {
    const done = d.file.open ? "" : `, ${fmtBytes(d.file.bytes)}, ${fmtUptime(d.file.duration_ms)}`;
    return { line: d.file.name + done, title: d.file.path };
  }
  if (d.platform === "file") return { line: "Copied, not converted", title: d.uri_host ? `Saves to ${d.uri_host}` : "" };
  return { line: "Copied, not converted", title: "Packaged as HLS on this mixer, nothing re-encoded" };
}

function head(id, channel) {
  const p = local(id);
  // The title is above it, so the line starts at what it does.
  const line = localLine(id, channel).slice(p.title.length + 2);
  return el("div.chn-dhead", {}, [brandMark(id, 48), el("div", {}, [el("strong", { text: p.title }), el("p.chn-dim", { text: line })])]);
}

/** The form for one: a folder for a recording, the stream when there is a choice. */
export function addLocal(view, channel, id) {
  const p = local(id);
  const folder = id === "file" ? field("Folder on the mixer", { placeholder: "Videos/GodwinMix in the mixer's home folder", mono: true, note: "Left empty, the recordings folder. A new file starts every time the stream goes live." }) : null;
  const stream = streamChoice(channel, "*");
  const start = el("button.btn.primary", { text: id === "file" ? "Start recording" : "Make the link" });
  const body = el("div.chn-dform", {}, [head(id, channel), folder && folder.node, stream && stream.node]);
  const m = modal({ title: `${p.title}: ${channel.name}`, body, footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), start] });
  on(body, "keydown", (e) => { if (e.key === "Enter" && e.target.tagName === "INPUT") { e.preventDefault(); start.click(); } });
  start.onclick = async () => {
    start.disabled = true;
    try {
      view.accept(await view.client.call("channel.destination.add", localParams(id, channel, { folder: folder && folder.value(), stream: stream && stream.value() })));
    } catch (e) {
      errorToast(e, p.title);
      start.disabled = false;
      return;
    }
    m.close();
    const when = isLive(channel) ? "It starts now." : `It starts when an encoder sends to ${channel.name}.`;
    toast({ text: id === "file" ? `Recording ${channel.name}. ${when}` : `Watch link made. ${when}` });
  };
  if (!folder) start.focus();
  else if (!coarse()) folder.focus();
  return m;
}

/** What one shows when pressed: its state, its file or its link, and Remove. */
export function editLocal(view, channel, dest) {
  const p = local(dest.platform);
  const status = el("div.chn-dstatus", { "data-state": dest.state }, [el("strong", { text: tileState(dest) }), dest.error ? el("span", { text: dest.error }) : null]);
  const where = dest.platform === "file" ? el("p.chn-dim", { text: dest.file ? `${dest.file.open ? "Writing" : "Last file"}: ${dest.file.path}` : `Saves to ${dest.uri_host || "the recordings folder"}.` }) : null;
  const card = el("div");
  if (dest.platform === "hls") import("./watch.js").then((w) => card.appendChild(w.watchCard(view, channel, dest).node));
  const remove = el("button.btn.danger", { text: "Remove" });
  const m = modal({
    title: dest.label || p.title,
    body: el("div.chn-dform", {}, [el("div.chn-dhead", {}, [brandMark(dest.platform, 48), status]), where, card]),
    footer: [remove, el("span.grow"), el("button.btn", { text: "Close", onclick: () => m.close() })],
  });
  const what = dest.platform === "file" ? "Stop recording? The files already written stay where they are." : "Take the watch link down? Anyone watching loses the picture.";
  remove.onclick = async () => {
    if (!(await confirmModal(what, "Remove"))) return;
    try {
      view.accept(await view.client.call("channel.destination.remove", { id: channel.id, destination: dest.id }));
      m.close();
    } catch (e) {
      errorToast(e, "Remove");
    }
  };
  return m;
}
