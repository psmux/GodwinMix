import { el, clear } from '../../shell/dom.js';
import { modal } from '../../shell/modal.js';
import { errorToast } from '../../shell/toast.js';
import { openPicker } from '../../shell/picker-loader.js';
import { nameOf } from './local.js';
import { sourcePreview } from './chooser-preview.js';

/** Reuse a mixer source in a scene without creating another capture pipeline. */
export function openSceneSources(client, scenes, scene) {
  const preview = sourcePreview(client);
  const search = el('input', { type: 'search', placeholder: 'Find a source', 'aria-label': 'Find an available source' });
  const list = el('div.source-chooser-list', { role: 'list', 'aria-label': 'Available sources' });
  const added = new Set(scene.sources || []);
  const pending = new Set();
  let signature = '', closed = false;
  const add = async source => {
    if (pending.has(source.id) || added.has(source.id)) return;
    pending.add(source.id);
    render(true);
    try {
      await scenes.itemAdd(scene.id, { source: source.id });
      added.add(source.id);
      scenes.undo.record(`Added ${nameOf(source)} to ${scene.name}`);
      await scenes.reread([scene.id]);
    } finally {
      pending.delete(source.id);
      if (!closed) render(true);
    }
  };
  function row(source) {
    const present = added.has(source.id);
    return el('div.source-choice', { role: 'listitem' }, [
      el('div.grow', {}, [el('strong', { text: nameOf(source) }), el('div.sm.dim', { text: source.type || source.state || source.id })]),
      el('button.btn', { text: 'Preview', 'aria-label': `Preview ${nameOf(source)}`, onclick: () => { preview.select(source); preview.node.scrollIntoView({ block: 'nearest', behavior: 'smooth' }); } }),
      el('button.btn.primary', { text: pending.has(source.id) ? 'Adding…' : present ? 'In scene' : 'Add',
        'aria-label': `${present ? 'Already in scene:' : 'Add'} ${nameOf(source)}`, disabled: present || pending.has(source.id),
        onclick: () => add(source).catch(error => errorToast(error, `Add to ${scene.name}`)) }),
    ]);
  }
  function render(force = false) {
    if (closed) return;
    const sources = client.state.sources || [];
    const next = sources.map(s => `${s.id}/${nameOf(s)}/${s.state}`).join('|') + search.value;
    if (!force && next === signature) return;
    signature = next;
    const query = search.value.trim().toLowerCase();
    const shown = sources.filter(s => `${nameOf(s)} ${s.id} ${s.type || ''}`.toLowerCase().includes(query));
    clear(list);
    list.append(...shown.map(row));
    if (!shown.length) list.append(el('p.dim', { text: query ? 'No matching sources.' : 'No sources yet. Create one to add it to this scene.' }));
  }
  search.oninput = () => render(true);
  const create = el('button.btn.primary', { text: 'Create new source', onclick: () => {
    dialog.close();
    openPicker(client, 'source', { onAdded: source => add(source).catch(error => errorToast(error, `Source created, but could not add it to ${scene.name}. Open Add sources to retry`)) });
  } });
  const body = el('div.source-chooser', {}, [
    el('div.col.source-chooser-library', {}, [search, list]), preview.node,
  ]);
  const off = client.onRender(() => render());
  const dialog = modal({ title: `Add sources to ${scene.name}`, wide: true, body,
    footer: [create, el('button.btn', { text: 'Done', onclick: () => dialog.close() })],
    onClose: () => { closed = true; off(); preview.destroy(); },
  });
  render(true);
  search.focus();
  return dialog;
}
