// What is in Studio mode's preview, and what the pane says about it.
//
// The armed scene wins; with none, a source armed on this page. A scene armed
// after a source clears the source, since arming a source disarmed the scene
// and anything armed since is newer.

import { fillNote, sceneNotRunning } from "../scenes/fix-note.js";

/** What is in preview: the armed scene, else a source armed on this page. */
export function render(panel, s) {
  const scene = armedScene(s);
  if (scene && scene !== panel.lastPreview) delete document.body.dataset.armed;
  panel.lastPreview = scene;
  const source = scene ? null : document.body.dataset.armed || null;
  panel.armed = scene || source;
  panel.armedScene = scene;
  if (!panel.takeBar) return;
  const known = source && panel.client.store && panel.client.store.source(source);
  panel.previewName.textContent = (known && known.name) || panel.armed || "Click a scene or a source";
  panel.previewWrap.classList.toggle("unarmed", !panel.armed);
  panel.takeBtn.disabled = panel.cutBtn.disabled = !panel.armed;
  // The take goes ahead without them, so the note says so while there is
  // still time to fix it, and its button opens the dialog that does. Read
  // from the Scenes panel, when one is on the page, as the Sources panel does.
  const kit = scene ? sceneKit() : null;
  const summary = kit ? kit.summary(scene) : null;
  const missing = summary ? sceneNotRunning(panel.client, kit, summary.id) : [];
  fillNote(panel.previewNote, panel.client, missing,
    () => ({ client: panel.client, scenes: kit, scene: summary.id }),
    `The take goes ahead without ${missing.length === 1 ? "it" : "them"}.`);
}

/**
 * The armed scene's name. The event says so when it happens, but the status
 * a page loads with has no field for it, so a page opened after the arming
 * reads it from the Scenes panel's list, as that panel's own tally does.
 */
export function armedScene(s) {
  if (s.preview) return s.preview;
  const scenes = sceneKit();
  const id = scenes && scenes.armed();
  const summary = id && scenes.summary(id);
  return summary ? summary.name : null;
}

function sceneKit() {
  const node = document.querySelector("gmx-scenes");
  return node && node.scenes && node.scenes.supported ? node.scenes : null;
}
