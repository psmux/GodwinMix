import { addView, dropViews } from '../../shell/meter.js';

/** Preserve tile selection and controls while releasing work for a hidden tab. */
export function setWorkspaceActive(panel, active) {
  panel.workspaceActive = active;
  if (!active) {
    panel.release();
    if (panel.positionWant) panel.positionWant.release();
    panel.positionWant = null;
    clearInterval(panel.stillTimer);
    clearTimeout(panel.sceneTimer);
    panel.stillTimer = panel.sceneTimer = null;
    dropViews('tile:');
    return;
  }
  panel.visible = panel.getBoundingClientRect().height > 0;
  panel.render(panel.client.state);
  for (const source of panel.sources(panel.client.state)) {
    const tile = panel.tiles.get(source.id);
    if (tile && source.has_audio !== false) addView('tile:' + source.id, 'src:' + source.id, tile.meter, 'v');
  }
  panel.refreshStills(true);
}
