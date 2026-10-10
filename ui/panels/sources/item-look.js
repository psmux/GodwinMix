// Turn a source and choose how it fills its box, from its tile's menu.
//
// A phone held sideways, a camera mounted on its side, a portrait clip in a
// wide show: each wants a quarter turn or a different fit, and both used to
// live only in the composer's inspector, three clicks and a double click
// away. Here they are on the tile, for the scene the tray is showing. They
// change that scene's item, so the same camera can sit upright in one scene
// and on its side in another, and they apply on air at once like any edit.

import { errorToast } from "../../shell/toast.js";

const quarter = (deg) => ((Math.round(deg / 90) * 90) % 360 + 360) % 360;

/** The items in `scene` that draw source `id`. */
function itemsOf(scenes, scene, id) {
  return scenes.mirror.descendants(scene.id).filter((item) => item.content?.source === id);
}

async function setAll(scenes, scene, items, transform, what) {
  try {
    for (const item of items) await scenes.itemSet(scene.id, item.id, { transform: transform(item) }, { duration_ms: 0 });
    scenes.undo.record(what, { offer: true });
  } catch (e) {
    errorToast(e, what);
  }
}

/** Menu entries for source `id` in `scene`, or none when the scene does not draw it. */
export function lookEntries(scenes, scene, id) {
  if (!scenes || !scene || !id) return [];
  const items = itemsOf(scenes, scene, id);
  if (!items.length) return [];
  const t = items[0].transform || {};
  const turn = (by, what) => () =>
    setAll(scenes, scene, items, (item) => ({ rotation: quarter(((item.transform && item.transform.rotation) || 0) + by) }), what);
  const fit = (value, what) => () => setAll(scenes, scene, items, () => ({ fit: value }), what);
  const turned = quarter(t.rotation || 0);
  return [
    { kind: "separator" },
    { label: "Rotate right", run: turn(90, "Rotate right") },
    { label: "Rotate left", run: turn(-90, "Rotate left") },
    turned && { label: "Upright again", run: turn(-turned, "Upright again") },
    t.fit === "cover"
      ? { label: "Show all of it", run: fit("contain", "Show all of it") }
      : { label: "Fill the box", run: fit("cover", "Fill the box") },
  ].filter(Boolean);
}
