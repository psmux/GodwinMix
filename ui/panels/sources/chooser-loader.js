import { lazyAction } from '../../shell/lazy-action.js';
import { el } from '../../shell/dom.js';

export const openSceneSources = lazyAction(() => import('./chooser.js').then(m => m.openSceneSources), 'Open scene sources');

export function addSourceTile(onclick) {
  return el('button.source-add-tile', { onclick, title: 'Add sources to this scene', 'aria-label': 'Add sources to this scene' }, [
    el('span.source-add-icon', { text: '+', 'aria-hidden': 'true' }),
    el('span', { text: 'Add sources' }),
  ]);
}
