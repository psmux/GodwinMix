import { el } from './dom.js';
import { modal } from './modal.js';
import { list, get } from './registry.js';
import { leaves } from './dock-model.js';

export function workspaceMenu(workspace) {
  const hidden = new Set(workspace.state.hidden);
  const body = el('div.col', {}, list().filter(s => !s.slots.includes('header') && !s.slots.includes('modal')).map(spec =>
    el('button', { text: (hidden.has(spec.id) ? 'Show ' : 'Close ') + spec.title,
      onclick: () => { if (hidden.has(spec.id)) workspace.show(spec.id); else workspace.hide(spec.id); dialog.close(); },
    })));
  body.append(el('button', { text: 'Reset workspace', onclick: () => { workspace.reset(); dialog.close(); } }));
  const dialog = modal({ title: 'Panels and layout', body });
}
export function panelMenu(workspace, id) {
  const target = el('select', { 'aria-label': 'Destination panel' });
  for (const group of leaves(workspace.state.tree)) {
    for (const other of group.tabs) if (other !== id) target.append(el('option', { value: other, text: get(other)?.title || other }));
  }
  const edge = el('select', { 'aria-label': 'Dock position' }, ['left', 'right', 'top', 'bottom', 'center'].map(value =>
    el('option', { value, text: value === 'center' ? 'Group as tabs' : 'Split ' + value })));
  const body = el('div.col', {}, [el('label', { text: 'Destination panel' }, [target]), el('label', { text: 'Position' }, [edge]),
    el('button', { text: 'Move panel', disabled: !target.options.length,
      onclick: () => { workspace.move(id, target.value, edge.value); dialog.close(); } }),
  ]);
  const dialog = modal({ title: 'Move ' + (get(id)?.title || id), body });
}
