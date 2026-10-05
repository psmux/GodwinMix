// Other people in the same scene, seen from the composer.
//
// Opening the composer tells the core which scene this page is editing
// (`presence.set`), and closing it says none. While it is open, a marker in
// its bar names anybody else who said the same scene, so two people do not
// lay one scene out against each other without knowing.
//
// And when they did anyway: Apply over a scene that changed since the draft
// was taken is refused by the core, and this asks what to do about it.

import { el } from "../../shell/dom.js";
import { toast, errorToast } from "../../shell/toast.js";
import { onPresence, tellEditing, watchPresence } from "../../shell/presence.js";
import { editing, sentence } from "../../kits/protocol/presence.js";

/** The marker, and `stop()` for when the composer closes. */
export function othersHere(client, scene) {
  watchPresence(client);
  tellEditing(client, scene);
  const mark = el("span.pill.live.sm", { hidden: true, role: "status" });
  const off = onPresence((list) => {
    const here = editing(list, scene);
    mark.hidden = here.length === 0;
    mark.textContent = here.length
      ? `${sentence(here)} ${here.length === 1 ? "is" : "are"} editing this scene too`
      : "";
  });
  return {
    el: mark,
    stop() {
      off();
      tellEditing(client, null);
    },
  };
}

/**
 * Apply was refused because the scene moved on. Ask, then apply over it, or
 * start again from the scene as it is, or leave the person editing.
 */
export async function staleDraft(composer, err) {
  const { askAboutStaleDraft } = await import("../../shell/conflicts.js");
  const choice = await askAboutStaleDraft(err);
  if (choice === "force") return composer.apply(true);
  if (choice !== "reopen") return;
  try {
    await composer.reopen();
    toast({ text: "Started again from the scene as it is now. Your changes were dropped." });
  } catch (e) {
    errorToast(e, "Start again");
  }
}
