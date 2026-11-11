// One camera's card in the strip: its picture with the tracking boxes over
// it, a red frame when it is on air and a green one when it is next, and
// what the tracker is doing in words. Tap the card to make it next; tap a
// person in the picture to follow them.

import { el } from "../../shell/dom.js";
import { personAt } from "./picture.js";
import { trackWords } from "./words.js";

export function card(cam, on) {
  const canvas = el("canvas.cam-pic", { width: 640, height: 360, "aria-hidden": "true" });
  const name = el("span.cam-name", { text: cam.name || cam.id });
  const tally = el("span.cam-tally");
  const words = el("span.cam-words");
  const dot = el("span.cam-trackdot");
  const root = el("div.cam-card", { role: "button", tabindex: "0", "aria-pressed": "false" }, [
    el("div.cam-picwrap", {}, [canvas, el("div.cam-top", {}, [name, tally])]),
    el("div.cam-under", {}, [dot, words]),
  ]);
  root.dataset.cam = cam.id;
  let seen = null;

  canvas.addEventListener("click", (e) => {
    const r = canvas.getBoundingClientRect();
    const who = personAt(seen, (e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height);
    if (who) {
      e.stopPropagation();
      on.follow(cam.id, who);
    }
  });
  root.addEventListener("click", () => on.pick(cam.id));
  root.addEventListener("keydown", (e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), on.pick(cam.id)));

  return {
    root,
    canvas,
    cam,
    /** Bring the frame, the words and the boxes up to date. */
    sync({ tally: t, track, people, next }) {
      seen = people;
      root.classList.toggle("program", t === "program");
      root.classList.toggle("preview", t === "preview");
      root.classList.toggle("chosen", next);
      root.setAttribute("aria-pressed", String(!!next));
      tally.textContent = t === "program" ? "On air" : t === "preview" ? "Next" : "";
      tally.hidden = !t;
      const text = trackWords(track);
      if (words.textContent !== text) words.textContent = text;
      dot.className = "cam-trackdot " + ((track && track.on && track.state) || "off");
      root.setAttribute("aria-label", `${cam.name || cam.id}${t === "program" ? ", on air" : t === "preview" ? ", next" : ""}. ${text}`);
    },
    get seen() {
      return seen;
    },
  };
}
