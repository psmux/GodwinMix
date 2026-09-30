// What is in Studio mode's preview, and what the pane says about it.
//
// The armed scene wins; with none, a source armed on this page. A scene armed
// after a source clears the source, since arming a source disarmed the scene
// and anything armed since is newer.

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
  const missing = notRunning(panel, scene);
  panel.previewNote.hidden = !missing.length;
  panel.previewNote.textContent = missing.length
    ? `${missing.join(", ")} ${missing.length === 1 ? "is" : "are"} in this scene but not running, so the mixer will not take it. ` +
      "Get them running, or open the scene with the pencil beside its tab and delete them."
    : "";
}

/**
 * The sources an armed scene draws that this mixer does not have.
 *
 * The core refuses to take such a scene, and says so only once Take is
 * pressed; the preview draws the scene without them, so without this note
 * the picture looks ready when it is not. Read from the Scenes panel, when
 * one is on the page, as the Sources panel does.
 */
function notRunning(panel, scene) {
  const scenes = scene && sceneKit();
  const summary = scenes ? scenes.summary(scene) : null;
  if (!summary || !summary.sources || !panel.client.store) return [];
  const sources = panel.client.state.sources || [];
  if (!sources.length) return [];
  return summary.sources.filter((id) => !panel.client.store.source(id));
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
