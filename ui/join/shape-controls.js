// The shape, fill and rotate controls on /join/, and the line that says how to
// hold the phone. The shape is picked before going live and held while live,
// because a new shape on the air is a new size the mixer has to start over
// on. Fill, fit and rotate change nothing about the size, so they stay free.

import { el } from "../shell/dom.js";
import { SHAPES, FITS, holdHint } from "./shape.js";
import { askMotion } from "./shaper.js";

const MEMORY = "gmx.join.shape";

/** `{shape, link, fit, auto}` this browser chose last time. */
export function rememberedShape(storage = safeStorage()) {
  try {
    return JSON.parse(storage.getItem(MEMORY) || "{}") || {};
  } catch {
    return {};
  }
}

export function rememberShape(choice, storage = safeStorage()) {
  try {
    storage.setItem(MEMORY, JSON.stringify(choice));
  } catch {
    // Private windows and blocked storage: the choice lasts this visit.
  }
}

function safeStorage() {
  try {
    return window.localStorage;
  } catch {
    return { getItem: () => null, setItem: () => {} };
  }
}

/** A row of buttons, one lit, as a radio group a thumb can hit. */
function choices(label, table, onPick) {
  const buttons = Object.entries(table).map(([id, c]) =>
    el("button.btn.pub-choice", { type: "button", role: "radio", "data-id": id, title: c.hint, text: c.label, onclick: () => (askMotion(), onPick(id)) })
  );
  const group = el("div.row.pub-choices", { role: "radiogroup", "aria-label": label }, buttons);
  group.light = (on) => buttons.forEach((b) => b.setAttribute("aria-checked", String(b.dataset.id === on)));
  group.lock = (locked, why) => buttons.forEach((b) => {
    b.disabled = locked;
    b.title = locked ? why : table[b.dataset.id].hint;
  });
  return group;
}

/**
 * The controls for `shaper`. `link` is the shape the mixer's link asked for,
 * kept with the person's choice so it only wins against the same mixer.
 */
export function shapeControls(shaper, link) {
  const save = () => rememberShape({ shape: shaper.s.shape, link: link || "", fit: shaper.s.fit, auto: shaper.s.auto });
  const pick = (changes) => {
    shaper.set(changes);
    save();
    paint();
  };
  const shape = choices("Shape", SHAPES, (id) => pick({ shape: id }));
  const fit = choices("Fill or fit", FITS, (id) => pick({ fit: id }));
  const rotate = el("button.btn", { type: "button", text: "Rotate", title: "A quarter turn clockwise", onclick: () => (askMotion(), shaper.rotate(), paint()) });
  const auto = el("input", { type: "checkbox", checked: shaper.s.auto, onchange: () => (askMotion(), pick({ auto: auto.checked })) });
  const hint = el("div.sm.dim.pub-hint");
  const node = el("div.col.pub-shape", {}, [
    el("div.row.pub-field.sm", {}, [el("span.pub-label.dim", { text: "Shape" }), shape]),
    el("div.row.pub-field.sm", {}, [el("span.pub-label.dim", { text: "Picture" }), fit, rotate]),
    el("label.row.sm", { title: "With auto rotate off, the phone's picture stays sideways when the phone is. This turns it upright." }, [
      auto,
      el("span", { text: "Keep the picture upright when the phone turns" }),
    ]),
    hint,
  ]);
  let live = false;
  function paint() {
    shape.light(shaper.s.shape);
    fit.light(shaper.s.fit);
    shape.lock(live, "Stop to change the shape. Fill, fit and rotate work while live.");
    hint.textContent = holdHint(shaper.s.shape, shaper.s.device);
  }
  // How the phone is held changes without a tap, so the hint is read again now and then.
  const timer = setInterval(paint, 1000);
  paint();
  return {
    node,
    setLive(on) {
      live = on;
      paint();
    },
    destroy: () => clearInterval(timer),
  };
}
