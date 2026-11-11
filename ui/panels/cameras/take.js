// Step three: take it live. Cut and Fade put the next camera on air, Black
// fades everything out and back, Lower third puts the name strap up or takes
// it down. Each goes to GodwinMix, to the ATEM or to both, by the switcher's
// mode: in `follow` GodwinMix takes and the ATEM follows it, in `lead` the
// ATEM takes and GodwinMix follows it, and `own` asks which.

import { el } from "../../shell/dom.js";
import { inputOf, route } from "./words.js";

/** The scene GodwinMix keys as the lower third. */
export const LOWER_THIRD = "Lower third";

const PICKS = [["mix", "GodwinMix"], ["atem", "Switcher"], ["both", "Both"]];

/** The calls for one action, as functions, so a test can read them without a DOM. */
export function plan(action, { api, cam, atem, pick, lower }) {
  const r = route(atem, pick);
  const sw = atem && atem.id;
  const input = inputOf(atem, cam);
  const steps = [];
  if (action === "cut" || action === "fade") {
    if (!cam) return steps;
    if (r.mix) steps.push(() => api.take(cam.source || cam.id, action === "fade" ? "fade" : undefined));
    if (r.atem && input !== null) {
      steps.push(() => api.atemPreview(sw, input));
      steps.push(() => (action === "fade" ? api.atemAuto(sw) : api.atemCut(sw)));
    }
  } else if (action === "black") {
    if (r.mix) steps.push(() => api.black());
    if (r.atem) steps.push(() => api.atemFtb(sw));
  } else if (action === "lower") {
    if (r.mix) steps.push(() => api.key(LOWER_THIRD, !lower));
    if (r.atem) steps.push(() => api.atemDsk(sw, 0, "auto"));
  }
  return steps;
}

export function takeStep(model, act) {
  let pick = "atem";
  let lower = false;
  const where = el("p.cam-route");
  const seg = el("div.cam-seg", { role: "radiogroup", "aria-label": "Send takes to" },
    PICKS.map(([v, label]) => el("button", { type: "button", role: "radio", text: label, "data-v": v, onclick: () => ((pick = v), sync()) })));
  const big = (cls, text, sub, action) =>
    el(`button.cam-take.${cls}`, { type: "button", onclick: () => run(action) }, [el("span.cam-take-word", { text }), el("span.cam-take-sub", { text: sub })]);
  const cut = big("cut", "Cut", "Straight to it", "cut");
  const fade = big("fade", "Fade", "A soft mix", "fade");
  const black = big("black", "Black", "Fade to black", "black");
  const strap = big("lower", "Lower third", "Name on screen", "lower");

  function run(action) {
    const atem = model.atem && model.atem.connected !== false ? model.atem : null;
    const steps = plan(action, { api: model.api, cam: model.current, atem, pick, lower });
    if (action === "lower") lower = !lower;
    act(async () => {
      for (const s of steps) await s();
    });
    sync();
  }

  function sync() {
    const atem = model.atem && model.atem.connected !== false ? model.atem : null;
    const r = route(atem, pick);
    where.textContent = r.words;
    seg.hidden = !atem || atem.mode !== "own";
    for (const b of seg.children) b.setAttribute("aria-checked", String(b.dataset.v === r.pick));
    const dsk = atem && r.atem && atem.dsks && atem.dsks[0];
    const up = dsk ? dsk.on : lower;
    strap.classList.toggle("on", up);
    strap.setAttribute("aria-pressed", String(up));
    strap.querySelector(".cam-take-sub").textContent = up ? "Up now, tap to take down" : "Name on screen";
    const ftb = atem && r.atem && atem.ftb && atem.ftb.on;
    black.classList.toggle("on", !!ftb);
    const cam = model.current;
    cut.disabled = fade.disabled = !cam;
    cut.querySelector(".cam-take-sub").textContent = cam ? `${cam.name || cam.id} goes on air` : "Pick a camera first";
  }

  const root = el("section.cam-step.cam-step-take", {}, [
    el("h3.cam-steptitle", {}, [el("span.cam-stepno", { text: "3" }), "Take it live"]),
    el("div.cam-takes", {}, [cut, fade]),
    el("div.cam-takes.small", {}, [black, strap]),
    el("div.cam-routerow", {}, [where, seg]),
  ]);
  return { root, sync, run };
}
