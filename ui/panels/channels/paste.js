// Paste several addresses: one push destination per line, the way a Livebox
// "Bulk RTMP url" box takes them. Each line is an rtmp://, rtmps:// or srt://
// address, with the stream key on the end of it or after a space. Every line
// becomes one `channel.destination.add`, and a line that fails is listed with
// its number and the core's reason, so the rest are not lost with it.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast } from "../../shell/toast.js";
import { platformOfHost } from "../../client/destinations.js";

const SCHEMES = { "rtmp:": "custom", "rtmps:": "custom", "srt:": "srt" };

/**
 * The lines of a paste as `{line, platform, label, server, key?}`, or `{line, error}`
 * for a line that cannot be one. Empty lines and lines starting with # are
 * skipped, so a list copied out of a notes file still works.
 */
export function parsePasted(text) {
  const out = [];
  String(text || "").split(/\r?\n/).forEach((raw, i) => {
    const line = i + 1;
    const words = raw.trim().split(/\s+/).filter(Boolean);
    if (!words.length || words[0].startsWith("#")) return;
    if (words.length > 2) return out.push({ line, error: "More than an address and a key. Put one destination on each line." });
    const [server, key] = words;
    let url;
    try {
      url = new URL(server);
    } catch {
      return out.push({ line, error: `"${short(server)}" is not an address. Start it with rtmp://, rtmps:// or srt://.` });
    }
    const platform = SCHEMES[url.protocol];
    if (!platform) return out.push({ line, error: `${url.protocol}// cannot be sent to. Use rtmp://, rtmps:// or srt://.` });
    if (!url.hostname) return out.push({ line, error: "The address has no host." });
    const path = url.pathname.split("/").filter(Boolean);
    if (platform === "custom" && !key && path.length < 2) {
      return out.push({ line, error: "No stream key. Put it on the end of the address or after a space." });
    }
    // Named for whose server it is, so a list of them is not all "Custom RTMP".
    const known = platformOfHost(url.hostname);
    const label = known && known.id !== "custom" && known.id !== "srt" ? known.title : url.hostname;
    out.push(key ? { line, platform, label, server, key } : { line, platform, label, server });
  });
  return out;
}

function short(text) {
  return text.length > 40 ? text.slice(0, 37) + "..." : text;
}

/**
 * Add every good line to a channel, one call each, in order. Answers the
 * failures, by line, with the reason the core gave.
 */
export async function addPasted(view, channelId, lines) {
  const failed = [];
  let added = 0;
  for (const item of lines) {
    if (item.error) {
      failed.push(item);
      continue;
    }
    const { line, ...rest } = item;
    try {
      view.accept(await view.client.call("channel.destination.add", { id: channelId, ...rest }));
      added += 1;
    } catch (e) {
      failed.push({ line, error: (e && e.message) || String(e) });
    }
  }
  return { added, failed };
}

/**
 * The dialog. `channel` is the one whose header it was opened from; opened
 * from the palette there may be none, and then it asks which.
 */
export function pasteAddresses(view, channel) {
  const channels = view.model.list();
  const pick = el("select", { "aria-label": "Channel" }, channels.map((c) => el("option", { value: c.id, text: c.name || c.id })));
  if (channel) pick.value = channel.id;
  const area = el("textarea.chn-mono.chn-paste", {
    rows: 6,
    spellcheck: "false",
    "aria-label": "Addresses, one per line",
    placeholder: "rtmp://a.rtmp.youtube.com/live2 xxxx-xxxx-xxxx\nrtmps://live-api-s.facebook.com:443/rtmp/FB-123\nsrt://10.0.0.9:9000?passphrase=secret",
  });
  const report = el("ul.chn-pastefail", { hidden: true });
  const add = el("button.btn.primary", { text: "Add them" });
  const m = modal({
    title: "Paste several addresses",
    body: el("div.col", {}, [
      channel || channels.length < 2 ? null : el("label.col.sm", {}, [el("span.chn-kicker", { text: "Channel" }), pick]),
      el("p.chn-dim.sm", { text: "One push destination per line, with its stream key on the end of the address or after a space. Each one starts switched on." }),
      area,
      report,
    ]),
    footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), add],
  });
  // The modal focuses the first button it finds, which is its close button.
  area.focus();
  add.onclick = async () => {
    const lines = parsePasted(area.value);
    if (!lines.length) return area.focus();
    add.disabled = true;
    const { added, failed } = await addPasted(view, pick.value, lines);
    add.disabled = false;
    if (added) toast({ text: `Added ${added} push destination${added === 1 ? "" : "s"}.` });
    if (!failed.length) return m.close();
    // Keep only the lines that failed, so fixing them and pressing again
    // does not add the good ones twice.
    const rows = area.value.split(/\r?\n/);
    area.value = failed.map((f) => rows[f.line - 1]).join("\n");
    report.replaceChildren(...failed.map((f) => el("li", { text: `Line ${f.line}: ${f.error}` })));
    report.hidden = false;
  };
  return m;
}
