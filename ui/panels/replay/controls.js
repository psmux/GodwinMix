// Mark in and out, the speed, play the last event, save the whole buffer and
// stop. The keys I, O and P run the first three from anywhere on the desk.

import { el } from "../../shell/dom.js";
import { toast } from "../../shell/toast.js";
import { call, quiet, markIn, markOut, playLast, picked, setSpeed } from "./panel.js";

const SPEEDS = [1, 0.5, 0.25];

export function controls(changed) {
  const after = (p) => p.then(changed, () => {});
  const speedButtons = SPEEDS.map((s) => el("button.btn.sm", { text: s === 1 ? "1×" : `${s}×`, "data-speed": s, onclick: () => speed(s) }));
  const slider = el("input.grow", { type: "range", min: 10, max: 100, step: 5, value: picked.speed * 100, "aria-label": "Replay speed",
    onchange: () => speed(Number(slider.value) / 100) });
  const shown = el("span.num.sm.replay-speed", { text: "" });
  const root = el("div.col.replay-controls", {}, [
    el("div.row.wrap", {}, [
      el("button.btn", { text: "Mark in", title: "Key I", onclick: () => after(markIn()) }),
      el("button.btn", { text: "Mark out", title: "Key O", onclick: () => after(markOut()) }),
      el("button.btn", { text: "Last 10 s", title: "An event of the last ten seconds", onclick: () => after(call("replay.mark", { seconds: 10 })) }),
      el("button.btn.primary", { text: "Play", title: "The last event, key P", onclick: () => playLast() }),
    ]),
    el("div.row", {}, [el("span.sm.dim", { text: "Speed" }), ...speedButtons, slider, shown]),
    el("div.row.wrap", {}, [
      el("strong.grow.sm", { text: "" }),
      el("button.btn.sm", { text: "Save buffer", title: "The whole buffer of the angle picked, to Media > Replays", onclick: () => saveAll() }),
      el("button.btn.sm.danger", { text: "Stop replay", onclick: () => after(call("replay.stop")) }),
    ]),
  ]);

  function speed(s) {
    setSpeed(s);
    paintSpeed();
    quiet("replay.set", { speed: s });
  }

  function saveAll() {
    toast({ text: "Saving the replay buffer" });
    call("replay.save", { angle: picked.angle || undefined }).then((s) => toast({ text: `Saved to ${s.name}` }), () => {});
  }

  function paintSpeed() {
    shown.textContent = `${picked.speed.toFixed(2)}×`;
    slider.value = String(Math.round(picked.speed * 100));
    for (const b of speedButtons) b.classList.toggle("on", Number(b.dataset.speed) === picked.speed);
  }

  function paint(status) {
    root.hidden = !status.on;
    paintSpeed();
  }

  return { root, paint };
}
