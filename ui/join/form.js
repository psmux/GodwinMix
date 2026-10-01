// The publisher's DOM, and nothing that acts on it. Built from the shell's
// one element helper so it reads the same in the mixer's page and alone.

import { el, clear } from "../shell/dom.js";

/** What a state is called on screen, and the dot beside it. */
export const STATE_WORDS = {
  idle: ["Not publishing", ""],
  connecting: ["Connecting", "connecting"],
  live: ["Live", "live"],
  reconnecting: ["Reconnecting", "stalled"],
  stopped: ["Stopped", ""],
};

export function buildForm(labels = {}) {
  const r = {};
  r.dot = el("span.dot");
  r.state = el("strong", { text: STATE_WORDS.idle[0] });
  r.where = el("span.sm.dim.ellipsis.grow", { text: labels.where || "" });
  r.error = el("div.pub-error.sm", { role: "alert" });
  r.preview = el("video.pub-preview", { muted: true, autoplay: true, playsInline: true, "aria-label": "Camera preview" });
  r.noPicture = el("div.pub-nopicture.dim.sm", { text: "No camera" });
  r.level = el("div.pub-level");
  r.camera = el("select", { "aria-label": "Camera" });
  r.mic = el("select", { "aria-label": "Microphone" });
  r.cameraMute = el("button.btn", { type: "button", text: "Turn camera off" });
  r.micMute = el("button.btn", { type: "button", text: "Mute microphone" });
  r.processing = el("input", { type: "checkbox", checked: true });
  r.go = el("button.btn.primary", { type: "button", text: labels.go || "Go live" });
  r.stats = el("div.pub-stats.sm.dim.num");
  r.root = el("div.pub.col", {}, [
    el("div.row", {}, [r.dot, r.state, r.where]),
    r.error,
    el("div.pub-picture", {}, [r.preview, r.noPicture]),
    el("div.pub-meter", { title: "Microphone level" }, [r.level]),
    field("Camera", r.camera),
    field("Microphone", r.mic),
    el("label.row.sm", { title: "Off for a music microphone or an instrument" }, [
      r.processing,
      el("span", { text: "Echo cancellation, noise suppression and automatic gain" }),
    ]),
    el("div.row.pub-buttons", {}, [r.cameraMute, r.micMute, el("span.grow"), r.go]),
    r.stats,
  ]);
  return r;
}

function field(label, select) {
  return el("label.pub-field.row.sm", {}, [el("span.pub-label.dim", { text: label }), select]);
}

/** Fill a device list, keeping `current` chosen. `offLabel` adds a "none" choice. */
export function fillSelect(select, devices, current, offLabel) {
  clear(select);
  if (offLabel) select.appendChild(el("option", { value: "off", text: offLabel }));
  for (const d of devices) select.appendChild(el("option", { value: d.id, text: d.label }));
  select.value = devices.some((d) => d.id === current) ? current : offLabel && current === "off" ? "off" : select.options[0]?.value || "";
}

/** Paint a session state onto the form. */
export function paintState(r, s, labels = {}) {
  const [word, dot] = STATE_WORDS[s.state] || STATE_WORDS.idle;
  let text = word;
  if (s.state === "reconnecting" && s.retryIn) text += ` in ${Math.ceil(s.retryIn / 1000)} s`;
  r.state.textContent = text;
  r.dot.className = `dot ${dot}`.trim();
  r.error.textContent = s.error || "";
  const busy = s.state === "connecting" || s.state === "live" || s.state === "reconnecting";
  r.go.textContent = busy ? "Stop" : labels.go || "Go live";
  r.go.classList.toggle("primary", !busy);
}

export function paintMutes(r, tracks) {
  const v = tracks.video;
  const a = tracks.audio;
  r.cameraMute.disabled = !v;
  r.micMute.disabled = !a;
  r.cameraMute.textContent = v && !v.enabled ? "Turn camera on" : "Turn camera off";
  r.micMute.textContent = a && !a.enabled ? "Unmute microphone" : "Mute microphone";
  r.cameraMute.setAttribute("aria-pressed", v && !v.enabled ? "true" : "false");
  r.micMute.setAttribute("aria-pressed", a && !a.enabled ? "true" : "false");
}

/** The mixer serves https on its own port; this page says how to reach it. */
const HTTPS_HELP = "https://github.com/psmux/GodwinMix/blob/main/docs/how-to/serve-https.md";

/** A page that cannot have a camera: the reason, and every control off. */
export function blocked(r, text) {
  r.error.textContent = text;
  if (window.isSecureContext === false) {
    r.error.append(" ", el("a", { href: HTTPS_HELP, target: "_blank", rel: "noopener", text: "How to open the mixer over https." }));
  }
  for (const b of [r.go, r.cameraMute, r.micMute, r.camera, r.mic, r.processing]) b.disabled = true;
  return { state: () => "blocked", active: () => false, stop() {}, setVisible() {}, destroy: () => r.root.remove() };
}
