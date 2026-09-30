// What is in Studio mode's preview, and what the pane says about it.
//
// The armed scene wins; with none, a source armed on this page. A scene armed
// after a source clears the source, since arming a source disarmed the scene
// and anything armed since is newer.

/** What is in preview: the armed scene, else a source armed on this page. */
export function render(panel, s) {
  if (s.preview && s.preview !== panel.lastPreview) delete document.body.dataset.armed;
  panel.lastPreview = s.preview || null;
  const source = s.preview ? null : document.body.dataset.armed || null;
  panel.armed = s.preview || source;
  if (!panel.takeBar) return;
  const known = source && panel.client.store && panel.client.store.source(source);
  panel.previewName.textContent = (known && known.name) || panel.armed || "Click a scene or a source";
  panel.previewWrap.classList.toggle("unarmed", !panel.armed);
  panel.takeBtn.disabled = panel.cutBtn.disabled = !panel.armed;
  const missing = notRunning(panel, s.preview);
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
  const node = scene && document.querySelector("gmx-scenes");
  const summary = node && node.scenes && node.scenes.supported ? node.scenes.summary(scene) : null;
  if (!summary || !summary.sources || !panel.client.store) return [];
  const sources = panel.client.state.sources || [];
  if (!sources.length) return [];
  return summary.sources.filter((id) => !panel.client.store.source(id));
}
