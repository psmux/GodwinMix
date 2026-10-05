// A finger on the page: a long press is the item menu, a double tap opens,
// a scroll is a scroll, and a tile moves by its grip. Synthetic pointer events
// against a detached root, so nothing here needs a touch screen.

import { watchTouch, ignored, SLOP } from "../shell/touch.js";
import { dragHandle } from "../shell/pointer.js";
import { trayTests } from "./touch-tray.js";
import { el } from "../shell/dom.js";

const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));
const PRESS = 40;

function finger(target, type, x, y, opts = {}) {
  target.dispatchEvent(new PointerEvent(type, {
    pointerType: "touch", pointerId: 11, isPrimary: true, button: type === "pointermove" ? -1 : 0,
    clientX: x, clientY: y, bubbles: true, cancelable: true, ...opts,
  }));
}

const tap = (target, x = 20, y = 20) => {
  finger(target, "pointerdown", x, y);
  finger(target, "pointerup", x, y);
};

/** A root with a tile, a text field and a grip in it, watched by the module. */
function stage() {
  const handle = dragHandle();
  const tile = el("div.tile", { "data-id": "cam-1", "data-drop": "cam-1" }, [el("div.bar", {}, [handle, el("span.name", { text: "Camera" })])]);
  const field = el("input", { type: "text" });
  const root = el("div", { style: { position: "fixed", left: "0", top: "0", width: "300px" } }, [tile, field]);
  document.body.append(root);
  const seen = [];
  const s = { root, tile, field, handle, seen, menus: true };
  for (const type of ["contextmenu", "dblclick", "click"]) {
    root.addEventListener(type, (e) => {
      seen.push([type, e.target, e.clientX, e.clientY]);
      // What a panel with an item menu does with the event.
      if (type === "contextmenu" && s.menus) e.preventDefault();
    });
  }
  const stop = watchTouch(root, { longPress: PRESS });
  s.done = () => { stop(); root.remove(); };
  return s;
}

async function gestures(test, eq, ok) {
  const s = stage();
  const name = s.tile.querySelector(".name");

  finger(name, "pointerdown", 30, 40);
  await tick(PRESS + 30);
  finger(name, "pointerup", 30, 40);
  test("a long press is a contextmenu on what the finger was on, where it was", () => {
    const menus = s.seen.filter(([t]) => t === "contextmenu");
    eq(menus.length, 1);
    eq(menus[0][1], name);
    eq([menus[0][2], menus[0][3]], [30, 40]);
  });
  name.click();
  test("the click a finger makes as it lifts after a long press is swallowed", () => {
    eq(s.seen.filter(([t]) => t === "click").length, 0);
  });

  // Nothing here has a menu: a Take button held a moment too long.
  s.menus = false;
  s.seen.length = 0;
  finger(name, "pointerdown", 30, 40);
  await tick(PRESS + 30);
  finger(name, "pointerup", 30, 40);
  name.click();
  test("a long press nothing answers leaves the click alone, so a held Take still takes", () => {
    eq(s.seen.map(([t]) => t), ["contextmenu", "click"]);
  });
  s.menus = true;

  s.seen.length = 0;
  finger(name, "pointerdown", 30, 40);
  finger(name, "pointermove", 30, 40 + SLOP + 2);
  await tick(PRESS + 30);
  finger(name, "pointerup", 30, 40 + SLOP + 2);
  test("a finger that moves is scrolling, and no menu opens", () => eq(s.seen.length, 0));

  finger(name, "pointerdown", 30, 40, { pointerType: "mouse" });
  await tick(PRESS + 30);
  finger(name, "pointerup", 30, 40, { pointerType: "mouse" });
  test("a mouse held down is not a long press", () => eq(s.seen.length, 0));

  for (const target of [s.field, s.handle]) {
    finger(target, "pointerdown", 30, 40);
    await tick(PRESS + 30);
    finger(target, "pointerup", 30, 40);
  }
  test("a text field and a drag grip keep their own long press", () => {
    eq(s.seen.length, 0);
    ok(ignored(s.field) && ignored(s.handle), "the field and the grip are ignored");
    ok(!ignored(el("input", { type: "range" })), "a fader is not text, so a double tap can reset it");
  });

  await tick(400);
  tap(name);
  tap(name, 24, 22);
  await tick(10);
  test("two taps close together are a dblclick on the tile", () => {
    const dbl = s.seen.filter(([t]) => t === "dblclick");
    eq(dbl.length, 1);
    eq(dbl[0][1], name);
  });

  s.seen.length = 0;
  await tick(400);
  tap(name);
  await tick(400);
  tap(name);
  await tick(10);
  test("two taps a moment apart are two taps", () => eq(s.seen.filter(([t]) => t === "dblclick").length, 0));
  s.done();
}

export async function touchTests(test, eq, ok) {
  await gestures(test, eq, ok);
  await trayTests(test, eq, ok, { stage, finger, tap, tick, PRESS });
}
