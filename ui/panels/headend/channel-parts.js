// Two parts of the channel editor: an input, as a transport and the
// fields that transport asks for (the main input and the backup share
// it), and the list of outputs beside the HLS one.

import { el } from "../../shell/dom.js";
import { ASKS, TRANSPORTS, inputFields, inputFrom, inputUri } from "./setup.js";
import { choice, field, num, text, tick } from "./form.js";

const HINTS = {
  group: "A multicast group, 239.0.0.1 say.",
  interface: "The card to join on, by name or address. Empty: the default route's.",
  program: "Of a feed that carries several. Empty: the first.",
  address: "192.0.2.10:9000 to call a sender, or :9000 to listen for one.",
  latency_ms: "Empty: the sender's.",
  url: "",
  path: "On the station's machine.",
};
const PLACE = { group: "239.0.0.1", port: "5000", interface: "eth1", program: "first", address: "192.0.2.10:9000", passphrase: "", latency_ms: "120", path: "/clips/loop.ts" };
const URL_PLACE = { rtmp: "rtmp://192.0.2.20/live/key", rtsp: "rtsp://192.0.2.20/stream1", hls: "https://example.com/live/index.m3u8", other: "udp://@239.0.0.1:5000" };
const LABEL = { group: "Group", port: "Port", interface: "Network card", program: "Program number", address: "Address", passphrase: "Passphrase", latency_ms: "Latency, ms", url: "Address", path: "Path" };

/** One input: a transport and its fields, with the address it makes shown. */
export function inputBox(input, who = "Input") {
  const f = inputFields(input);
  const keep = { ...((input && input.params) || {}) };
  const kind = choice(TRANSPORTS, f.kind, `${who} transport`);
  const box = {};
  for (const k of Object.keys(LABEL)) {
    const v = f[k] ?? "";
    box[k] = k === "port" || k === "latency_ms" || k === "program"
      ? num(v, PLACE[k], `${who} ${LABEL[k].toLowerCase()}`, { min: k === "port" ? "1" : "0", max: k === "port" ? "65535" : null })
      : text(v, PLACE[k], `${who} ${LABEL[k].toLowerCase()}`, k === "passphrase" ? { type: "password" } : {});
  }
  const fields = Object.fromEntries(Object.keys(LABEL).map((k) => [k, field(LABEL[k], box[k], HINTS[k], `.hs-f-${k}`)]));
  const uri = el("code.hs-uri", { "aria-live": "polite" });
  const node = el("div.hs-input", {}, [field("Transport", kind), el("div.hs-grid", {}, Object.values(fields)), el("p.hs-note.dim", {}, ["Address: ", uri])]);
  const answers = () => Object.fromEntries([["kind", kind.value], ...Object.keys(LABEL).map((k) => [k, box[k].value])]);
  const sync = () => {
    const asks = ASKS[kind.value] || ASKS.other;
    for (const [k, node] of Object.entries(fields)) node.hidden = !asks.includes(k);
    box.url.placeholder = URL_PLACE[kind.value] || "";
    uri.textContent = inputUri(answers()) || "fill in the fields above";
  };
  node.addEventListener("input", sync);
  node.addEventListener("change", sync);
  sync();
  return {
    node,
    box,
    kind,
    read: () => inputFrom(answers(), keep),
    uri: () => inputUri(answers()),
  };
}

const OUT_KINDS = [["udp", "UDP out", "udp://239.0.1.1:5000"], ["srt", "SRT", "srt://192.0.2.30:9000"], ["rtmp", "RTMP push", "rtmp://192.0.2.40/live/key"]];

/** The outputs beside HLS, each with Copy as it comes. */
export function outputsBox(outputs) {
  const rows = el("div.hs-outs");
  const add = (o = {}, kind) => {
    const uri = text(o.uri || "", (OUT_KINDS.find((k) => k[0] === kind) || OUT_KINDS[0])[2], "Output address");
    if (!o.uri && kind) uri.value = `${kind}://`;
    const copy = tick(!!o.copy, "Copy as it comes", "Copy as it comes, whatever the profile");
    const del = el("button.btn.icon", { type: "button", text: "×", title: "Remove this output", "aria-label": "Remove this output" });
    const row = el("div.hs-out", {}, [uri, copy.node, del]);
    row.read = () => ({ ...(o.id ? { id: o.id } : {}), uri: uri.value.trim(), ...(copy.box.checked ? { copy: true } : {}) });
    del.onclick = () => row.remove();
    rows.append(row);
    return row;
  };
  for (const o of outputs || []) add(o);
  const buttons = el("div.hs-row", {}, OUT_KINDS.map(([k, words]) => el("button.btn", { type: "button", text: `Add ${words}`, "data-out": k, onclick: () => add({}, k).querySelector("input").focus() })));
  return {
    node: el("div.hs-outbox", {}, [rows, buttons]),
    add,
    read: () => [...rows.children].map((r) => r.read()).filter((o) => o.uri && !/^[a-z]+:\/\/$/i.test(o.uri)),
  };
}
