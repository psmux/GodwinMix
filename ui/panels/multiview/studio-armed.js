// What is in Studio mode's preview, and what the pane says about it.
//
// studio-next.js decides what Preview holds; this keeps the page in step: a
// scene armed after a source clears the source, since arming a source
// disarmed the scene and anything armed since is newer, and the label, the
// Take and Cut buttons, the missing sources note and the words over the
// picture all follow the answer. `body.dataset.chosen` is the scene picked
// in the Scenes panel, kept until the programme moves.

import { fillNote, sceneNotRunning } from "../scenes/fix-note.js";
import { sceneKit, armedScene as armedName, follow, nextUp, whyText } from "./studio-next.js";
import { paintSlots } from "./studio-slots.js";
import { settings } from "../../shell/settings.js";

export { sceneKit };

/** The armed scene's name, for the callers that only want that. */
export function armedScene(s) {
  return armedName(s, sceneKit(null));
}

/**
 * What is in preview, kept on the panel as `next`. Asked again by every
 * retune as well as every render, so a change the core announced between
 * the two is never drawn a frame late.
 */
export function resolve(panel, s) {
  const kit = sceneKit(panel);
  const scene = armedName(s, kit);
  if (scene && scene !== panel.lastPreview) delete document.body.dataset.armed;
  panel.lastPreview = scene;
  // The scene picked in the Scenes panel stops counting once a take moves
  // the programme, so Preview goes back to suggesting the next shot.
  const was = panel.onAirKey;
  follow(panel, s, kit);
  const after = settings().afterTake;
  if (was !== undefined && was !== panel.onAirKey) {
    delete document.body.dataset.chosen;
    if (after !== "swap") keepTaken(panel, s, kit, scene);
  }
  const next = nextUp(panel, s, kit, document.body.dataset.armed || null, document.body.dataset.chosen || null, after);
  panel.next = next;
  // Kept for whoever reads the old names: the take target and the armed scene.
  panel.armed = next ? next.id : null;
  panel.armedScene = next && next.kind === "scene" && next.why === "armed" ? next.name : null;
  return next;
}

/**
 * A take moved the programme: Preview keeps what was taken. A source armed
 * here gives way, and a scene put on air that is not the armed one (a take by
 * name, from a Stream Deck or another desk) is armed, so the core draws it
 * with its filters. Every page in this mode asks for the same thing.
 */
function keepTaken(panel, s, kit, armed) {
  const live = panel.onAirLive;
  if (document.body.dataset.armed && document.body.dataset.armed !== s.program) delete document.body.dataset.armed;
  const summary = live && kit ? kit.summary(live) : null;
  if (!summary || (armed && kit.summary(armed) && kit.summary(armed).id === summary.id)) return;
  if (panel.client && panel.client.call) panel.client.call("scene.preview.set", { scene: summary.id }).catch(() => {});
}

/** The pane drawn for what is in preview. */
export function render(panel, s) {
  const next = resolve(panel, s);
  const kit = sceneKit(panel);
  if (!panel.takeBar) return;
  panel.previewName.textContent = next ? next.name : "Nothing to preview";
  panel.previewWhy.textContent = whyText(next);
  panel.previewWhy.hidden = !panel.previewWhy.textContent;
  panel.previewWrap.classList.toggle("unarmed", !next || next.why !== "armed");
  panel.takeBtn.disabled = panel.cutBtn.disabled = !next;
  paintSlots(panel, s, kit, next);
  // The take goes ahead without them, so the note says so while there is
  // still time to fix it, and its button opens the dialog that does.
  const summary = next && next.kind === "scene" && kit ? kit.summary(next.id) : null;
  const missing = summary ? sceneNotRunning(panel.client, kit, summary.id) : [];
  fillNote(panel.previewNote, panel.client, missing,
    () => ({ client: panel.client, scenes: kit, scene: summary.id }),
    `The take goes ahead without ${missing.length === 1 ? "it" : "them"}.`);
}
