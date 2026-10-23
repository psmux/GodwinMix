// The ladder picker for HLS: which sizes a viewer's player may switch
// between. Each ladder is drawn as its rungs, nested, to scale, so four
// steps looks like four steps before anyone reads a number. A custom rung
// has an Advanced group of its own (rung-advanced.js), and the sound every
// rung shares has one bitrate for the ladder.

import { el } from "../../shell/dom.js";
import { describe, ladders, usable, rate } from "./model.js";
import { keepAdvanced, rungAdvanced } from "./rung-advanced.js";

const HEIGHTS = [1080, 720, 540, 480, 360, 240];
const BITRATE = { 1080: 6000, 720: 3000, 540: 2000, 480: 1400, 360: 800, 240: 400 };

/** One rung as a RenditionRequest. Width follows from 16:9, kept even. */
export function rung(height, kbps) {
  const width = Math.round((height * 16) / 9 / 2) * 2;
  return {
    id: `${height}p`,
    container: "hls",
    video: { codec: "h264", width, height, bitrate_kbps: kbps || BITRATE[height] || 1000, keyframe_ms: 2000 },
    audio: { codec: "aac", bitrate_kbps: 128 },
  };
}

/**
 * A saved ladder's rungs, as the picker keeps them: each one with the
 * container and audio the picker writes, and whatever else it asked for.
 */
export function rungsFrom(ladder) {
  return (ladder || []).filter((r) => r && r.video && r.video.height).map((r) => {
    const made = keepAdvanced(rung(r.video.height, r.video.bitrate_kbps), r);
    if (r.video.codec) made.video.codec = r.video.codec;
    return made;
  });
}

/** The sound bitrate of a saved ladder: its top rung's, 128 when it names none. */
export function soundOf(ladder) {
  const a = ((ladder || [])[0] || {}).audio;
  return (a && a.bitrate_kbps) || 128;
}

/** The rungs to send, each with the ladder's one sound bitrate. */
export function withSound(rungs, kbps) {
  const n = Math.round(Number(kbps));
  const sound = n >= 32 && n <= 512 ? n : 128;
  return rungs.map((r) => ({ ...r, video: { ...r.video }, audio: { codec: "aac", bitrate_kbps: sound } }));
}

/** What is wrong with a custom ladder, or "". */
export function ladderError(rungs) {
  if (!rungs.length) return "Add at least one rung.";
  const heights = rungs.map((r) => r.video.height);
  if (new Set(heights).size !== heights.length) return "Two rungs are the same size. Change or remove one.";
  return "";
}

/** The ladder as a small picture: one rectangle per rung, nested at scale. */
export function ladderArt(requests) {
  // To one scale for every card, 1080p full, so three sizes look smaller than four.
  const tallest = Math.max(1080, ...requests.map((r) => (r.video || {}).height || 0));
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 64 36");
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("rnd-ladderart");
  for (const r of [...requests].sort((a, b) => b.video.height - a.video.height)) {
    const k = r.video.height / tallest;
    const rect = document.createElementNS(ns, "rect");
    Object.entries({ x: 1, y: 35 - 34 * k, width: 62 * k, height: 34 * k, rx: 2 }).forEach(([a, v]) => rect.setAttribute(a, String(v)));
    svg.appendChild(rect);
  }
  return svg;
}

/**
 * @param {object[]} presets  from `rendition.presets`
 * @param {object} [current]  the `rendition` an edit opens on
 * @returns {{node, value: () => ({preset: string}|{ladder: object[]}|{error: string})}}
 */
