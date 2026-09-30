// The "Ways in" part of a channel's settings: which protocols it takes, and
// RTMPS with its port and the certificate it answers with.
//
// A switch here opens or closes a port on the mixer when Save is pressed,
// and only then: nothing listens that no channel uses. The certificate is
// the mixer's, shared by every channel with RTMPS on, so it is set the
// moment it is uploaded or made rather than with Save.

import { el } from "../../shell/dom.js";
import { toast, errorToast } from "../../shell/toast.js";
import { toggle } from "./fields.js";

const WAYS = [
  ["rtmp", "RTMP", "OBS, vMix, a phone or a hardware encoder. The usual."],
  ["srt", "SRT", "For a contribution feed over the internet. The key is the passphrase."],
  ["whip", "WHIP", "WebRTC from a browser or OBS, on this page's own port. The key is the bearer token."],
];

export function waysSection(view, channel) {
  const on = channel.protocols || ["rtmp"];
  const switches = WAYS.map(([id, label, note]) => ({ id, ...toggle(label, on.includes(id), note) }));
  const rtmps = channel.rtmps || { enabled: false, port: 443 };
  const secure = toggle("RTMPS", rtmps.enabled, "RTMP inside TLS, on a port of its own. It needs a certificate.");
  const port = el("input.chn-mono.chn-port", { type: "number", min: 1, max: 65535, value: rtmps.port || 443, "aria-label": "RTMPS port" });
  const cert = certificate(view);
  const extra = el("div.chn-rtmps", { hidden: !rtmps.enabled }, [
    el("label.chn-portline", {}, [el("span", { text: "Port" }), port, el("small.chn-dim", { text: "443 is what encoders expect. Below 1024 some systems need the mixer to run as an administrator." })]),
    cert.node,
  ]);
  secure.box.addEventListener("change", () => { extra.hidden = !secure.box.checked; });
  const node = el("div.chn-field.chn-waysin", {}, [
    el("label", { text: "Ways in" }),
    el("div.chn-group", {}, [...switches.map((s) => s.node), secure.node]),
    extra,
  ]);
  return {
    node,
    value: () => ({
      protocols: switches.filter((s) => s.value()).map((s) => s.id),
      rtmps: { enabled: secure.value(), port: Number(port.value) || 443 },
    }),
  };
}

/** What `channel.set` needs to hear about the ways in: only what changed. */
export function waysParams(channel, v) {
  const out = {};
  const before = [...(channel.protocols || ["rtmp"])].sort().join();
  if (v.protocols && [...v.protocols].sort().join() !== before) out.protocols = v.protocols;
  const was = channel.rtmps || { enabled: false, port: 443 };
  if (v.rtmps && (v.rtmps.enabled !== was.enabled || (v.rtmps.enabled && v.rtmps.port !== was.port))) out.rtmps = v.rtmps;
  return out;
}

function certificate(view) {
  const status = el("p.chn-certline");
  const draw = (c) => {
    status.textContent = c
      ? `${c.source === "self_signed" ? "Self signed, for " + (c.names || []).join(", ") : "Uploaded"}. Fingerprint ${String(c.fingerprint).slice(0, 23)}…`
      : "No certificate yet.";
    status.classList.toggle("bad", !c);
  };
  draw(view.model.certificate);
  const files = el("input", { type: "file", accept: ".pem,.crt,.cer,.key", multiple: true, hidden: true });
  const upload = el("button.btn.sm", { type: "button", text: "Upload certificate and key", onclick: () => files.click() });
  const make = el("button.btn.sm", { type: "button", text: "Make a self signed one" });
  const done = (c, said) => {
    view.model.certificate = c;
    draw(c);
    toast({ text: said });
  };
  files.onchange = async () => {
    const texts = await Promise.all([...files.files].map((f) => f.text()));
    files.value = "";
    const key = texts.find((t) => t.includes("PRIVATE KEY"));
    const cert = texts.filter((t) => t.includes("BEGIN CERTIFICATE")).join("\n");
    if (!key || !cert) return errorToast(new Error("choose both files: the certificate (.crt or .pem) and its private key (.key)."), "Upload certificate");
    try {
      done(await view.client.call("channel.certificate.set", { cert, key }), "RTMPS has its certificate.");
    } catch (e) {
      errorToast(e, "Upload certificate");
    }
  };
  make.onclick = async () => {
    make.disabled = true;
    try {
      done(await view.client.call("channel.certificate.generate", {}), "RTMPS has a self signed certificate. Tell the encoder to accept it.");
    } catch (e) {
      errorToast(e, "Make a certificate");
    } finally {
      make.disabled = false;
    }
  };
  return { node: el("div.chn-cert", {}, [status, el("div.chn-certbtns", {}, [upload, make, files])]) };
}
