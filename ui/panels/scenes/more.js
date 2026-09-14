// The parts of the Scenes panel that are not the two minute path.
//
// 07 Phase 3 asks that somebody who has never used a mixer sweeps three inputs,
// drags them into a scene, renames it, colours it and puts it on air, with no
// instruction, in under two minutes. Everything that path needs is in panel.js
// and is fetched with the page. Copying a layout between scenes, duplicating,
// the right click menu and moving an item from one scene to another are all
// second visit work, so they arrive on the first right click or the first
// Ctrl+C and cost a page that never does either of those things nothing.
//
// Every function here takes the panel. It is a module rather than a mixin
// because there is no state to carry: the panel owns the state, this owns the
// verbs.

import { contextMenu } from "../../shell/menu.js";
import { toast, errorToast } from "../../shell/toast.js";
import { settings } from "../../shell/settings.js";

export function menu(panel, id, e) {
  const ids = panel.selected();
  const many = ids.length > 1;
  contextMenu(e.clientX, e.clientY, [
    id && { label: settings().producer ? "Arm" : "Put on air", key: "Click", run: () => panel.activate(id) },
    id && { label: "Open the composer", key: "Enter", disabled: many, run: () => panel.open(id) },
    id && { label: "Rename", key: "F2", disabled: many, run: () => panel.beginRename(id) },
    id && { kind: "colours", onColour: (colour) => panel.setColour(ids, colour) },
    id && { kind: "separator" },
    id && { label: "Copy the layout", disabled: many, run: () => copyLayout(panel, id) },
    id && { label: "Paste the layout onto this", disabled: !panel.layoutClip, run: () => pasteLayout(panel, ids) },
    id && { label: many ? `Duplicate ${ids.length}` : "Duplicate", run: () => duplicate(panel, ids) },
    id && { kind: "separator" },
    id && { label: many ? `Remove ${ids.length}` : "Remove", key: "Delete", run: () => panel.remove(ids) },
    { kind: "separator" },
    { label: "New scene", run: () => panel.newScene() },
  ].filter(Boolean));
}

/**
 * The geometry of one scene, held on this device until it is pasted.
 *
 * It is the scene's own shape rather than a reference to it, so pasting still
 * works after somebody else has changed the scene it came from.
 */
export async function copyLayout(panel, id) {
  try {
    panel.layoutClip = await panel.scenes.layoutCopy(id);
    const name = (panel.scenes.summary(id) || {}).name || id;
    toast({ text: `Copied the layout of "${name}". Select another scene and paste the layout onto it.` });
  } catch (e) {
    errorToast(e, "Copy layout");
  }
}

/**
 * Items are matched by name first and slot order second, and anything that
 * matches nothing is left exactly as it was. That is what makes this "copy the
 * design from one scene to another" rather than "replace that scene".
 */
export async function pasteLayout(panel, ids) {
  if (!panel.layoutClip || !ids.length) {
    toast({ text: "Copy a layout first: right click the scene it should come from." });
    return;
  }
  for (const id of ids) {
    try {
      await panel.scenes.layoutPaste(id, panel.layoutClip.layout || panel.layoutClip, "name");
    } catch (e) {
      errorToast(e, "Paste layout");
      return;
    }
  }
  panel.scenes.undo.record("Pasted a layout", { offer: true });
  await panel.scenes.reread(ids);
  toast({ text: "Items with matching names moved; the rest were left alone." });
}

export async function duplicate(panel, ids) {
  for (const id of ids) {
    try {
      await panel.scenes.duplicate(id);
    } catch (e) {
      errorToast(e, "Duplicate");
      return;
    }
  }
  panel.scenes.undo.record(ids.length === 1 ? "Duplicated a scene" : `Duplicated ${ids.length} scenes`);
  await panel.scenes.refresh();
}

export function copy(panel, ids) {
  if (!ids.length) return;
  panel.clipboard = ids.slice();
  toast({ text: ids.length === 1 ? "Copied a scene. Ctrl+V pastes a duplicate." : `Copied ${ids.length} scenes.` });
}

export function paste(panel) {
  if (panel.clipboard && panel.clipboard.length) return duplicate(panel, panel.clipboard);
  if (panel.layoutClip) return pasteLayout(panel, panel.selected());
  return Promise.resolve();
}

/** An item dragged from one scene tile onto another. A modifier copies it. */
export async function moveItem(panel, info, toScene) {
  if (info.scene === toScene) return;
  try {
    await panel.scenes.itemMove(info.scene, info.item, toScene, info.copy);
    panel.scenes.undo.record(info.copy ? "Copied an item into another scene" : "Moved an item to another scene", { offer: true });
  } catch (e) {
    errorToast(e, info.copy ? "Copy" : "Move");
  }
}