export function ladderPicker(presets, current) {
  const offered = ladders(presets);
  const cards = el("div.rnd-cards.rnd-ladders", { role: "radiogroup", "aria-label": "Sizes for viewers" });
  const rows = el("div.rnd-rungs");
  const saved = current && current.ladder && current.ladder.length ? current.ladder : null;
  const sound = el("input", { type: "number", min: "32", max: "512", step: "16", value: String(soundOf(saved)), "aria-label": "Sound bitrate in kb/s" });
  const soundRow = el("label.rnd-rungsound", {}, [el("span", { text: "Sound" }), sound, el("span.rnd-dim", { text: "kb/s, one track every size shares" })]);
  const custom = el("div.rnd-customwrap", { hidden: true }, [rows, soundRow, el("button.btn.rnd-addrung", { type: "button", text: "Add a rung", onclick: () => addRung() })]);
  let choice = saved ? "custom" : current && current.preset ? current.preset : (offered.find(usable) || {}).id || "custom";
  let rungs = saved ? rungsFrom(saved) : (offered.find((p) => p.id === "abr-ladder-3") || offered[0] || { ladder: [rung(720), rung(480), rung(360)] }).ladder.map((r) => rung(r.video.height, r.video.bitrate_kbps));

  const pick = (id) => {
    choice = id;
    for (const c of cards.children) c.setAttribute("aria-checked", String(c.dataset.id === id));
    custom.hidden = id !== "custom";
  };
  const drawRungs = () => {
    rows.replaceChildren(...rungs.map((r, i) => rungRow(r, i)));
    cards.lastChild.replaceChildren(...customFace(rungs));
  };
  const addRung = () => {
    const free = HEIGHTS.find((h) => !rungs.some((r) => r.video.height === h)) || 240;
    rungs = [...rungs, rung(free)].sort((a, b) => b.video.height - a.video.height);
    drawRungs();
  };
  function rungRow(r, i) {
    const size = el("select", { "aria-label": "Height" }, HEIGHTS.map((h) => el("option", { value: String(h), text: `${h}p`, selected: h === r.video.height })));
    const kbps = el("input", { type: "number", min: "200", max: "20000", step: "100", value: String(r.video.bitrate_kbps), "aria-label": "Bitrate in kb/s" });
    // A new size or bitrate keeps what the rung's Advanced group says.
    size.onchange = () => { rungs[i] = keepAdvanced(rung(Number(size.value)), r); drawRungs(); };
    kbps.onchange = () => { rungs[i] = keepAdvanced(rung(r.video.height, Number(kbps.value)), r); drawRungs(); };
    const remove = el("button.btn.icon", { type: "button", text: "×", title: "Remove this rung", "aria-label": `Remove ${r.video.height}p`, onclick: () => { rungs.splice(i, 1); drawRungs(); } });
    const more = el("div.rnd-rungmore");
    rungAdvanced(r, (next) => { rungs[i] = next; }).then((form) => more.appendChild(form));
    return el("div.rnd-rungbox", {}, [el("div.rnd-rung", {}, [size, kbps, el("span.rnd-dim", { text: "kb/s" }), remove]), more]);
  }

  for (const p of offered) cards.appendChild(ladderCard(p, p.id, p.title, p.ladder, pick));
  // A saved preset this machine no longer lists still opens on Custom.
  if (choice !== "custom" && !offered.some((p) => p.id === choice)) choice = "custom";
  cards.appendChild(ladderCard({}, "custom", "Custom", rungs, pick));
  drawRungs();
  pick(choice);
  return {
    node: el("div", {}, [cards, custom]),
    value() {
      if (choice !== "custom") return { preset: choice };
      const wrong = ladderError(rungs);
      return wrong ? { error: wrong } : { ladder: withSound(rungs, sound.value) };
    },
  };
}

function ladderCard(p, id, title, requests, pick) {
  const b = el("button.rnd-card.rnd-laddercard", { type: "button", role: "radio", "aria-checked": "false", disabled: !usable(p), onclick: () => pick(id) });
  b.dataset.id = id;
  b.append(...(id === "custom" ? customFace(requests) : face(title, requests, p)));
  if (!usable(p)) b.classList.add("off");
  return b;
}

function face(title, requests, p) {
  const sizes = requests.map((r) => `${r.video.height}p`).join(", ");
  const top = requests.reduce((n, r) => n + (r.video.bitrate_kbps || 0), 0);
  return [
    ladderArt(requests),
    el("span.rnd-ctitle", { text: title }),
    el("span.rnd-meta", { text: sizes }),
    el("span.rnd-note", { text: usable(p) ? `${rate(top)} going out in all` : p.why || "This machine cannot make it" }),
  ];
}

function customFace(rungs) {
  const art = rungs.length ? ladderArt(rungs) : el("span.rnd-ladderart");
  const d = rungs.map((r) => describe(r).size && `${r.video.height}p`).join(", ");
  return [art, el("span.rnd-ctitle", { text: "Custom" }), el("span.rnd-meta", { text: d || "No rungs yet" }), el("span.rnd-note", { text: "Your own sizes and bitrates" })];
}
