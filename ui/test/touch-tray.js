// The tray's pointer pipeline under a finger: a long press is the tile's menu
// and never a take, a swipe scrolls, the grip drags. The finger and the stage
// come from touch.js, which runs this after its own gestures.

import { DragSelect } from "../shell/pointer.js";
import { Selection } from "../shell/selection.js";

export async function trayTests(test, eq, ok, { stage, finger, tap, tick, PRESS }) {
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
