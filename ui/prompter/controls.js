// The bar a tap brings up: play, speed, markers, this screen's mirror and
// full screen. It goes again after a few seconds so nothing covers the words.

import { el } from "../shell/dom.js";

const HIDE_MS = 3500;

export const MIRROR_CHOICES = [
  ["desk", "Mirror: as the desk"],
  ["on", "Mirror: on here"],
  ["off", "Mirror: off here"],
];

export function controls(root, run) {
  const button = (text, move, title) => el("button.pbtn", { type: "button", text, title, onclick: () => run(move) });
  const play = button("Play", { method: "toggle" }, "Space");
  const mirror = button(MIRROR_CHOICES[0][1], { local: "mirror" }, "M");
  const status = el("div.pstatus", { role: "status" });
  const note = el("div.pnote", { role: "alert" });
  const bar = el("div.pbar", {}, [
    el("div.prow", {}, [
      button("Back", { method: "prompter.seek", params: { step: -1 } }, "Page Up"),
      button("Slower", { method: "prompter.speed", params: { by: -0.1 } }, "Up arrow"),
      play,
      button("Faster", { method: "prompter.speed", params: { by: 0.1 } }, "Down arrow"),
      button("Next", { method: "prompter.seek", params: { step: 1 } }, "Page Down"),
    ]),
    el("div.prow", {}, [mirror, button("Full screen", { local: "fullscreen" }, "F")]),
    status,
  ]);
  root.append(bar, note);

  let timer = 0;
  const show = () => {
    bar.classList.add("shown");
    document.body.classList.remove("idle");
    clearTimeout(timer);
    timer = setTimeout(() => {
      bar.classList.remove("shown");
      document.body.classList.add("idle");
    }, HIDE_MS);
  };
  root.addEventListener("pointerdown", show);
  root.addEventListener("pointermove", (e) => e.pointerType === "mouse" && show());
  show();

  let noteTimer = 0;
  return {
    show,
    paint({ playing, mirrorChoice, line }) {
      play.textContent = playing ? "Pause" : "Play";
      mirror.textContent = MIRROR_CHOICES.find(([k]) => k === mirrorChoice)[1];
      status.textContent = line;
    },
    say(words, ms = 5000) {
      note.textContent = words || "";
      note.hidden = !words;
      clearTimeout(noteTimer);
      if (words && ms) noteTimer = setTimeout(() => (note.hidden = true), ms);
    },
  };
}
