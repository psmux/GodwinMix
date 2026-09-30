// Scene pictures: where a source lands in its box, and that the mosaic is asked
// for only while the Scenes panel is on screen with live pictures on.

import { placement, alignOf } from "../panels/scenes/draw.js";
import { settings, setSetting } from "../shell/settings.js";

function fakePanel(wants) {
  const listeners = new Set();
  const client = {
    state: { multiview: { enabled: true, cols: 3 } },
    want: (key, value) => {
      const held = { key, value, released: false, update(v) { held.value = v; }, release() { held.released = true; } };
      wants.push(held);
      return held;
    },
    sheet: { observe: (fn) => { listeners.add(fn); return () => listeners.delete(fn); } },
  };
  const panel = document.createElement("div");
  Object.assign(panel, {
    client,
    visible: true,
    workspaceActive: true,
    tabs: new Map([["a", document.createElement("button")]]),
    tiles: new Map(),
    grid: Object.assign(document.createElement("div"), { hidden: true }),
    strip: document.createElement("div"),
    scenes: {
      scenes: () => [{ id: "a", name: "A" }],
      view: () => ({ id: "a", records: [], geometry: [], canvas: { width: 1920, height: 1080 } }),
      mirror: { descendants: () => [], record: () => null },
      reread: async () => {},
    },
  });
  return { panel, listeners };
}

export async function scenePictureTests(test, eq, ok) {
  test("cover fills the box and takes the middle of the source", () => {
    eq(placement({ x: 0, y: 0, w: 160, h: 90 }, { x: 0, y: 0, w: 90, h: 90 }, "cover", "center"), [35, 0, 90, 90, 0, 0, 90, 90]);
  });
  test("contain fits inside the box and sits where align says", () => {
    eq(placement({ x: 0, y: 0, w: 160, h: 90 }, { x: 0, y: 0, w: 160, h: 180 }, "contain", "top-left"), [0, 0, 160, 90, 0, 0, 160, 90]);
    eq(alignOf("bottom-right"), { ax: 1, ay: 1 });
  });
  test("stretch, and a fit nobody named, fill the box exactly", () => {
    eq(placement({ x: 5, y: 5, w: 10, h: 10 }, { x: 0, y: 0, w: 40, h: 20 }, "stretch"), [5, 5, 10, 10, 0, 0, 40, 20]);
  });

  const before = settings().gallery;
  const { ScenePictures } = await import("../panels/scenes/pictures.js");
  const wants = [];
  const { panel, listeners } = fakePanel(wants);
  const pictures = new ScenePictures(panel);
  setSetting("gallery", "icon");
  pictures.tune();
  test("on icons the scenes ask the mixer for nothing", () => eq(wants.length, 0));
  setSetting("gallery", "live");
  pictures.tune();
  test("on screen with live pictures, one small slow mosaic and a canvas per tab", () => {
    eq(wants.length, 1);
    eq(wants[0].value.fps, 3);
    ok(wants[0].value.width <= 784, `asked for ${wants[0].value.width} wide`);
    eq(listeners.size, 1);
    ok(panel.tabs.get("a").querySelector("canvas"), "no canvas on the tab");
  });
  panel.visible = false;
  pictures.tune();
  test("scrolled away, the mosaic is given back", () => {
    ok(wants[0].released, "still held");
    eq(listeners.size, 0);
  });
  panel.visible = true;
  panel.workspaceActive = false;
  pictures.tune();
  test("behind another tab, nothing is asked for", () => eq(wants.filter((w) => !w.released).length, 0));
  pictures.destroy();
  setSetting("gallery", before);
}
