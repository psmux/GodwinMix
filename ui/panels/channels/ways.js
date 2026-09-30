// The ways into a channel: which protocols it has on, where each one is, and
// what an encoder is given for each. No DOM here, so every rule is testable
// with a plain object.
//
// RTMP and RTMPS give OBS its two boxes (Server and Stream key). SRT gives a
// server, a stream id and the key as the passphrase. WHIP gives one URL on
// the page's own port, with the key as the bearer token. On a channel whose
// key is the stream name, the key takes the stream name's place in all four.

import { obsFields } from "./model.js";

export const NAMES = { rtmp: "RTMP", rtmps: "RTMPS", srt: "SRT", whip: "WHIP", webrtc: "WebRTC media", relay: "RTMP" };

/** The protocols a channel has on, in the order the page shows them. */
export function waysIn(channel) {
  const on = channel.protocols || ["rtmp"];
  const out = [];
  if (on.includes("rtmp")) out.push("rtmp");
  if (channel.rtmps && channel.rtmps.enabled) out.push("rtmps");
  if (on.includes("srt")) out.push("srt");
  if (on.includes("whip")) out.push("whip");
  return out;
}

/** The port a protocol is on, from what the core said, or its default. */
export function portOf(model, protocol, channel) {
  if (protocol === "rtmps") return (channel.rtmps && channel.rtmps.port) || 443;
  const row = (model.listeners || []).find((r) => r.protocol === protocol);
  if (row) return row.port;
  if (protocol === "rtmp") return (model.rtmp && model.rtmp.port) || 1935;
  if (protocol === "whip") return Number(location.port) || 80;
  return 9000;
}

/** The address a protocol starts with on one host: `srt://10.0.0.5:9000`. */
export function baseFor(model, channel, protocol, host) {
  const scheme = protocol === "whip" ? "http" : protocol;
  return `${scheme}://${host}:${portOf(model, protocol, channel)}`;
}

/**
 * What an encoder is given for one protocol, as rows of [label, value,
 * holds the key], and the whole URL. `secret` may be the masked form.
 */
export function waysFields(protocol, channel, secret, base, stream = "main") {
  const byName = channel.key_mode === "stream";
  if (protocol === "rtmp" || protocol === "rtmps") {
    const f = obsFields(channel, secret, base, stream);
    return { rows: [["Server", f.server, false], ["Stream key", f.key, true], ["Full URL", f.url, true]], url: f.url };
  }
  if (protocol === "srt") {
    const id = `${channel.app}/${byName ? secret : stream}`;
    const url = byName ? `${base}?streamid=${id}` : `${base}?streamid=${id}&passphrase=${secret}`;
    const rows = [["Server", base, false], ["Stream ID", id, byName]];
    if (!byName) rows.push(["Passphrase", secret, true]);
    rows.push(["Full URL", url, true]);
    return { rows, url };
  }
  const url = `${base}/whip/${channel.app}/${byName ? secret : stream}`;
  const rows = [["WHIP URL", url, byName]];
  if (!byName) rows.push(["Bearer token", secret, true]);
  return { rows, url };
}

/** The line under the panel's title: which ports are open, and for what. */
export function openPorts(listeners) {
  const open = (listeners || []).filter((r) => r.open);
  if (!open.length) return "No ingest port is open. One opens when a channel needs it.";
  const said = open.map((r) => {
    const port = r.last_port ? `${r.port} to ${r.last_port}` : String(r.port);
    const where = r.transport === "udp" ? `${port}/udp` : port;
    const what = r.protocol === "relay" ? "on this machine only" : r.protocol === "whip" ? "(this page's port)" : "";
    const who = r.because && r.because.length ? ` for ${r.because.join(", ")}` : "";
    return `${NAMES[r.protocol] || r.protocol} ${where}${what ? " " + what : ""}${who}`;
  });
  return "Open ports: " + said.join(" · ");
}

/** A listener a channel wants that is not open, with the reason, if any. */
export function portProblems(listeners) {
  return (listeners || [])
    .filter((r) => !r.open && r.problem && r.because && r.because.length)
    .map((r) => `${NAMES[r.protocol] || r.protocol} ${r.port} is not open: ${r.problem}`);
}
