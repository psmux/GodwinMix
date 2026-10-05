// A finger on the page: a long press is the item menu, a double tap opens,
// a scroll is a scroll, and a tile moves by its grip. Synthetic pointer events
// against a detached root, so nothing here needs a touch screen.

import { watchTouch, ignored, SLOP } from "../shell/touch.js";
import { DragSelect, dragHandle } from "../shell/pointer.js";
import { Selection } from "../shell/selection.js";
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
  for (const type of ["contextmenu", "dblclick", "click"]) root.addEventListener(type, (e) => seen.push([type, e.target, e.clientX, e.clientY]));
  const stop = watchTouch(root, { longPress: PRESS });
  return { root, tile, field, handle, seen, done: () => { stop(); root.remove(); } };
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

/** The tray's pointer pipeline under a finger. */
async function tray(test, eq, ok) {
  const s = stage();
  const calls = { activate: 0, menu: 0, drops: [] };
  const drag = new DragSelect({
    container: s.root,
    selection: new Selection(),
    order: () => ["cam-1"],
    onChange: () => {},
    onActivate: () => { calls.activate += 1; },
    onMenu: () => { calls.menu += 1; },
    onDrop: (info) => calls.drops.push(info),
  });
  const name = s.tile.querySelector(".name");

  finger(name, "pointerdown", 30, 40);
  await tick(PRESS + 30);
  finger(name, "pointerup", 30, 40);
  test("a long press on a tile opens its menu and does not put it on air", () => {
    eq(calls.menu, 1);
    eq(calls.activate, 0);
  });

  await tick(400);
  finger(name, "pointerdown", 30, 40);
  finger(name, "pointermove", 30, 70);
  finger(name, "pointerup", 30, 70);
  test("a finger dragged across a tile scrolls rather than moves it", () => {
    eq(calls.drops.length, 0);
    eq(calls.activate, 0);
    ok(!document.querySelector(".dragging-tiles"), "no drag left behind");
  });

  await tick(400);
  finger(s.handle, "pointerdown", 10, 40);
  finger(s.handle, "pointermove", 60, 90);
  finger(s.handle, "pointerup", 60, 90);
  test("the grip drags the tile", () => {
    eq(calls.drops.length, 1);
    eq(calls.drops[0].ids, ["cam-1"]);
  });

  await tick(400);
  tap(s.handle, 10, 40);
  test("a tap on the grip selects without taking", () => eq(calls.activate, 0));

  tap(name, 30, 40);
  test("a tap on the tile still takes it", () => eq(calls.activate, 1));
  drag.destroy();
  s.done();
}

export async function touchTests(test, eq, ok) {
  await gestures(test, eq, ok);
  await tray(test, eq, ok);
}
