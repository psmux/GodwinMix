// What Sources does to a scene's items that the first page does not need:
// turning a source and choosing how it fills its box, from its tile's menu,
// and taking the scene when its lone on air source is removed from it.
// Fetched on first use, because the first page has a byte budget.
//
// Turn and fit:
// A phone held sideways, a camera mounted on its side, a portrait clip in a
// wide show: each wants a quarter turn or a different fit, and both used to
// live only in the composer's inspector, three clicks and a double click
// away. Here they are on the tile, for the scene the tray is showing. They
// change that scene's item, so the same camera can sit upright in one scene
// and on its side in another, and they apply on air at once like any edit.

import { errorToast, toast } from "../../shell/toast.js";

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

/**
 * A source on air by itself, taken out of the scene a person is building,
 * goes off air: the scene goes on in its place.
 *
 * A tester removed "Browser jaffer" from Default scene and the header went on
 * saying Browser jaffer, because the programme was that source alone and not
 * the scene. To them the two were the same thing. Studio mode too, where a
 * take is otherwise the operator's alone: the source the person has just
 * taken out is the one thing they asked to be rid of.
 */
export async function followScene(client, scenes, scene, ids) {
  const state = client.state || {};
  if (state.scene || !ids.includes(state.program)) return;
  const name = client.store.source(state.program)?.name || state.program;
  await scenes.take(scene.id);
  toast({ text: `${name} was on air by itself, so ${scene.name} is on air now.` });
}
