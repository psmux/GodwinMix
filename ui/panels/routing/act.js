// What a press in the routing view does: send an input to an output from an
// empty cell, add a new output to a group, open a route to change it, start
// a stopped show. Each reuses the form the rest of the page already has.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { streamOf } from "./model.js";

function channelSheet() {
  if (document.getElementById("gmx-channels-css")) return;
  document.head.append(el("link#gmx-channels-css", { rel: "stylesheet", href: new URL("../channels/channels.css", import.meta.url).href }));
}

/** A channel destination's view for the channel forms: accept is a re-read. */
const chanView = (data) => ({ client: data.client, accept: () => data.readChannels() });

/** A stream as the format step reads a source. */
function streamShape(s) {
  const v = s && s.video;
  if (!v) return null;
  const fps = typeof v.fps === "number" ? { num: Math.round(v.fps * 1000), den: 1000 } : v.fps;
  return { encoded: true, video: { codec: v.codec, width: v.width, height: v.height, fps, bitrate_kbps: v.kbps || 0 } };
}

/** An empty cell: send this stream to that destination, in a format. */
export async function sendHere(data, group, row, col) {
  const { formatStep } = await import("../renditions/format-step.js");
  const { isRefusal, showRefusal } = await import("../renditions/refusal.js");
  const d = col.dest;
  const format = formatStep(data.client, { platform: d.platform, platformTitle: col.label, shape: streamShape(row.stream), current: d.rendition || { preset: "copy" }, id: () => d.id });
  const refused = el("div", { hidden: true });
  const go = el("button.btn.primary", { text: "Send it" });
  const from = streamOf(group, d);
  const note = from && from !== "*" ? `${col.label} gets ${from} now. It switches to ${row.id} and reconnects.` : `${col.label} switches to ${row.id}.`;
  const m = modal({ title: `Send ${row.id} to ${col.label}`, body: el("div.rt-send", {}, [el("p.dim", { text: note, style: { marginTop: "0" } }), format.node, refused]), footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), go] });
  const send = async (params) => {
    try {
      await data.client.call("channel.destination.set", params);
    } catch (e) {
      if (isRefusal(e)) showRefusal(refused, e, (request) => send({ ...params, rendition: request }));
      else errorToast(e, `Sending ${row.id}`);
      return;
    }
    m.close();
    toast({ text: `${col.label} now sends ${row.id}.` });
    data.readChannels();
  };
  go.onclick = async () => {
    await format.ready;
    const params = { id: group.id, destination: d.id, stream: row.id };
    const rendition = format.value();
    if (rendition !== undefined) params.rendition = rendition;
    go.disabled = true;
    await send(params);
    go.disabled = false;
  };
}

/** The + at the end of a group: a new destination for that channel or show. */
export async function addTo(data, group) {
  if (group.kind === "channel") {
    channelSheet();
    return (await import("../channels/destination-form.js")).addDestination(chanView(data), group.channel);
  }
  const link = data.links.get(group.key);
  if (!link) return toast({ text: `${group.name} is not running. Start it first, then add a destination to it.` });
  const { addDestination } = await import("../outputs/destination.js");
  return addDestination(link.client, { onDone: () => data.refresh(group.key) });
}

/** A route: open what it is, to change its format or anything else. */
export async function edit(data, group, col) {
  if (group.kind === "channel") {
    channelSheet();
    return (await import("../channels/destination-form.js")).editDestination(chanView(data), group.channel, col.dest);
  }
  const link = data.links.get(group.key);
  if (!link) return;
  const { editDestination } = await import("../outputs/destination.js");
  return editDestination(link.client, col.output, { onDone: () => data.refresh(group.key) });
}

export async function startShow(data, group) {
  try {
    await data.client.call("show.start", { id: group.id });
    toast({ text: `Starting ${group.name}.` });
  } catch (e) {
    errorToast(e, `Starting ${group.name}`);
  }
  data.readShows();
}
